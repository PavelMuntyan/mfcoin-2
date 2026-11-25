# MFCoin 2.0 — Blockchain

Public development repository for the new MFCoin blockchain architecture.

## Status

Contour 18: Testnet 115 currently runs runtime 116 with independently generated validator and payout keys, GRANDPA finality, native rewards, governance upgrades and community assets. The runtime-117 proposal is scheduled for block 33918, and its on-chain code matches the reviewed artifact. Mainnet has not been launched.

The current workspace is based on the standalone Polkadot SDK Solochain Template, branch `update-template/stable2512`, upstream commit `1adedaffe70df1dab07e34e6a57c19e732ececb9`.

The project is being built incrementally: base node and GRANDPA finality first, then an isolated PoW authoring contour, Freeland records, rewards and public testnet. AuxPoW, EVM and ZK features are intentionally deferred until the base network is proven.

Runtime `spec_version` is `117`. Sudo, the template pallet and Aura have been removed from the runtime; GRANDPA is the only session key. Issuer changes have no root or development-account bypass and still require on-chain multisig approval followed by the timelock. Development accounts remain in the dev/local presets and the explicitly non-production example configuration.

The development contour uses a managed mining-thread lifecycle, discards stale mining jobs after the best block changes, and retargets difficulty every 60 blocks against a 6-second target using runtime timestamps. SHA-256 and these parameters are devnet scaffolding, not the final RandomX/mainnet configuration.

Verified: 74 workspace tests, 21 reward tests with benchmarks, 7 JavaScript address-codec tests, 3 indexer tests and both clippy configurations pass. The isolated production-candidate example finalized block 40, executed its multisig/timelocked issuer change and finalized a synthetic passport record with a verified 256-sibling registry proof. The multi-node regression separately confirmed finality loss/recovery, canonical reorg handling and `orphaned` indexer history.

`pallet-mfc-governance` provides a restricted upgrade path without an admin key. The existing 2-of-3 governance multisig signs proposals with a future `execute_at`; anyone may execute a proposal only within a six-hour execution window, and execution removes it to prevent replay. Governance can cancel pending proposals. Runtime upgrades and rotations require a two-day timelock, cannot be scheduled more than seven days ahead, and expire six hours after `execute_at`, while `note_stalled` retains the short recovery delay. Every proposal carries a governance epoch, so stale proposals cannot execute after governance rotation. `set_code` stores at most 3,000,000 bytes of runtime WASM with size-aware weights, `note_stalled` uses the native GRANDPA recovery call, and `rotate_grandpa` uses native `schedule_change` with a bounded non-empty authority list. Freeland issuer governance reads the same account. Mock-chain tests cover unauthorized signatures, early execution, cancel, expiry, stale epochs, oversized inputs, runtime-code replacement, authority rotation and governance-key rotation.

Runtime 114 replaces identity fees with a 1% weight coefficient and `10 * MICRO_UNIT` per byte. `TargetedFeeAdjustment` targets 25% normal-block fullness, varies the multiplier by `1/10000`, and clamps it between 1x and 100x. A 3 MiB payload costs more than 30 MFC before congestion adjustment; the 200-byte length fee remains below one MFC. Tests cover exact weight/length amounts and the multiplier response to a full block.

## Native community assets

Runtime 117 uses the audited `pallet_assets` for user-issued native tokens. Any signed account can create an asset by reserving 100 MFC; metadata reserves a separate base plus per-byte deposit. The runtime supports metadata, issuer-controlled mint and burn, transfers, approvals, freezing and the standard multi-step destruction flow. Asset IDs start at 1 and increase through `AutoIncAssetId`; an explicit wrong ID is rejected. There is no EVM or custom token pallet.

`scripts/check-assets.mjs` verifies creation, metadata, mint, transfer, burn, automatic IDs, supply and both account balances against a fresh running devnet.

## Native reward ledger

`pallet-mfc-rewards` implements integer-only emission accounting: 10,000,000 MFC in year one, a 10% annual reduction for 20 years, then 500,000 MFC per year. The target is 6 seconds per block and 5,256,000 blocks per year. Subsidy is split into 45% community PoW, 35% GRANDPA and 20% treasury; rounding remainder goes to treasury so the parts always sum to the whole subsidy.

Each PoW `PreRuntime` digest carries `version=0 + AccountId32` for the public payout address. Live mining requires an explicit `--pow-payout-account`; no private payout key is stored on the node. There is no root extrinsic or manual mint.

PoW and treasury obligations mature after 10 blocks. Validator obligations remain in separate storage until an on-chain release. `release_validator_reward` accepts the original encoded GRANDPA justification plus its exact signer and ancestry counts, checks those counts before verification, validates the canonical target, maturation, ancestry, authority membership, signatures and quorum, and rejects duplicates and understated weights. Only actual precommit signers share the target's 35% allocation.

