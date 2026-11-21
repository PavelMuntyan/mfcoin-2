# MFCoin 2.0 — Blockchain

Public development repository for the new MFCoin blockchain architecture.

## Status

Contour 14: verifiable GRANDPA participation evidence. Validator reward distribution is not yet activated.

The current workspace is based on the standalone Polkadot SDK Solochain Template, branch `update-template/stable2512`, upstream commit `1adedaffe70df1dab07e34e6a57c19e732ececb9`.

The project is being built incrementally: base node and GRANDPA finality first, then an isolated PoW authoring contour, Freeland records, rewards and public testnet. AuxPoW, EVM and ZK features are intentionally deferred until the base network is proven.

Runtime `spec_version` is `110`. Sudo, the template pallet and Aura have been removed from the runtime; GRANDPA is the only session key. Issuer changes have no root or development-account bypass and still require on-chain multisig approval followed by the timelock. Development accounts remain in the dev/local presets and the explicitly non-production example configuration.

The development contour uses a managed mining-thread lifecycle, discards stale mining jobs after the best block changes, and retargets difficulty every 60 blocks against a 3-second target using runtime timestamps. SHA-256 and these parameters are devnet scaffolding, not the final RandomX/mainnet configuration.

Verified: 44 workspace tests, 7 JavaScript address-codec tests, 3 indexer tests and both clippy configurations pass. The isolated production-candidate example finalized block 40, executed its multisig/timelocked issuer change and finalized a synthetic passport record with a verified 256-sibling registry proof. The multi-node regression separately confirmed finality loss/recovery, canonical reorg handling and `orphaned` indexer history.

## Native reward ledger

`pallet-mfc-rewards` implements integer-only emission accounting: 10,000,000 MFC in year one, a 10% annual reduction for 20 years, then 500,000 MFC per year. The target is 6 seconds per block and 5,256,000 blocks per year. Subsidy is split into 45% community PoW, 35% GRANDPA and 20% treasury; rounding remainder goes to treasury so the parts always sum to the whole subsidy.

Each PoW `PreRuntime` digest carries `version=0 + AccountId32` for the public payout address. Live mining requires an explicit `--pow-payout-account`; no private payout key is stored on the node. There is no root extrinsic or manual mint.

Obligations are recorded in canonical block state. PoW and treasury mature after 10 blocks in the development configuration, while the GRANDPA share is only accrued and is not minted or paid until verifiable validator work is implemented. Canonical maturation is not a claim of GRANDPA finality proof. Development presets enable this ledger for testing; the production example and the existing six-node testnet have not activated a new emission schedule.

Read-only `mfc_rewardTotals(at?)` and `mfc_rewardObligation(block, at?)` expose accounting for independent verification. Amounts are strings to avoid JavaScript precision loss.

## GRANDPA evidence

`mfc_validatorEvidence(block_hash)` decodes the block's GRANDPA justification, checks the commit target, ancestry, authority set id and validator signatures, and returns the actual precommit signers. Missing or invalid justifications are rejected instead of being treated as validator activity. The development node stores a justification for every finalized block so the proof remains queryable after a restart; the additional disk usage and production retention policy require review before a public launch.

This is a read-only proof of participation, not a payout mechanism. The 35% validator pool remains accrued and is not minted until the reward-release contour uses this evidence safely. `scripts/check-grandpa-evidence.mjs` verifies a finalized block and rejects a block without a valid proof.

## Canonical account addresses

MFCoin account addresses use Bech32m with `mfcx` for mainnet and `tmfcx` for testnets. Payload version `0` contains one zero byte followed by the same 32-byte Sr25519 `AccountId` used by Substrate; private keys are unchanged. Addresses are lowercase only, use six checksum characters, and are 64 characters for `mfcx` and 65 for `tmfcx`.

The dependency-free codecs live in `primitives/address` (Rust, `no_std`-compatible) and `packages/address` (JavaScript for Node.js and browsers). Both consume `packages/address/test-vectors.json` and reject bad checksums, HRP mismatches, wrong lengths, uppercase, invalid padding and unknown versions. The binary specification is in `packages/address/README.md`.

Example representations of one public key:

