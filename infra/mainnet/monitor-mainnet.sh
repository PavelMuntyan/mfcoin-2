#!/usr/bin/env bash
set -euo pipefail

PORTS=${MFC_MAINNET_MONITOR_PORTS:-9980 9981 9984}
STATUS=${MFC_MAINNET_MONITOR_STATUS:-/var/lib/mfcoin-mainnet/monitor-status.json}
STATE=${MFC_MAINNET_MONITOR_STATE:-/var/lib/mfcoin-mainnet/monitor-last-finalized}
MIN_PEERS=${MFC_MAINNET_MIN_PEERS:-1}
install -d -m 700 "$(dirname "$STATUS")"

rpc() {
  local port=$1 method=$2
  curl -fsS --max-time 5 -H 'content-type: application/json' \
    --data "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"$method\",\"params\":[]}" \
    "http://127.0.0.1:$port"
}

previous=$(cat "$STATE" 2>/dev/null || echo 0)
maximum=0
results=''
for port in $PORTS; do
  health=$(rpc "$port" system_health)
  peers=$(node -pe 'JSON.parse(require("fs").readFileSync(0, "utf8")).result.peers' <<< "$health")
  syncing=$(node -pe 'JSON.parse(require("fs").readFileSync(0, "utf8")).result.isSyncing' <<< "$health")
  hash=$(rpc "$port" chain_getFinalizedHead | node -pe 'JSON.parse(require("fs").readFileSync(0, "utf8")).result')
  finalized=$(rpc "$port" chain_getHeader "[\"$hash\"]" | node -pe 'parseInt(JSON.parse(require("fs").readFileSync(0, "utf8")).result.number, 16)')
  if ((peers < MIN_PEERS)) || [[ "$syncing" == "true" ]]; then
    echo "Main Net RPC $port is unhealthy: peers=$peers syncing=$syncing" >&2
    exit 1
  fi
  if ((finalized > maximum)); then maximum=$finalized; fi
  results="$results{\"port\":$port,\"peers\":$peers,\"finalized\":$finalized},"
done
if ((maximum <= previous)); then
  echo "Main Net finality did not advance: previous=$previous current=$maximum" >&2
  exit 1
fi
printf '%s\n' "$maximum" > "$STATE"
chmod 600 "$STATE"
printf '{"checkedAt":"%s","endpoints":[%s]}\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "${results%,}" > "$STATUS.tmp"
chmod 600 "$STATUS.tmp"
mv "$STATUS.tmp" "$STATUS"
cat "$STATUS"
