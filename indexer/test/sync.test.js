import test from "node:test";
import assert from "node:assert/strict";
import postgres from "postgres";
import { FreelandIndexer } from "../src/indexer.js";
import { createApiServer } from "../src/server.js";
import { assertFinalityHealthy } from "../src/finality.js";

const databaseUrl = process.env.DATABASE_URL ||
  "postgres://mfcoin_indexer:indexer_dev_only@127.0.0.1:55432/mfcoin_indexer_dev";
const subject = `0x${"11".repeat(32)}`;

function record(action = "issue") {
  return {
    recordId: `0x${action === "issue" ? "aa" : "bb"}`.padEnd(66, "0"),
    kind: "passport",
    action,
    subjectCommitment: subject,
    payloadHash: `0x${"22".repeat(32)}`,
    evidenceHash: `0x${"33".repeat(32)}`,
    issuer: "alice",
    nonce: action === "issue" ? 0 : 1,
    extrinsicIndex: 1
  };
}

function block(hash, parentHash, number, records = []) {
  hash = hash.padEnd(66, "0");
  parentHash = parentHash.padEnd(66, "0");
  return {
    hash,
    parentHash,
    number,
    info: {
      previousInfoRoot: `0x${"01".repeat(32)}`,
      recordsRoot: `0x${"02".repeat(32)}`,
      registryRoot: `0x${"03".repeat(32)}`,
      governanceRoot: `0x${"00".repeat(32)}`,
      recordsCount: records.length,
      epoch: 0,
      validatorSetHash: `0x${"00".repeat(32)}`,
      infoRoot: `0x${"04".repeat(32)}`
    },
    records
  };
}

class MockChain {
  constructor(blocks, bestHash, finalizedHash) {
    this.blocks = new Map(blocks.map(block => [block.hash, block]));
    this.bestHash = bestHash;
    this.finalizedHash = finalizedHash;
    this.canonical = new Map(blocks.map(block => [block.number, block.hash]));
    this.rpc = this.rpc.bind(this);
  }

  header(block) {
    return { number: `0x${block.number.toString(16)}`, parentHash: block.parentHash };
  }

  async rpc(method, params = []) {
    if (method === "chain_getHeader") {
      const block = this.blocks.get(params[0] || this.bestHash);
      return this.header(block);
    }
    if (method === "chain_getBlockHash") return this.canonical.get(params[0]);
    if (method === "chain_getBlock") {
      const block = this.blocks.get(params[0]);
      return { block: { header: this.header(block), extrinsics: [] } };
    }
    if (method === "chain_getFinalizedHead") return this.finalizedHash;
    if (method === "freeland_latestBlockInfo") return this.blocks.get(params[0]).info;
    if (method === "freeland_blockRecords") return this.blocks.get(params[0]).records;
    if (method === "freeland_blockRecordsPage") {
      const [offset, limit, hash] = params;
      return this.blocks.get(hash).records.slice(offset, offset + limit);
    }
    if (method === "freeland_totalRecords") return this.blocks.get(params[0]).records.length;
    if (method === "freeland_registryProof") {
      return { kind: params[0], subjectCommitment: params[1], entry: null, root: `0x${"05".repeat(32)}`, siblings: [] };
    }
    throw new Error(`Unexpected RPC method ${method}`);
  }
}

test("sync preserves history across reorg and reports finality delay", async () => {
  const sql = postgres(databaseUrl, { max: 4 });
  const genesis = block("0x00", "0x00", 0);
  const a1 = block("0xa1", genesis.hash, 1, [record()]);
  const a2 = block("0xa2", a1.hash, 2);
  const a3 = block("0xa3", a2.hash, 3, [record("update")]);
  const chain = new MockChain([genesis, a1, a2, a3], a3.hash, a1.hash);
  const indexer = new FreelandIndexer(sql, chain.rpc, { maximumFinalityLag: 2 });
  await indexer.init();
  await sql`TRUNCATE freeland_records, freeland_block_info, indexer_state RESTART IDENTITY CASCADE`;
  await indexer.init();
  assert.equal(await indexer.sync(), 3);
  assert.equal((await indexer.finality()).status, "healthy");

  const b2 = block("0xb2", a1.hash, 2);
  const b3 = block("0xb3", b2.hash, 3);
  const b4 = block("0xb4", b3.hash, 4, [record("update")]);
  chain.blocks.set(b2.hash, b2);
  chain.blocks.set(b3.hash, b3);
  chain.blocks.set(b4.hash, b4);
  chain.canonical.set(2, b2.hash);
  chain.canonical.set(3, b3.hash);
  chain.canonical.set(4, b4.hash);
  chain.bestHash = b4.hash;
  assert.equal(await indexer.sync(), 3);

  const oldBlocks = await sql`
    SELECT block_hash, status FROM freeland_block_info
    WHERE block_hash IN (${a2.hash}, ${a3.hash}) ORDER BY block_hash
  `;
  assert.deepEqual(oldBlocks.map(row => row.status), ["orphaned", "orphaned"]);
  const oldRecords = await sql`
    SELECT status FROM freeland_records WHERE block_hash = ${a3.hash}
  `;
  assert.equal(oldRecords[0].status, "orphaned");
  const canonical = await sql`
    SELECT block_hash, status FROM freeland_block_info
    WHERE block_number IN (2, 3, 4) AND status != 'orphaned' ORDER BY block_number
  `;
  assert.deepEqual(canonical.map(row => row.block_hash), [b2.hash, b3.hash, b4.hash]);

  const server = createApiServer(sql, chain.rpc, indexer);
  await new Promise(resolve => server.listen(0, "127.0.0.1", resolve));
  const port = server.address().port;
  const orphaned = await fetch(`http://127.0.0.1:${port}/freeland/records/${subject}?status=orphaned`).then(response => response.json());
  assert.ok(orphaned.length >= 1);
  const proof = await fetch(`http://127.0.0.1:${port}/freeland/proofs/passport/${subject}?at=${b4.hash}`).then(response => response.json());
  assert.equal(proof.status, "included");
  await new Promise(resolve => server.close(resolve));

  const b5 = block("0xb5", b4.hash, 5);
  const b6 = block("0xb6", b5.hash, 6);
  chain.blocks.set(b5.hash, b5);
  chain.blocks.set(b6.hash, b6);
  chain.canonical.set(5, b5.hash);
  chain.canonical.set(6, b6.hash);
  chain.bestHash = b6.hash;
  await indexer.sync();
  assert.equal((await indexer.finality()).status, "finality_delayed");
  assert.throws(() => assertFinalityHealthy(6, 1, 2), error => error.code === "finality_delayed");
  await sql.end();
});
