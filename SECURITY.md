# Security policy

## Scope

Security reports are welcome for the node, runtime, native pallets, address codecs, reward relayer, public chain specifications and Main Net operations scripts in this repository. Wallet-specific reports belong in the [MFCoin wallet repository](https://github.com/PavelMuntyan/mfcoin-wallet).

## Reporting a vulnerability

Use GitHub Security Advisories for a private report instead of opening a public issue. Include the affected commit, impact, reproduction steps and a minimal proof of concept where possible. Do not include real seed phrases, private keys, passport payloads, user records or production credentials in a report.

Do not test against private user data or disrupt public nodes. Use a local development chain for consensus, runtime and transaction proofs unless a public testnet test is explicitly coordinated.

## Disclosure

Allow maintainers time to reproduce the issue, prepare a fix and coordinate any required governance or validator action before public disclosure. Security fixes should identify the affected runtime or source commit and include a reproducible verification path.

The public review history is documented in [docs/AUDIT.md](docs/AUDIT.md). That document is not a third-party security certification.
