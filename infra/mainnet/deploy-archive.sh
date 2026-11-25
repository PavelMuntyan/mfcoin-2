#!/usr/bin/env bash
set -euo pipefail

ROOT=${MFC_MAINNET_ROOT:-/opt/mfcoin-mainnet}
NODE=${MFC_MAINNET_NODE:-$ROOT/target/release/solochain-template-node}
CONFIG=${MFC_MAINNET_CONFIG:-$ROOT/production.mainnet.json}
BOOTNODE=${MFC_MAINNET_BOOTNODE:-}
RPC_PORT=${MFC_MAINNET_RPC_PORT:-9984}
P2P_PORT=${MFC_MAINNET_P2P_PORT:-30464}

if [[ -z "$BOOTNODE" ]]; then
  echo "MFC_MAINNET_BOOTNODE must point to a Main Net validator" >&2
  exit 1
fi
if [[ ! -x "$NODE" ]]; then
  echo "Main Net node binary is missing: $NODE" >&2
  exit 1
fi
if [[ ! -f "$CONFIG" ]]; then
  echo "Main Net public config is missing: $CONFIG" >&2
  exit 1
fi

mkdir -p "$ROOT/data/archive"
export MFCOIN_PRODUCTION_CONFIG="$CONFIG"
"$NODE" export-chain-spec --raw --chain production > "$ROOT/chain-spec-raw.json"

if pm2 describe mfc-mainnet-archive >/dev/null 2>&1; then
  pm2 delete mfc-mainnet-archive
fi
pm2 start "$NODE" --name mfc-mainnet-archive --cwd "$ROOT" --time -- \
  --chain "$ROOT/chain-spec-raw.json" \
  --base-path "$ROOT/data/archive" \
  --pruning archive \
  --rpc-port "$RPC_PORT" \
  --port "$P2P_PORT" \
  --rpc-methods safe \
  --no-mdns \
  --no-prometheus \
  --disable-pow-mining \
  --bootnodes "$BOOTNODE"
pm2 save

for attempt in $(seq 1 180); do
  if curl -fsS --max-time 3 -H 'content-type: application/json' \
    --data '{"jsonrpc":"2.0","id":1,"method":"chain_getHeader","params":[]}' \
    "http://127.0.0.1:$RPC_PORT" >/dev/null; then
    echo "Main Net archive RPC is ready on 127.0.0.1:$RPC_PORT"
    exit 0
  fi
  sleep 1
done

echo "Main Net archive RPC did not become ready" >&2
exit 1
