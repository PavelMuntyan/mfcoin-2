#!/usr/bin/env bash
set -euo pipefail

PEER_IP=${MFC_MAINNET_PEER_IP:?MFC_MAINNET_PEER_IP is required}
SSH_PORT=${MFC_SSH_PORT:-22}
VALIDATOR_PORTS=${MFC_MAINNET_VALIDATOR_PORTS:?MFC_MAINNET_VALIDATOR_PORTS is required}
ARCHIVE_PORT=${MFC_MAINNET_ARCHIVE_PORT:-30464}
WEB_PORTS=${MFC_WEB_PORTS:-}

if [[ ! "$PEER_IP" =~ ^[0-9a-fA-F.:]+$ ]]; then
  echo "MFC_MAINNET_PEER_IP must be an IP address" >&2
  exit 1
fi
for port in "$SSH_PORT" "$ARCHIVE_PORT" $VALIDATOR_PORTS $WEB_PORTS; do
  if [[ ! "$port" =~ ^[0-9]+$ ]] || ((port < 1 || port > 65535)); then
    echo "Invalid TCP port: $port" >&2
    exit 1
  fi
done

command -v ufw >/dev/null
ufw allow "$SSH_PORT/tcp" comment 'MFCoin administrative SSH'
for port in $WEB_PORTS; do
  ufw allow "$port/tcp" comment 'MFCoin public web'
done
for port in $VALIDATOR_PORTS; do
  ufw allow from "$PEER_IP" to any port "$port" proto tcp comment 'MFCoin Main Net validator peer'
done
ufw allow "$ARCHIVE_PORT/tcp" comment 'MFCoin Main Net archive P2P'
ufw deny 9980:9985/tcp comment 'MFCoin Main Net RPC is local only'
ufw --force enable
ufw status verbose
