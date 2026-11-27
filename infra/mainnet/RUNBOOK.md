# MFCoin Main Net runbook

Main Net must not start until Testnet 115 has executed runtime 117 after its scheduled block and passed finality, reward, Explorer and wallet checks. Staging files and exporting a chain spec do not start a chain.

## Layout and trust boundaries

| Server | Validators | Validator RPC | Validator P2P | Archive RPC / P2P |
| --- | --- | --- | --- | --- |
| Primary `65.108.230.60` | 1, 2 | 9980, 9981 | 30460, 30461 | 9984 / 30464 |
| Failover `138.201.131.105` | 3, 4 | 9982, 9983 | 30462, 30463 | 9985 / 30465 |

Both servers stage files in `/opt/mfcoin-mainnet`. Chain data is in `/var/lib/mfcoin-mainnet`.

Each server stores only its own two validator and payout documents. Treasury and relayer documents stay in the protected deployment secret store and are exposed only to the short-lived launch job. Governance documents remain in independent signer vaults and must not be uploaded to GitHub, workflow artifacts or either host. Bootstrap needs only the public governance addresses from `production.mainnet.json`. Never print secret JSON, seed phrases or private keys. Validator RPC ports 9980–9985 are local-only. Public clients use the archive gateway, never a validator. On a host that also serves the website, set `MFC_WEB_PORTS="80 443"` when configuring UFW; never enable default-deny incoming without preserving existing public services.

## Governance custody

Before launch, at least two signers must each hold their own document in an independent 1Password or Bitwarden vault under their control, not a shared project vault. Each signer creates an encrypted offsite backup, stores its decryption passphrase separately and performs a restore onto a clean device. The restored document is valid only when its public key matches the corresponding `governanceSignatories` entry; never compare or transmit the secret phrase in chat or logs. Remove the governance documents from the shared GitHub environment after these restores are confirmed. Runtime approvals must use the independent signer copies.

## Pre-launch

1. Confirm the Testnet 115 runtime-117 proposal executed and all post-upgrade checks passed.
2. Run `scripts/check-mainnet-config.mjs` and export the raw spec on both servers with `MFCOIN_PRODUCTION_CONFIG` pointing to `production.mainnet.json`.
3. Compare raw-spec SHA-256 hashes. Run `scripts/verify-mainnet-genesis.mjs`; it must report runtime 117, the exact four configured authority IDs, the configured governance 2-of-3 account, exactly one funded account, zero reserved/frozen balances and `1000000000000` base units (1 MFC).
4. Record the expected genesis hash after starting validator 1. Every other node and every launch check must use the same hash.
5. Confirm both servers pass `pm2 status`, have time synchronized, have at least 20 GiB free, and have a fresh backup. Confirm UFW and the local-only RPC rules.

## Start

Run node and backup commands as `root`, not as the web deployment user.

```bash
MFC_MAINNET_START=1 MFC_MAINNET_DATA=/var/lib/mfcoin-mainnet \
  bash /opt/mfcoin-mainnet/deploy-mainnet-validators.sh
```

Read validator 1's public peer ID from its node key, then start validators 3 and 4 on failover with:

```bash
MFC_MAINNET_START=3 MFC_MAINNET_BOOTNODE=/ip4/65.108.230.60/tcp/30460/p2p/PEER_ID \
  bash /opt/mfcoin-mainnet/deploy-mainnet-validators.sh
```

Do not start a validator with an empty or reused chain database from another network. PoW payout destinations come only from the six public `founderPayouts` entries in `founder-payouts.mainnet.json`, which is deliberately separate from the strict genesis config: validator indices 1–4 map to Founder 01–04. The older `payout-*.json` documents remain protected custody/backups for historical recipients but are not read by the Main Net validator start script. After the four validators finalize blocks, start one archive per server with `MFC_MAINNET_BOOTNODE` set to a validator multiaddress. Primary uses RPC/P2P 9984/30464 and failover 9985/30465.

Run the protected bootstrap job after finality is confirmed. It verifies the genesis hash and runtime, then uses `transferKeepAlive` to fund two relayers with 0.05 MFC each and three governance signers with 0.01 MFC each. Already funded recipients are skipped on a retry; fresh zero-balance recipients can be resumed after a partial failure, while partial balances and used empty accounts require manual review. Each submission pins the treasury nonce and verifies it after finalization. Register validator payouts and start reward relayers only after this succeeds. The protected `Register Main Net founder payouts` workflow pins the reviewed `mfcoin-2` revision and expected genesis, uses each host's validator documents without printing them, obtains the current registration nonce through RPC and confirms the Founder 01–04 destination at the finalized block. Finally run `Verify Main Net launch` with the expected genesis hash; it checks all six nodes, finality, GRANDPA evidence, rewards, archive synchronization and `author_submitExtrinsic`.

## Routine checks and backups

`monitor-mainnet.sh` checks local RPC health, peers, synchronization and advancing finality. It writes a mode-600 JSON status file and exits nonzero on failure. Run it every five minutes from root's crontab and send nonzero exits to the system logger.

`backup-mainnet.sh` stores configuration, that server's validator/payout secrets, network keys and PM2 state in `/var/backups/mfcoin-mainnet`. Archives and checksums are mode 600; the directory is mode 700. It keeps 14 local archives. Copy backups to separate encrypted offline storage through a controlled process; never upload them to GitHub artifacts or the public web root.

Before upgrading or replacing a node, create a backup and verify it with `tar -tzf` and `sha256sum -c`.

## Stop, cancel and recover

For a planned stop, use `pm2 stop` on the Main Net archive and validator processes, then `pm2 save`. Stop archives before validators. Do not delete chain data.

To cancel a not-yet-public launch, stop all `mfc-mainnet-*` processes on both servers, disable any Main Net gateway virtual host, and preserve data and backups for diagnosis. Do not reuse the same genesis after a public launch; a restart requires an explicitly versioned chain ID and newly reviewed configuration.

To replace a server, stage the same reviewed binary and raw spec, restore only that server's validator/payout documents and network keys, verify file modes, then start its validators with the surviving server as bootnode. Confirm finality before starting the archive. If validator keys are lost or compromised, stop and use the governance GRANDPA-rotation procedure; never silently substitute new authorities.

For data recovery, restore into a separate directory first, compare configuration and key hashes with the backup, and avoid running two copies of the same validator key simultaneously.

## Runtime upgrade

Prepare and test the new runtime on Testnet first. Use the reviewed governance 2-of-3 workflow with a pinned artifact run ID and proposal ID. Verify the proposal's code hash, execution block and expiry before signing. After execution, check `specVersion`, best and finalized block progress, GRANDPA evidence, reward totals, archive synchronization, Explorer and wallet. A failed Testnet upgrade blocks Main Net launch or the corresponding Main Net upgrade.
