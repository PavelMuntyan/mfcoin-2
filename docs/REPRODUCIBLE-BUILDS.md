# Reproducible build verification

## Build the runtime

Use Rust 1.88.0 with the `wasm32-unknown-unknown` target, LLVM/libclang and `protoc`. Node-based verification scripts require `@polkadot/api` 16.4.4; install it locally without changing the repository manifest with `npm install --no-save --package-lock=false @polkadot/api@16.4.4`.

```sh
git clone https://github.com/PavelMuntyan/mfcoin-2.git
cd mfcoin-2
git checkout 7cf7e48a30eb084c14e51325839e3ea5bfc8f0c1
cargo +1.88.0 build --locked --release
```

The runtime artifact is:

```text
target/release/wbuild/solochain-template-runtime/solochain_template_runtime.compact.compressed.wasm
```

The reviewed runtime-117 artifact contains 548,538 bytes and has this Blake2-256 hash:

```text
0xb7f166eb61484bc13749ce4ee03046eb9be2185b9c8c8eafd45367dd319378bf
```

The public CI artifact is stored as `runtime-117` in workflow run `45493192039` of the deployment repository. Its `mfcoin-source-sha` file must contain the commit shown above.

## Verify the scheduled proposal

`scripts/upgrade-runtime.mjs` checks the expected genesis, derives the governance multisig address from the configured signers and compares the local WASM Blake2-256 hash with the on-chain `SetCode` proposal before signing anything.

The deployment workflow's `verify_only` mode stops after these checks and submits no transaction. Execution uses the same artifact and proposal ID only after the scheduled finalized block.

## Verify Main Net genesis without starting a node

Export the raw specification with the public candidate configuration:

```sh
export MFCOIN_PRODUCTION_CONFIG="$PWD/production.mainnet.json"
./target/release/solochain-template-node export-chain-spec --raw --chain production > chain-spec-raw.json
```

Run `scripts/verify-mainnet-genesis.mjs` with `MFC_MAINNET_RAW_SPEC` pointing to that file. The check requires:

- network ID `mfcoin2-mainnet` and runtime `specVersion=117`;
- four GRANDPA authorities and three governance signatories;
- no sudo key or Aura authorities;
- total issuance of exactly `1000000000000` base units (1 MFC);
- exactly one funded account, the configured treasury;
- zero funded balances for every other account.

Compare the raw-spec SHA-256 across every production host before launch. After validator 1 starts, pin and compare the resulting genesis hash everywhere else.

## Regression checks

```sh
cargo +1.88.0 test --workspace
node scripts/check-production.mjs
node scripts/check-production-rewards.mjs
cd packages/address && npm test
```

Some integration checks build and start isolated local nodes with public development keys. They must never be pointed at production secrets or real user data.
