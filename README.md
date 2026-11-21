# MFCoin 2.0 — Blockchain

Public development repository for the new MFCoin blockchain architecture.

## Status

Contour 1: isolated devnet foundation.

The current workspace is based on the standalone Polkadot SDK Solochain Template, branch `update-template/stable2512`, upstream commit `1adedaffe70df1dab07e34e6a57c19e732ececb9`.

The project is being built incrementally: base node and GRANDPA finality first, then an isolated PoW authoring contour, rewards, Freeland records and public testnet. AuxPoW, EVM and ZK features are intentionally deferred until the base network is proven.

Verified: the release build completed on Rust `1.88.0`, and the devnet advanced best block `0x3 -> 0x4` with finalized block `0x2` via `scripts/check-devnet.mjs`.

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

The check confirms that the best block advances and returns the current finalized head.

## Upstream

- Polkadot SDK Solochain Template: https://github.com/paritytech/polkadot-sdk-solochain-template
- Polkadot SDK: https://github.com/paritytech/polkadot-sdk

See `LICENSE` for the upstream license.
