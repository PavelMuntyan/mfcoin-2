#!/usr/bin/env bash
set -euo pipefail

ROOT=/var/www/balexey/data/mfcoin-testnet-115
OLD=/var/www/balexey/data/mfcoin-testnet
BIN="$ROOT/target/release"
NODE="$BIN/solochain-template-node"
SECRETS="$OLD/secrets"
DATA="$ROOT/data"
OPERATORS="$ROOT/operators.json"
RAW="$ROOT/chain-spec-raw.json"
mkdir -p "$DATA" "$ROOT/infra/testnet"

if [ ! -f "$OPERATORS" ]; then
  node -e 'const fs=require("fs");const config=JSON.parse(fs.readFileSync(process.argv[1],"utf8"));config.network={name:"MFCoin 2.0 Testnet 115",id:"mfcoin-testnet-115"};fs.writeFileSync(process.argv[2],JSON.stringify(config,null,2)+"\n",{mode:0o644})' "$OLD/operators.json" "$OPERATORS"
fi
export MFCOIN_PRODUCTION_CONFIG="$OPERATORS"
"$NODE" export-chain-spec --chain production > "$ROOT/chain-spec.json"
"$NODE" export-chain-spec --raw --chain production > "$RAW"
"$NODE" --version

json_field() {
  node -p 'JSON.parse(require("fs").readFileSync(process.argv[1], "utf8"))[process.argv[2]]' "$1" "$2"
}

rpc_result() {
  local port=$1
  local method=$2
  curl -fsS -H 'content-type: application/json' \
    --data "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"$method\",\"params\":[]}" \
    "http://127.0.0.1:$port" | node -pe 'JSON.parse(require("fs").readFileSync(0, "utf8")).result'
}

wait_for_rpc() {
  local port=$1
  for _ in $(seq 1 120); do
    if rpc_result "$port" chain_getHeader >/dev/null 2>&1; then return 0; fi
    sleep 0.5
  done
  echo "RPC did not start on port $port" >&2
  return 1
}

BOOT_PIDS=()
stop_bootstrap() {
  for pid in "${BOOT_PIDS[@]:-}"; do kill -INT "$pid" 2>/dev/null || true; done
}
trap stop_bootstrap EXIT

BOOTNODE=""
for index in 1 2 3 4; do
  rpc_port=$((9959 + index))
  p2p_port=$((30439 + index))
  base="$DATA/validator-$index"
  if pm2 describe "mfc115-validator-$index" >/dev/null 2>&1; then
    if [ "$index" -eq 1 ]; then
      peer_id=$("$NODE" key inspect-node-key --file "$base/chains/mfcoin-testnet-115/network/secret_ed25519")
      BOOTNODE="/ip4/127.0.0.1/tcp/$p2p_port/p2p/$peer_id"
    fi
    continue
  fi
  mkdir -p "$base"
  if [ ! -f "$base/chains/mfcoin-testnet-115/network/secret_ed25519" ]; then
    "$NODE" key generate-node-key --base-path "$base" --chain "$RAW" >/dev/null
  fi
  "$NODE" --validator --chain "$RAW" --base-path "$base" \
    --rpc-port "$rpc_port" --port "$p2p_port" --rpc-methods unsafe \
    --no-mdns --no-prometheus --disable-pow-mining \
    > "$DATA/validator-$index-bootstrap.log" 2>&1 &
  BOOT_PIDS+=("$!")
  wait_for_rpc "$rpc_port"
  public_key=$(json_field "$SECRETS/validator-$index.json" publicKeyHex)
  node "$ROOT/infra/testnet/insert-grandpa-key.mjs" "$SECRETS/validator-$index.json" "$rpc_port" "$public_key"
  if [ "$index" -eq 1 ]; then
    peer_id=$(rpc_result "$rpc_port" system_localPeerId)
    BOOTNODE="/ip4/127.0.0.1/tcp/$p2p_port/p2p/$peer_id"
  fi
  kill -INT "${BOOT_PIDS[-1]}" 2>/dev/null || true
  wait "${BOOT_PIDS[-1]}" 2>/dev/null || true
  unset 'BOOT_PIDS[-1]'