Weights were measured with compiled WASM over `1..32` signers and `0..64` ancestry headers. The generated formula charges actual counts and DB access instead of the maximum proof for every release. Benchmarks cover release, registration, claim and `on_finalize`; reference-hardware review is still required before mainnet.

Validator obligations and releases are pruned after 1,200 blocks, within the canonical hash window. Unclaimed credits are never pruned and carry their own `set_id`, so claims still work after obligation cleanup. Validator set history preserves the old set for transition-block rewards and activates the new set from the following block.

A GRANDPA Ed25519 public key is not treated as a spendable Sr25519 account. Validators register a payout account with an Ed25519 signature over a domain-separated payload containing genesis hash, set id, authority, payout and a replay-protected nonce. If no payout is registered, the share becomes an unclaimed credit and is not minted until `claim_validator_credit`.

Runtime 114 offered no reward-storage migration from runtime 112; existing public networks received no retroactive obligations. Runtime 117 adds the asset pallet, initializes its next ID when absent and sets it to 1 in production genesis. The [public review summary](docs/AUDIT.md) records completed hardening and the remaining launch gates: runtime-117 execution on Testnet 115 followed by post-upgrade verification.

Read-only `mfc_rewardTotals`, `mfc_rewardObligation`, `mfc_validatorReward`, `mfc_validatorCredit`, `mfc_validatorPayout` and `mfc_validatorPayoutNonce` expose accounting for independent verification. Amounts and nonces are strings where JavaScript precision could be lost.

## GRANDPA evidence and relayer

`mfc_validatorEvidence(block_hash)` decodes the block's GRANDPA justification, checks the commit target, ancestry, authority set id and validator signatures, and returns actual precommit signers. Missing or invalid justifications are rejected. The native justification period defaults to 512; regression networks set `MFCOIN_GRANDPA_JUSTIFICATION_PERIOD=1` explicitly to retain every proof.

The read-only endpoint is not trusted for payment. Release verification runs independently in runtime. `node/examples/reward_relayer.rs` polls mature obligations, writes each proof to an atomic bounded queue before submission, resumes pending work after restart and fails loudly instead of silently skipping missing evidence. During its first synchronization only, it advances past historically unfinalized blocks that have no justification; after recording the first available proof, later missing evidence remains a hard failure. Expired obligations are removed from pending work after a fresh canonical-state check. One-shot mode processes a fixed mature-head snapshot instead of chasing new blocks indefinitely. `MFC_RELAYER_END_TARGET` can additionally cap that snapshot for bounded batches and deterministic restart tests. It uses a separate fee-paying relayer key, never a validator key. Configure `MFC_RELAYER_SEED`, `MFC_RELAYER_QUEUE` and the activation-appropriate `MFC_RELAYER_START_TARGET`; the explicit seed also applies to local RPC endpoints. Redundant relayers are recommended. `node/examples/register_validator_payout.rs` signs registration on the operator host with `MFC_VALIDATOR_SEED`, an SS58 `MFC_VALIDATOR_PAYOUT`, `MFC_VALIDATOR_SET_ID` and the separate relayer key.

`scripts/check-validator-rewards.mjs` covers direct release/claim/replay, while `scripts/check-reward-relayer.mjs` covers durable release, restart and duplicate safety.

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
- between 2 and 16 governance signatories, unique and sorted by `AccountId`, with an optional `governanceThreshold` of at least two thirds (default `ceil(2N/3)`);
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

## Public verification documents

- [Handoff for an AI user assistant](docs/AI_HANDOFF.md)
- [Security review summary](docs/AUDIT.md)
- [Vulnerability reporting policy](SECURITY.md)
- [Reproducible build verification](docs/REPRODUCIBLE-BUILDS.md)
- [Main Net operations runbook](infra/mainnet/RUNBOOK.md)
- [Public Main Net candidate configuration](production.mainnet.json)

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

## PoW work and retarget

`PowAux.difficulty` stores proof of work as `U256::MAX / target`; it does not store the hash target itself. Fork choice therefore accumulates more work for harder blocks. Seal verification converts that work back to a target with `U256::MAX / work`. Every 60 blocks, the node computes `next_target = current_target * elapsed / (60 * 6000)` and clamps the change to ±25%. Slow blocks make the target easier, fast blocks make it harder. The retarget ancestor is found by following `parent_hash` links through the actual fork, not by reading a block number from the canonical chain. Warp sync is disabled for this PoW contour, and missing auxiliary difficulty after genesis is a hard import error instead of a silent reset to the initial target. Nodes built before this model was introduced cannot be mixed with the old testnet database: use a coordinated fresh devnet or an explicitly planned reset and native-client upgrade.

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
