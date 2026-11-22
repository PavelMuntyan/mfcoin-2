#!/usr/bin/env bash
set -euo pipefail

ROOT=/var/www/balexey/data/mfcoin-testnet
for index in 1 2 3 4; do
  validator_seed=$(node -p 'JSON.parse(require("fs").readFileSync(process.argv[1], "utf8")).secretPhrase' "$ROOT/secrets/validator-$index.json")
  payout=$(node -p 'JSON.parse(require("fs").readFileSync(process.argv[1], "utf8")).publicKey' "$ROOT/secrets/payout-$index.json")
  relayer_seed=$(node -p 'JSON.parse(require("fs").readFileSync(process.argv[1], "utf8")).secretPhrase' "$ROOT/secrets/relayer-1.json")
  MFC_VALIDATOR_SEED="$validator_seed" \
  MFC_VALIDATOR_PAYOUT="$payout" \
  MFC_RELAYER_SEED="$relayer_seed" \
  MFC_VALIDATOR_SET_ID=0 \
  DEVNET_WS_URL=ws://127.0.0.1:9951 \
    "$ROOT/target/release/examples/register_validator_payout"
done
