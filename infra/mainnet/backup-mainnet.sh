#!/usr/bin/env bash
set -euo pipefail

ROOT=${MFC_MAINNET_ROOT:-/opt/mfcoin-mainnet}
DATA=${MFC_MAINNET_DATA:-/var/lib/mfcoin-mainnet}
BACKUP_ROOT=${MFC_MAINNET_BACKUP_ROOT:-/var/backups/mfcoin-mainnet}
KEEP=${MFC_MAINNET_BACKUP_KEEP:-14}

if [[ ! "$KEEP" =~ ^[1-9][0-9]*$ ]]; then
  echo "MFC_MAINNET_BACKUP_KEEP must be a positive integer" >&2
  exit 1
fi
for required in "$ROOT/production.mainnet.json" "$ROOT/chain-spec-raw.json" "$ROOT/secrets"; do
  if [[ ! -e "$required" ]]; then
    echo "Required Main Net backup input is missing: $required" >&2
    exit 1
  fi
done

install -d -m 700 "$BACKUP_ROOT"
timestamp=$(date -u +%Y%m%dT%H%M%SZ)
stage=$(mktemp -d "$BACKUP_ROOT/.staging-$timestamp.XXXXXX")
archive="$BACKUP_ROOT/mainnet-$timestamp.tar.gz"
trap 'rm -rf "$stage"' EXIT
chmod 700 "$stage"
mkdir -p "$stage/config" "$stage/network-keys"
cp -a "$ROOT/production.mainnet.json" "$ROOT/chain-spec-raw.json" "$ROOT/secrets" "$stage/config/"
if [[ -f "$ROOT/founder-payouts.mainnet.json" ]]; then
  cp -a "$ROOT/founder-payouts.mainnet.json" "$stage/config/"
fi
if [[ -f "$ROOT/target/release/solochain-template-node" ]]; then
  "$ROOT/target/release/solochain-template-node" --version > "$stage/node-version.txt"
fi
if [[ -d "$DATA" ]]; then
  while IFS= read -r -d '' key; do
    validator=$(basename "$(dirname "$(dirname "$(dirname "$(dirname "$key")")")")")
    install -D -m 600 "$key" "$stage/network-keys/$validator-secret_ed25519"
  done < <(find "$DATA" -path '*/chains/*/network/secret_ed25519' -type f -print0)
fi
if [[ -f "$HOME/.pm2/dump.pm2" ]]; then
  cp -a "$HOME/.pm2/dump.pm2" "$stage/pm2-dump.pm2"
fi
chmod -R go-rwx "$stage"
tar -C "$stage" -czf "$archive" .
chmod 600 "$archive"
tar -tzf "$archive" >/dev/null
sha256sum "$archive" > "$archive.sha256"
chmod 600 "$archive.sha256"

mapfile -t archives < <(find "$BACKUP_ROOT" -maxdepth 1 -name 'mainnet-*.tar.gz' -type f -printf '%T@ %p\n' | sort -rn | cut -d' ' -f2-)
for ((index = KEEP; index < ${#archives[@]}; index++)); do
  rm -f "${archives[$index]}" "${archives[$index]}.sha256"
done
printf 'Main Net backup created: %s\n' "$archive"
