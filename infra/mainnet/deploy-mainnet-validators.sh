#!/usr/bin/env bash
set -euo pipefail

ROOT=${MFC_MAINNET_ROOT:-/opt/mfcoin-mainnet}
NODE=${MFC_MAINNET_NODE:-$ROOT/target/release/solochain-template-node}
CONFIG=${MFC_MAINNET_CONFIG:-$ROOT/production.mainnet.json}
PAYOUT_CONFIG=${MFC_MAINNET_FOUNDER_CONFIG:-$ROOT/founder-payouts.mainnet.json}
DATA=${MFC_MAINNET_DATA:-/var/lib/mfcoin-mainnet}
SECRETS=${MFC_MAINNET_SECRETS:-$ROOT/secrets}
START=${MFC_MAINNET_START:-1}
BOOTNODE=${MFC_MAINNET_BOOTNODE:-}

if [[ "$START" != "1" && "$START" != "3" ]]; then
  echo "MFC_MAINNET_START must be 1 or 3" >&2
  exit 1
fi
for required in "$NODE" "$CONFIG" "$PAYOUT_CONFIG"; do
  if [[ ! -e "$required" ]]; then
    echo "Required Main Net file is missing: $required" >&2
    exit 1
  fi
done

mapfile -t founder_payouts < <(node -e '
  const config = JSON.parse(require("fs").readFileSync(process.argv[1], "utf8"));
  const production = JSON.parse(require("fs").readFileSync(process.argv[2], "utf8"));
  if (!Array.isArray(config.founderPayouts) || config.founderPayouts.length !== 4) throw new Error("Exactly four active founder payout addresses are required");
  if (new Set(config.founderPayouts).size !== 4) throw new Error("Founder payout addresses must be unique");
  if (config.founderPayouts.includes(production.rewards.treasuryAccount)) throw new Error("Treasury cannot be a founder payout");
  for (const payout of config.founderPayouts) console.log(payout);
' "$PAYOUT_CONFIG" "$CONFIG")

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
  if [[ ! -f "$validator_secret" ]]; then
    echo "Required validator secret document is missing: $validator_secret" >&2
    exit 1
  fi
  base="$DATA/validator-$index"
  mkdir -p "$base"
  if [[ ! -f "$base/chains/mfcoin2-mainnet/network/secret_ed25519" ]]; then
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
  payout=${founder_payouts[index - 1]}
  if [[ "$START" == "1" && "$index" == "2" && -z "$BOOTNODE" ]]; then
    peer_id=$("$NODE" key inspect-node-key --file "$DATA/validator-1/chains/mfcoin2-mainnet/network/secret_ed25519")
    BOOTNODE="/ip4/127.0.0.1/tcp/30460/p2p/$peer_id"
  fi
  args=(
    --validator --chain "$ROOT/chain-spec-raw.json" --base-path "$base"
    --rpc-port "$((9979 + index))" --port "$((30459 + index))"
    --rpc-methods safe --no-mdns --no-prometheus --pow-payout-account "$payout"
  )
  if [[ -n "$BOOTNODE" ]]; then
    args+=(--bootnodes "$BOOTNODE")
  fi
  start_or_reload "mfc-mainnet-validator-$index" "${args[@]}"
done

pm2 save
printf 'Main Net validators %s and %s are running with founder PoW payouts\n' "$START" "$((START + 1))"
