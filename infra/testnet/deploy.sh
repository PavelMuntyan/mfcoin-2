#!/usr/bin/env bash
set -euo pipefail

ROOT=/var/www/balexey/data/mfcoin-testnet
BIN="$ROOT/target/release"
NODE="$BIN/solochain-template-node"
SECRETS="$ROOT/secrets"
DATA="$ROOT/data"
OPERATORS="$ROOT/operators.json"
RAW="$ROOT/chain-spec-raw.json"
mkdir -p "$DATA"

if [ ! -f "$OPERATORS" ]; then
  if [ -e "$SECRETS" ]; then
    echo "Refusing to regenerate an incomplete secrets directory" >&2
    exit 1
  fi
  "$BIN/examples/generate_testnet_keys" "$SECRETS"
  cp "$SECRETS/operators.json" "$OPERATORS"
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
    if rpc_result "$port" chain_getHeader >/dev/null 2>&1; then
      return 0
    fi
    sleep 0.5
  done
  echo "RPC did not start on port $port" >&2
  return 1
}

BOOT_PIDS=()
stop_bootstrap() {
  for pid in "${BOOT_PIDS[@]:-}"; do
    kill -INT "$pid" 2>/dev/null || true
  done
}
trap stop_bootstrap EXIT

BOOTNODE=""
for index in 1 2 3 4; do
  rpc_port=$((9943 + index))
  p2p_port=$((30332 + index))
  base="$DATA/validator-$index"
  mkdir -p "$base"
  "$NODE" key generate-node-key --base-path "$base" --chain "$RAW" >/dev/null
  "$NODE" --validator --chain "$RAW" --base-path "$base" \
    --rpc-port "$rpc_port" --port "$p2p_port" --rpc-methods unsafe \
    --no-mdns --no-prometheus --disable-pow-mining \
    > "$DATA/validator-$index-bootstrap.log" 2>&1 &
  BOOT_PIDS+=("$!")
  wait_for_rpc "$rpc_port"
  public_key=$(json_field "$SECRETS/validator-$index.json" publicKey)
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

export MFCOIN_GRANDPA_JUSTIFICATION_PERIOD=1
for index in 1 2 3 4; do
  rpc_port=$((9943 + index))
  p2p_port=$((30332 + index))
  payout=$(json_field "$SECRETS/payout-$index.json" publicKey)
  args=(
    --validator --chain "$RAW" --base-path "$DATA/validator-$index"
    --rpc-port "$rpc_port" --port "$p2p_port"
    --no-mdns --no-prometheus --pow-payout-account "$payout"
  )
  if [ "$index" -gt 1 ]; then
    args+=(--bootnodes "$BOOTNODE")
  fi
  pm2 startOrReload "$NODE" --name "mfc112-validator-$index" --cwd "$ROOT" --time -- "${args[@]}"
done

for index in 1 2; do
  relayer_seed=$(json_field "$SECRETS/relayer-$index.json" secretPhrase)
  MFC_RELAYER_SEED="$relayer_seed" \
  MFC_RELAYER_QUEUE="$DATA/relayer-$index-queue.json" \
  MFC_RELAYER_START_TARGET=100 \
  MFC_RELAYER_ONCE=0 \
  DEVNET_WS_URL=ws://127.0.0.1:9945 \
    pm2 startOrReload "$BIN/examples/reward_relayer" \
      --name "mfc112-relayer-$index" --cwd "$ROOT" --time
done

MFCOIN_CHAIN_SPEC="$OPERATORS" MFCOIN_RPC_URL=http://127.0.0.1:9944 \
  pm2 restart mf-life-community --update-env
pm2 save

rpc_result 9944 chain_getHeader
rpc_result 9944 chain_getFinalizedHead
