# Public security review summary

Last updated: 2026-11-25.

This document records manual and automated project review, including AI-assisted review. It is not a third-party security certification and does not guarantee the absence of vulnerabilities.

## Reviewed scopes

| Component | Reviewed revision |
| --- | --- |
| Blockchain / runtime 116 | `edae5ae` |
| Wallet 0.6.0 | `6481671` |
| Website and Explorer | `04393b5` |
| Runtime 117 candidate | `7cf7e48a30eb084c14e51325839e3ea5bfc8f0c1` |

At the time of this update, Testnet 115 still runs runtime 116. The runtime-117 governance proposal is scheduled for block 33918, and its on-chain WASM matches the reviewed build artifact. Main Net has not launched.

## Confirmed hardening

- Governance supports proposal cancellation, delayed execution, expiry, bounded scheduling and stale-epoch rejection. Runtime upgrades require multisig approval and a timelock; no sudo or admin-mint call remains in the production runtime.
- Main Net uses a newly generated validator and governance set, separate from Testnet. Each validator server stores only its own two validator/payout documents. Treasury, relayer and governance documents are not permanently stored on validator servers.
- The private community archive remains outside public chain storage. Raw passport and MRZ payloads are not exposed by the community application or its public API.
- Public RPC uses keyless archive nodes with health-aware failover. Validator RPC is local-only, and public transaction submission does not expose node key-management methods.
- Asset metadata is sanitized before database storage and JSON rendering, including NUL and invalid Unicode handling. Explorer history remains reorg-aware.
- Wallet boundaries include mandatory backup confirmation, encrypted secrets, password-gated export, watch-only signing rejection, genesis pinning, fee caps and non-automatic rebroadcast after mortal expiry.

## Launch rehearsal

The isolated Main Net rehearsal used four validators on two servers, runtime 117 and exactly 1 MFC at treasury genesis. It confirmed:

- block production and GRANDPA finality;
- finality stopping when two validators went offline and resuming after quorum returned;
- runtime-117 operation and reward accounting;
- no endowed balance other than the 1 MFC treasury account;
- archive synchronization and RPC failover;
- cleanup of rehearsal secrets and chain data.

Production host preparation also includes local-only validator RPC, restricted validator peering, access-restricted local backups, a recovery runbook and post-launch health checks.

## Remaining launch gates

1. Execute the scheduled runtime-117 proposal on Testnet 115 and verify finality, rewards, Explorer and wallet behavior.
2. Start Main Net only with the reviewed raw specification and matching genesis hash on every node.
3. Run the complete six-node launch check, a real transaction broadcast and ongoing monitoring before announcing production availability.

Further independent review, including review by external security professionals, remains welcome. Report vulnerabilities through the process in `SECURITY.md`.

## Public evidence

- [GitHub Actions](https://github.com/PavelMuntyan/mfcoin-2/actions)
- [Regression scripts](https://github.com/PavelMuntyan/mfcoin-2/tree/main/scripts)
- [Main Net operations runbook](https://github.com/PavelMuntyan/mfcoin-2/blob/main/infra/mainnet/RUNBOOK.md)
- [Reproducible build verification](REPRODUCIBLE-BUILDS.md)
- [Main Net candidate configuration](https://github.com/PavelMuntyan/mfcoin-2/blob/main/production.mainnet.json)
