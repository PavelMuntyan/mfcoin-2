#!/usr/bin/env bash
set -euo pipefail

ROOT=/var/www/balexey/data/mfcoin-testnet
for _ in $(seq 1 60); do
  if curl -fsS -H 'content-type: application/json' --data '{"jsonrpc":"2.0","id":1,"method":"chain_getHeader","params":[]}' http://127.0.0.1:9951 >/dev/null 2>&1; then
    break
  fi
  sleep 1
done
for index in 1 2 3 4; do
  authority=$(node -p 'JSON.parse(require("fs").readFileSync(process.argv[1], "utf8")).publicKeyHex' "$ROOT/secrets/validator-$index.json")
  payout=$(node -p 'JSON.parse(require("fs").readFileSync(process.argv[1], "utf8")).publicKey' "$ROOT/secrets/payout-$index.json")
  registered=$(curl -fsS -H 'content-type: application/json' --data "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"mfc_validatorPayout\",\"params\":[0,\"$authority\",null]}" http://127.0.0.1:9951 | node -pe 'JSON.parse(require("fs").readFileSync(0,"utf8")).result')
  if [ "$registered" = "$payout" ]; then
    echo "validator_payout_registered=true already_present=true validator=$index"
    continue
  fi
  validator_seed=$(node -p 'JSON.parse(require("fs").readFileSync(process.argv[1], "utf8")).secretPhrase' "$ROOT/secrets/validator-$index.json")
  relayer_seed=$(node -p 'JSON.parse(require("fs").readFileSync(process.argv[1], "utf8")).secretPhrase' "$ROOT/secrets/relayer-1.json")
  MFC_VALIDATOR_SEED="$validator_seed" \
  MFC_VALIDATOR_PAYOUT="$payout" \
  MFC_RELAYER_SEED="$relayer_seed" \
  MFC_VALIDATOR_SET_ID=0 \
  DEVNET_WS_URL=ws://127.0.0.1:9951 \
    "$ROOT/target/release/examples/register_validator_payout"
done