```text
mfcx1qr0q6mcy33zau35pvz8qnt87z02mzmrp04ehd7n0d4m67rg8c4axwm22jed
tmfcx1qr0q6mcy33zau35pvz8qnt87z02mzmrp04ehd7n0d4m67rg8c4axwyt737x
```

Run the JavaScript tests with `cd packages/address && npm test`, and the Rust tests with `cargo +1.88.0 test -p mfcoin-address`. Nodes and transaction tooling continue to accept SS58 until wallet/API integration is complete; the codecs alone do not enable `mfcx` financial transactions.

## Production-candidate configuration

`--chain production` fails closed unless `MFCOIN_PRODUCTION_CONFIG` points to an explicit operator configuration. The JSON must contain:

- unique GRANDPA **Ed25519** validator public keys (not the Sr25519 transaction account IDs);
- between 2 and 16 governance signatories, unique and sorted by `AccountId` (the current threshold is 2-of-N);
- unique endowed accounts with positive balances for transaction fees and multisig deposits.

Unknown JSON fields are rejected. No private keys or seeds belong in this file.

`production.example.json` uses only the public Alice/Bob/Charlie development accounts so the complete flow can be tested locally. **These keys are public and must never secure a real network.** A real launch still requires independently controlled validator and governance keys, operational and security review, reproducible release artifacts, and a reviewed migration plan for real registry data.

To inspect the example genesis without starting a node:

```sh
MFCOIN_PRODUCTION_CONFIG=production.example.json \
  ./target/release/solochain-template-node export-chain-spec --chain production
```

A live chain does not auto-generate a network key. Provision it separately for each node and protect the resulting `chains/mfcoin2/network/secret_ed25519` file:

```sh
MFCOIN_PRODUCTION_CONFIG=production.example.json \
  ./target/release/solochain-template-node key generate-node-key \
  --chain production --base-path .devnet/production-node
```

The following check exports that genesis, starts an isolated two-validator local network, exercises multisig/timelock governance and verifies a finalized registry proof. It uses public development keys only and stops all processes on exit:

```sh
cargo +1.88.0 build --release --example freeland_devnet
node scripts/check-production.mjs
```

## Freeland data contour

`pallet-freeland-records` accepts only fixed-size commitments and hashes, never raw passport or KYC payloads. It provides typed records, per-kind issuer permissions, monotonic issuer nonces, latest-state storage and full record data in events. Each block is limited to 64 records in this devnet configuration.

Passport and citizen state uses a domain-separated sparse Merkle tree with 256 levels. A leaf key commits to `kind + subject_commitment`; updating or revoking a subject replaces its leaf without adding another subject. Each update touches only the 256-node path, and `on_finalize` reads the already computed root in O(1). Nodes are overwritten by path rather than retained as an append-only history.

`freeland_registryProof` returns inclusion or absence proofs. Verification detects changed entries, siblings and proof lengths. There is no 256-entry registry cap. The 33K test is synthetic and contains no real user data. Production migration should use batch/genesis construction rather than 33,000 individual transactions.

The info root is deposited as an `MFCF` digest during `on_finalize`. `governance_root`, `epoch` and `validator_set_hash` remain explicitly reserved and zero. Issuer changes require the on-chain 2-of-N multisig account and a 20-block timelock: `set_issuer` creates a pending change, and `execute_issuer_change` applies it only after `execute_at`.

## Isolation

- Development state belongs to `.devnet/` and is not committed.
- Validator keys, deployment credentials, passport data and KYC are not committed.
- The MF Life website and its private archive are deployed from a separate repository.

## Build

Requirements:

- Rust `1.88.0` with the `wasm32-unknown-unknown` target;
- LLVM/libclang on macOS;
- `protoc` (`brew install protobuf`) for `litep2p`;
- Node.js for the devnet RPC check.

```sh
cargo +1.88.0 build --release
./target/release/solochain-template-node --dev
```

On macOS with Homebrew LLVM:

```sh
export LIBCLANG_PATH=/opt/homebrew/opt/llvm/lib
export DYLD_LIBRARY_PATH=/opt/homebrew/opt/llvm/lib
cargo +1.88.0 build --release
```

## Verify a running devnet

```sh
node scripts/check-devnet.mjs
```

The check confirms block production and finality, requires PoW and Freeland digests, and compares the RPC info root with the digest.

## Benchmarks and multi-node checks

