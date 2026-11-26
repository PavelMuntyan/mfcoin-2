# Post-quantum cryptography roadmap

Status: design track, not implemented and not active on Testnet or Main Net. No activation date is promised. Current Sr25519 accounts and Ed25519 GRANDPA authorities remain the launch cryptography.

## Goals and boundaries

The first post-quantum project concerns user-account signatures. It does not replace GRANDPA, change genesis or require a new chain. The goals are to preserve existing account addresses where possible, make migration optional and avoid a flag day for wallets, Explorer and pending transactions.

The immediate threats are future signature forgery and the operational cost of migrating a live network. AES-256-GCM in the wallet vault does not need an emergency replacement. TLS/post-quantum key exchange is a separate infrastructure topic and must not be confused with transaction signatures.

## Candidate primitives

ML-DSA-65 is the initial candidate for user transactions. SLH-DSA is a candidate for infrequent cold governance signatures. These are evaluation choices, not final protocol constants.

| Primitive | Public key | Signature | Intended evaluation |
| --- | ---: | ---: | --- |
| Current Sr25519 | 32 bytes | 64 bytes | Existing accounts |
| ML-DSA-65 | 1952 bytes | 3309 bytes | User transactions |
| SLH-DSA | Parameter-dependent | Much larger than ML-DSA | Cold governance evaluation |

Production selection requires maintained libraries, reproducible builds, published test vectors, side-channel review, safe randomness and secret-memory handling, fuzzing and an independent security review. Do not implement the primitives or a seed-derivation standard in-house.

## Account and address design

Keep the existing 32-byte account identifier and Bech32m address representation. A new PQC-only account identifier can be derived with a versioned, domain-separated hash of its PQC public key. An existing account can retain its current account ID by binding a PQC key through a transaction signed by its current Sr25519 key.

Store the PQC public key on chain once when the binding is revealed. Later extrinsics reference the account ID instead of repeatedly paying for the full public key. Every signing payload must remain bound to the chain genesis, nonce, mortality and call data. Key rotation, algorithm versioning and explicit legacy/hybrid/PQC-only account states must be specified before implementation.

A twelve-word Sr25519 seed must not silently be described as a PQC backup. Any PQC derivation needs a separately reviewed, domain-separated standard and an explicit user backup flow.

## Hybrid migration

The first Testnet migration should require both the current Sr25519 signature and the bound PQC signature for migrated accounts. This tests the transition without immediately removing the existing verification path. PQC-only accounts can follow after hybrid behavior, recovery and tooling are proven.

Migration must cover:

- versioned signature and public-key encoding in runtime and Polkadot API metadata;
- account binding, rotation and watch-only behavior;
- wallet creation, import, backup, export and QR flows;
- multisig and governance signer migration;
- fee previews, runtime weights and full-block behavior with larger signatures;
- pending-transaction reconciliation, mortality and duplicate-submission protection;
- Explorer decoding, history, public API schemas and SDK compatibility;
- rollback to a pre-activation runtime without invalidating already finalized history.

Loss of an old key is not a reason for a privileged recovery path. Governance must not gain the ability to reassign arbitrary user accounts.

## Phases and exit criteria

### 0. Architecture record

Document the candidate primitives, account states, address derivation, migration and compatibility requirements. Make no change to launch genesis or production signatures.

### 1. Isolated laboratory

Compare maintained implementations and produce deterministic test vectors, reproducible builds and memory/CPU measurements. Benchmark key generation, signing and verification on supported desktop, Android and runtime/WASM targets. Fuzz malformed keys, signatures and encoded extrinsics.

Exit when the implementation and parameter choices pass cryptographic and operational review.

### 2. PQC devnet

Implement versioned account binding and hybrid verification on an isolated network. Measure verification weight, block utilization, bandwidth, archive growth and fees with empty, typical and full blocks. Exercise reorgs, finality stalls, runtime rollback and expired transactions.

Exit when reproducible results are published and no unresolved consensus, finality or duplicate-signing issue remains.

### 3. Public Testnet migration

Enable hybrid accounts on Testnet with real Chrome and Android flows, Explorer, SDK examples and recovery drills. Run the complete wallet matrix: create, bind, receive, sign, pending, finalize, rotate and watch-only. Conduct an independent security audit and resolve its findings.

### 4. Optional Main Net activation

Activate only through the reviewed governance process after the Testnet evidence is accepted. Existing Sr25519 accounts remain valid; migration is opt-in. Publish the runtime artifact, test vectors, weight changes and user migration guide before the proposal.

### 5. Separate consensus research

Evaluate post-quantum GRANDPA authorities, vote size, evidence verification and finality recovery independently of account migration. This phase requires upstream-compatible production evidence and must not be implied by hybrid account support.

## Public communication rules

Do not claim that MFCoin is quantum-resistant while this is a design track. Describe ML-DSA and SLH-DSA as candidates until their parameters and implementations are reviewed. Distinguish hybrid account signatures from post-quantum consensus, and never present a roadmap phase as deployed functionality.
