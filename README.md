# MFCoin 2.0 — Blockchain

Public development repository for the new MFCoin blockchain architecture.

## Status

Contour 6: a 256-level sparse Merkle registry with verifiable proofs and a 33K-record synthetic test.

The current workspace is based on the standalone Polkadot SDK Solochain Template, branch `update-template/stable2512`, upstream commit `1adedaffe70df1dab07e34e6a57c19e732ececb9`.

The project is being built incrementally: base node and GRANDPA finality first, then an isolated PoW authoring contour, Freeland records, rewards and public testnet. AuxPoW, EVM and ZK features are intentionally deferred until the base network is proven.

Verified: 24 workspace tests and 17 active pallet tests with runtime benchmarks pass. A separate release test populated 33,000 passport/citizen entries and verified inclusion proofs. A signed devnet record was finalized, returned through `freeland_registryProof`, independently verified with 256 siblings and matched to the finalized `FreelandBlockInfo` root. Four-validator finality loss/recovery and fork reorganization also passed again.

The development contour uses a managed mining-thread lifecycle, discards stale mining jobs after the best block changes, and retargets difficulty every 60 blocks against a 3-second target using runtime timestamps. SHA-256 and these parameters are devnet scaffolding, not the final RandomX/mainnet configuration. Runtime `spec_version` is `104`.

## Freeland data contour

`pallet-freeland-records` accepts only fixed-size commitments and hashes, never raw passport or KYC payloads. It provides typed records, per-kind issuer permissions, monotonic issuer nonces, latest-state storage and full record data in events. Each block is limited to 64 records in this devnet configuration.

Passport and citizen state uses a domain-separated sparse Merkle tree with 256 levels. A leaf key commits to `kind + subject_commitment`; updating or revoking a subject replaces its leaf without adding another subject. Each update touches only the 256-node path, and `on_finalize` reads the already computed root in O(1). Nodes are overwritten by path rather than retained as an append-only history.

`freeland_registryProof` returns inclusion or absence proofs. Verification detects changed entries, siblings and proof lengths. There is no 256-entry registry cap. The 33K test is synthetic and contains no real user data. Production migration should use batch/genesis construction rather than 33,000 individual transactions.

The info root is deposited as an `MFCF` digest during `on_finalize`. `governance_root`, `epoch` and `validator_set_hash` remain explicitly reserved and zero. Issuer management currently uses the development root origin. Production multisig, timelocks, voting, encryption and migration remain separate later contours.

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

The multi-node script starts four validators, with only Alice and Bob mining. It verifies finality loss and recovery after Bob and Dave go offline, then builds two isolated forks and verifies that Bob adopts the canonical chain. Processes, keys and logs stay under `.devnet/multinode-v1/`; mDNS is disabled so isolation is explicit.

Validator-only nodes use `--disable-pow-mining`. Generated weights live in `pallets/freeland-records/src/weights.rs` and were measured with 50 steps and 20 repeats using compiled WASM execution. Current measurements on this CPU are about 2.299 ms for `submit_record` and 74 µs for `on_finalize`; reference-hardware review is still required. A normal release build is required after building with `runtime-benchmarks`.

## Read-only Freeland RPC

- `freeland_latestBlockInfo(at?)`
- `freeland_latestRecord(kind, subjectCommitment, at?)`
- `freeland_registryProof(kind, subjectCommitment, at?)`

`kind` is `passport`, `citizen`, `election`, `document` or `revocation`. The optional `at` parameter is a block hash; without it, methods query the best block. Responses contain commitments, hashes and Merkle proof data only.

On a fresh local devnet, the development-only example assigns a passport issuer, submits a signed test record and verifies its finalized registry proof:

```sh
cargo +1.88.0 run --release -p solochain-template-node --example freeland_devnet
```

It connects only to localhost, uses the public development Alice key and contains no real personal data or production credentials.

## Upstream

- Polkadot SDK Solochain Template: https://github.com/paritytech/polkadot-sdk-solochain-template
- Polkadot SDK: https://github.com/paritytech/polkadot-sdk

See `LICENSE` for the upstream license.