```sh
cargo +1.88.0 test -p pallet-freeland-records --features runtime-benchmarks
cargo +1.88.0 test --release -p pallet-freeland-records synthetic_33k_registry_and_proofs -- --ignored --nocapture
cargo +1.88.0 build --release
node scripts/check-multinode.mjs
```

The multi-node script starts four validators, with only Alice and Bob mining. It verifies finality loss and recovery after Bob and Dave go offline, then builds two isolated forks and verifies that Bob adopts the canonical chain. Processes, keys and logs stay under `.devnet/multinode-v1/`; mDNS is disabled so isolation is explicit. With `CHECK_INDEXER=1`, it also starts PostgreSQL and verifies that a record on Bob's losing fork becomes `orphaned`.

Validator-only nodes use `--disable-pow-mining`. Generated weights live in `pallets/freeland-records/src/weights.rs` and were measured with 50 steps and 20 repeats using compiled WASM execution. Current measurements on this CPU are about 2.299 ms for `submit_record` and 74 µs for `on_finalize`; reference-hardware review is still required. A normal release build is required after building with `runtime-benchmarks`.

## Batch genesis and proofs

A development chain can include up to 33,000 synthetic records at genesis:

```sh
MFCOIN_SYNTHETIC_REGISTRY=33000 ./target/release/solochain-template-node --dev --rpc-port 9945 --base-path .devnet/genesis-33k-v1
```

Genesis is built incrementally to avoid holding the full node map in WASM memory. This is a deliberately long development operation with substantial storage and memory use, not a production migration recipe. `freeland_blockRecordsPage(offset, limit, at?)` returns at most 1,000 entries per call, and `freeland_totalRecords(at?)` verifies the total.

To verify a proof independently:

```sh
FREELAND_KIND=passport FREELAND_SUBJECT=0x... FREELAND_AT=0x... \
  cargo +1.88.0 run --release -p solochain-template-node --example verify_freeland_proof
```

`DEVNET_HTTP_URL` defaults to `http://127.0.0.1:9944`; set it to `http://127.0.0.1:9945` for the batch example above.

## Read-only Freeland RPC

- `freeland_latestBlockInfo(at?)`
- `freeland_latestRecord(kind, subjectCommitment, at?)`
- `freeland_registryProof(kind, subjectCommitment, at?)`
- `freeland_blockRecords(at?)`
- `freeland_blockRecordsPage(offset, limit, at?)`
- `freeland_totalRecords(at?)`
- `freeland_pendingIssuerChanges(at?)`

`kind` is `passport`, `citizen`, `election`, `document` or `revocation`. The optional `at` parameter is a block hash; without it, methods query the best block. Responses contain commitments, hashes and Merkle proof data only.

On a fresh local devnet, the development-only example approves an issuer change through the 2-of-3 multisig, waits for the timelock, executes it, then submits and verifies a signed test record:

```sh
cargo +1.88.0 run --release -p solochain-template-node --example freeland_devnet
```

It connects only to localhost, uses the public development Alice key and contains no real personal data or production credentials.

## PostgreSQL indexer

The separate `indexer/` process stores block informers and Freeland record history in PostgreSQL. It marks non-canonical history `orphaned`, canonical records `included`, and GRANDPA-confirmed records `finalized`. It does not hold validator keys or personal payloads.

Start the development database and API after starting the node:

```sh
cd indexer
docker compose up -d --wait
npm ci
npm test
npm start
```

The API listens on `127.0.0.1:8787` by default:

- `GET /health`
- `GET /freeland/finality`
- `GET /freeland/block-info/:block_hash`
- `GET /freeland/records/:subject_commitment?status=finalized`
- `GET /freeland/proofs/:kind/:subject_commitment?at=:block_hash`

`DATABASE_URL`, `CHAIN_RPC_URL`, `INDEXER_HOST`, `INDEXER_PORT` and `FINALITY_MAX_LAG` are configurable. The credentials in `compose.yml` are development-only. Stop the database with `docker compose stop` to preserve its volume for the next run.

## Upstream

- Polkadot SDK Solochain Template: https://github.com/paritytech/polkadot-sdk-solochain-template
- Polkadot SDK: https://github.com/paritytech/polkadot-sdk

See `LICENSE` for the upstream license.