done
trap - EXIT

start_or_reload() {
  local name=$1
  local script=$2
  shift 2
  if pm2 describe "$name" >/dev/null 2>&1; then pm2 delete "$name"; fi
  pm2 start "$script" --name "$name" "$@"
}

export MFCOIN_GRANDPA_JUSTIFICATION_PERIOD=1
for index in 1 2 3 4; do
  rpc_port=$((9959 + index))
  p2p_port=$((30439 + index))
  payout=$(json_field "$SECRETS/payout-$index.json" publicKey)
  args=(
    --validator --chain "$RAW" --base-path "$DATA/validator-$index"
    --rpc-port "$rpc_port" --port "$p2p_port"
    --rpc-methods safe --rpc-cors "https://mf.life" --no-mdns --no-prometheus --pow-payout-account "$payout"
  )
  if [ "$index" -gt 1 ]; then args+=(--bootnodes "$BOOTNODE"); fi
  start_or_reload "mfc115-validator-$index" "$NODE" --cwd "$ROOT" --time -- "${args[@]}"
done

for _ in $(seq 1 300); do
  best_number=$(curl -fsS -H 'content-type: application/json' --data '{"jsonrpc":"2.0","id":1,"method":"chain_getHeader","params":[]}' http://127.0.0.1:9961 | node -pe 'const payload=JSON.parse(require("fs").readFileSync(0,"utf8")); parseInt(payload.result.number,16)')
  if [ "$best_number" -ge 100 ]; then break; fi
  sleep 2
done
if [ "$best_number" -lt 100 ]; then
  echo "Reward activation block 100 was not reached" >&2
  exit 1
fi

for index in 1 2 3 4; do
  authority=$(json_field "$SECRETS/validator-$index.json" publicKeyHex)
  payout=$(json_field "$SECRETS/payout-$index.json" publicKey)
  registered=$(curl -fsS -H 'content-type: application/json' --data "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"mfc_validatorPayout\",\"params\":[0,\"$authority\",null]}" http://127.0.0.1:9961 | node -pe 'JSON.parse(require("fs").readFileSync(0,"utf8")).result')
  if [ "$registered" = "$payout" ]; then continue; fi
  validator_seed=$(json_field "$SECRETS/validator-$index.json" secretPhrase)
  relayer_seed=$(json_field "$SECRETS/relayer-1.json" secretPhrase)
  MFC_VALIDATOR_SEED="$validator_seed" \
  MFC_VALIDATOR_PAYOUT="$payout" \
  MFC_RELAYER_SEED="$relayer_seed" \
  MFC_VALIDATOR_SET_ID=0 \
  DEVNET_WS_URL=ws://127.0.0.1:9961 \
    "$BIN/examples/register_validator_payout"
done

for index in 1 2; do
  relayer_seed=$(json_field "$SECRETS/relayer-$index.json" secretPhrase)
  MFC_RELAYER_SEED="$relayer_seed" \
  MFC_RELAYER_QUEUE="$DATA/relayer-$index-queue.json" \
  MFC_RELAYER_START_TARGET=100 \
  MFC_RELAYER_ONCE=0 \
  DEVNET_WS_URL=ws://127.0.0.1:9961 \
    start_or_reload "mfc115-relayer-$index" "$BIN/examples/reward_relayer" --cwd "$ROOT" --time
done

start_or_reload "mfc115-archive" "$NODE" --cwd "$ROOT" --time -- \
  --chain "$RAW" --base-path "$DATA/archive" --pruning archive \
  --rpc-port 9964 --port 30444 --rpc-methods safe --no-mdns --no-prometheus \
  --disable-pow-mining --bootnodes "$BOOTNODE"

pm2 save
rpc_result 9960 chain_getHeader
rpc_result 9960 chain_getFinalizedHead
