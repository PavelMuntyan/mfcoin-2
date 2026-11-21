import fs from "node:fs/promises";
import { blockNumber } from "./rpc.js";
import { finalityStatus } from "./finality.js";

export class FreelandIndexer {
  constructor(sql, rpc, { batchSize = 100, maximumFinalityLag = 6 } = {}) {
    this.sql = sql;
    this.rpc = rpc;
    this.batchSize = batchSize;
    this.maximumFinalityLag = maximumFinalityLag;
  }

  async init() {
    const schema = await fs.readFile(new URL("../schema.sql", import.meta.url), "utf8");
    await this.sql.unsafe(schema);
    const state = await this.state();
    if (!state) {
      const genesisHash = await this.rpc("chain_getBlockHash", [0]);
      const records = [];
      for (let offset = 0; ; offset += 1000) {
        const page = await this.rpc("freeland_blockRecordsPage", [offset, 1000, genesisHash]);
        records.push(...page);
        if (page.length < 1000) break;
      }
      const totalRecords = await this.rpc("freeland_totalRecords", [genesisHash]);
      if (Number(totalRecords) !== records.length) {
        throw new Error(`Genesis record count mismatch: ${totalRecords} != ${records.length}`);
      }
      if (records.length > 0) {
        const proof = await this.rpc("freeland_registryProof", [records[0].kind, records[0].subjectCommitment, genesisHash]);
        await this.sql.begin(async sql => {
          await sql`
            INSERT INTO freeland_block_info (
              block_hash, block_number, parent_hash, previous_info_root, records_root,
              registry_root, governance_root, records_count, epoch, validator_set_hash,
              info_root, status
            ) VALUES (
              ${genesisHash}, 0, ${genesisHash}, NULL, NULL, ${proof.root}, NULL,
              ${records.length}, 0, NULL, NULL, 'included'
            )
          `;
          for (const record of records) {
            await sql`
              INSERT INTO freeland_records (
                record_id, kind, action, subject_commitment, payload_hash, evidence_hash,
                issuer, nonce, block_hash, block_number, extrinsic_index, status
              ) VALUES (
                ${record.recordId}, ${record.kind}, ${record.action}, ${record.subjectCommitment},
                ${record.payloadHash}, ${record.evidenceHash}, ${record.issuer}, ${record.nonce},
                ${genesisHash}, 0, ${record.extrinsicIndex}, 'included'
              )
            `;
          }
        });
      }
      await this.sql`
        INSERT INTO indexer_state (id, best_hash, best_number, finalized_hash, finalized_number)
        VALUES (1, ${genesisHash}, 0, ${genesisHash}, 0)
      `;
    }
  }

  async state() {
    const rows = await this.sql`SELECT * FROM indexer_state WHERE id = 1`;
    return rows[0];
  }

  async sync() {
    const header = await this.rpc("chain_getHeader");
    const target = blockNumber(header);
    let synced = 0;
    while ((await this.state()).best_number < target && synced < this.batchSize) {
      await this.syncNext();
      synced += 1;
    }
    await this.updateFinality();
    return synced;
  }

  async syncNext() {
    for (;;) {
      const state = await this.state();
      const number = Number(state.best_number) + 1;
      const hash = await this.rpc("chain_getBlockHash", [number]);
      const block = await this.rpc("chain_getBlock", [hash]);
      const header = block.block.header;
      if (header.parentHash !== state.best_hash) {
        await this.rollback(header.parentHash, number - 1);
        continue;
      }

      const [info, records] = await Promise.all([
        this.rpc("freeland_latestBlockInfo", [hash]),
        this.rpc("freeland_blockRecords", [hash])
      ]);
      if (!info) throw new Error(`Missing FreelandBlockInfo at ${hash}`);
      if (info.recordsCount !== records.length) {
        throw new Error(`Record count mismatch at ${hash}: ${info.recordsCount} != ${records.length}`);
      }

      await this.sql.begin(async sql => {
        await sql`
          INSERT INTO freeland_block_info (
            block_hash, block_number, parent_hash, previous_info_root, records_root,
            registry_root, governance_root, records_count, epoch, validator_set_hash,
            info_root, status
          ) VALUES (
            ${hash}, ${number}, ${header.parentHash}, ${info.previousInfoRoot}, ${info.recordsRoot},
            ${info.registryRoot}, ${info.governanceRoot}, ${info.recordsCount}, ${info.epoch},
            ${info.validatorSetHash}, ${info.infoRoot}, 'included'
          )
        `;
        for (const record of records) {
          await sql`
            INSERT INTO freeland_records (
              record_id, kind, action, subject_commitment, payload_hash, evidence_hash,
              issuer, nonce, block_hash, block_number, extrinsic_index, status
            ) VALUES (
              ${record.recordId}, ${record.kind}, ${record.action}, ${record.subjectCommitment},
              ${record.payloadHash}, ${record.evidenceHash}, ${record.issuer}, ${record.nonce},
              ${hash}, ${number}, ${record.extrinsicIndex}, 'included'
            )
          `;
        }
        await sql`
          UPDATE indexer_state
          SET best_hash = ${hash}, best_number = ${number}, updated_at = now()
          WHERE id = 1
        `;
      });
      return;
    }
  }

  async rollback(candidateHash, candidateNumber) {
    for (;;) {
      const canonical = await this.sql`
        SELECT block_hash FROM freeland_block_info
        WHERE block_number = ${candidateNumber} AND status != 'orphaned'
        LIMIT 1
      `;
      if (canonical[0]?.block_hash === candidateHash) {
        await this.sql`
          UPDATE indexer_state
          SET best_hash = ${candidateHash}, best_number = ${candidateNumber}, updated_at = now()
          WHERE id = 1
        `;
        return;
      }

      await this.sql.begin(async sql => {
        await sql`
          UPDATE freeland_records SET status = 'orphaned'
          WHERE block_number = ${candidateNumber} AND status != 'orphaned'
        `;
        await sql`
          UPDATE freeland_block_info SET status = 'orphaned'
          WHERE block_number = ${candidateNumber} AND status != 'orphaned'
        `;
      });
      if (candidateNumber === 0) {
        await this.sql`
          UPDATE indexer_state
          SET best_hash = ${candidateHash}, best_number = 0, updated_at = now()
          WHERE id = 1
        `;
        return;
      }
      const block = await this.rpc("chain_getBlock", [candidateHash]);
      candidateHash = block.block.header.parentHash;
      candidateNumber -= 1;
    }
  }

  async updateFinality() {
    const hash = await this.rpc("chain_getFinalizedHead");
    const header = await this.rpc("chain_getHeader", [hash]);
    const number = blockNumber(header);
    await this.sql.begin(async sql => {
      await sql`
        UPDATE freeland_records SET status = 'finalized'
        WHERE block_number <= ${number} AND status = 'included'
      `;
      await sql`
        UPDATE freeland_block_info SET status = 'finalized'
        WHERE block_number <= ${number} AND status = 'included'
      `;
      await sql`
        UPDATE indexer_state
        SET finalized_hash = ${hash}, finalized_number = ${number}, updated_at = now()
        WHERE id = 1
      `;
    });
  }

  async finality() {
    const state = await this.state();
    return finalityStatus(state.best_number, state.finalized_number, this.maximumFinalityLag);
  }
}
