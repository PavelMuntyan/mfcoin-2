# MFCoin 2.0 — Blockchain

Public development repository for the new MFCoin blockchain architecture.

## Status

Contour 3: isolated PoW authoring, GRANDPA finality and the first Freeland data records.

The current workspace is based on the standalone Polkadot SDK Solochain Template, branch `update-template/stable2512`, upstream commit `1adedaffe70df1dab07e34e6a57c19e732ececb9`.

The project is being built incrementally: base node and GRANDPA finality first, then an isolated PoW authoring contour, Freeland records, rewards and public testnet. AuxPoW, EVM and ZK features are intentionally deferred until the base network is proven.

Verified: the release build completed on Rust `1.88.0`, and the Freeland devnet advanced best block `0x15 -> 0x17` with finalized block `0x15`. The best block contained both a `pow_` seal and an `MFCF` Freeland info root. All 20 workspace tests pass, including PoW retarget, issuer permissions, replay rejection, record limits and Freeland root tests.

The development contour now uses a managed mining-thread lifecycle, discards stale mining jobs after the best block changes, and retargets difficulty every 60 blocks against a 3-second target using runtime timestamps. SHA-256 and these parameters are devnet scaffolding, not the final RandomX/mainnet configuration.

## Freeland data contour

`pallet-freeland-records` accepts only fixed-size commitments and hashes, never raw passport or KYC payloads. It provides typed records, per-kind issuer permissions, monotonic issuer nonces, latest-state storage and full record data in events. Each block is limited to 64 records in this devnet configuration.

`FreelandBlockInfo` currently commits to the previous info root and a domain-separated Merkle root of record IDs from the block. Its root is deposited as an `MFCF` digest during `on_finalize`. `registry_root`, `governance_root`, `epoch` and `validator_set_hash` are explicitly reserved and zero; they are not yet implemented.

Issuer management currently uses the development root origin. Production multisig, timelocks, registry roots, voting, encryption and migration remain separate later contours.

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

The check confirms that the best block advances, returns the current finalized head, and requires both a PoW seal and a Freeland info root in the best-block digest.

## Upstream

- Polkadot SDK Solochain Template: https://github.com/paritytech/polkadot-sdk-solochain-template
- Polkadot SDK: https://github.com/paritytech/polkadot-sdk

See `LICENSE` for the upstream license.
