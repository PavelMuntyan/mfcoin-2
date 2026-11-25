#!/usr/bin/env bash
set -euo pipefail

ROOT=${MFC_REHEARSAL_ROOT:-/opt/mfcoin-mainnet-rehearsal}
NODE=${MFC_REHEARSAL_NODE:-$ROOT/target/release/solochain-template-node}
CONFIG=${MFC_REHEARSAL_CONFIG:-$ROOT/production.rehearsal.json}
DATA=${MFC_REHEARSAL_DATA:-/var/lib/mfcoin-mainnet-rehearsal}
SECRETS=${MFC_REHEARSAL_SECRETS:-$ROOT/secrets}
START=${MFC_REHEARSAL_START:-1}
BOOTNODE=${MFC_REHEARSAL_BOOTNODE:-}

if [[ "$START" != "1" && "$START" != "3" ]]; then
  echo "MFC_REHEARSAL_START must be 1 or 3" >&2
  exit 1
fi
for required in "$NODE" "$CONFIG"; do
  if [[ ! -e "$required" ]]; then
    echo "Required rehearsal file is missing: $required" >&2
    exit 1
  fi
done

mkdir -p "$DATA" "$ROOT"
export MFCOIN_PRODUCTION_CONFIG="$CONFIG"
"$NODE" export-chain-spec --raw --chain production > "$ROOT/chain-spec-raw.json"

json_field() {
  node -p 'JSON.parse(require("fs").readFileSync(process.argv[1], "utf8"))[process.argv[2]]' "$1" "$2"
}

start_or_reload() {
  local name=$1
  if pm2 describe "$name" >/dev/null 2>&1; then
    pm2 delete "$name"
  fi
  shift
  pm2 start "$NODE" --name "$name" --cwd "$ROOT" --time -- "$@"
}

export MFCOIN_GRANDPA_JUSTIFICATION_PERIOD=1
for index in "$START" "$((START + 1))"; do
  validator_secret="$SECRETS/validator-$index.json"
  payout_secret="$SECRETS/payout-$index.json"
  for required in "$validator_secret" "$payout_secret"; do
    if [[ ! -f "$required" ]]; then
      echo "Required secret document is missing: $required" >&2
      exit 1
    fi
  done
  base="$DATA/validator-$index"
  mkdir -p "$base"
  if [[ ! -f "$base/chains/mfcoin-mainnet-rehearsal/network/secret_ed25519" ]]; then
    "$NODE" key generate-node-key --base-path "$base" --chain "$ROOT/chain-spec-raw.json" >/dev/null
  fi
  seed_file=$(mktemp "$DATA/validator-$index-grandpa-seed.XXXXXX")
  trap 'rm -f "$seed_file"' EXIT
  chmod 600 "$seed_file"
  json_field "$validator_secret" secretPhrase > "$seed_file"
  "$NODE" key insert --base-path "$base" --chain "$ROOT/chain-spec-raw.json" \
    --key-type gran --scheme ed25519 --suri "$seed_file"
  rm -f "$seed_file"
  trap - EXIT
  payout=$(json_field "$payout_secret" publicKey)
  if [[ "$START" == "1" && "$index" == "2" && -z "$BOOTNODE" ]]; then
    peer_id=$("$NODE" key inspect-node-key --file "$DATA/validator-1/chains/mfcoin-mainnet-rehearsal/network/secret_ed25519")
    BOOTNODE="/ip4/127.0.0.1/tcp/30450/p2p/$peer_id"
  fi
  args=(
    --validator --chain "$ROOT/chain-spec-raw.json" --base-path "$base"
    --rpc-port "$((9969 + index))" --port "$((30449 + index))"
    --rpc-methods safe --no-mdns --no-prometheus --pow-payout-account "$payout"
  )
  if [[ -n "$BOOTNODE" ]]; then
    args+=(--bootnodes "$BOOTNODE")
  fi
  start_or_reload "mfc-rehearsal-validator-$index" "${args[@]}"
done

pm2 save
printf 'Rehearsal validators %s and %s are running\n' "$START" "$((START + 1))"
