# MFCoin applied product roadmap

Status: all four tracks below are **CONCEPT**. They are not deployed, have no activation dates and do not imply a general smart-contract environment. Current MFC20 native assets use `pallet_assets`; new capabilities may require dedicated runtime pallets, an NFT standard, offchain services, ZK circuits and independent audits.

## Shared design rules

- Keep documents, media, inference and personal data offchain. Use commitments, proofs, payment state and minimal public metadata on chain.
- State clearly which component is decentralized and which is operated by a service provider.
- Do not equate a blockchain timestamp with legal authorship, a patent, identity verification or proof of AI output quality.
- Use versioned cryptography and permissions so the separate PQC migration does not require rewriting product pallets.
- Advance through `CONCEPT`, `DESIGN`, `DEVNET`, `TESTNET` and only then a reviewed `GOVERNANCE ACTIVATION` proposal.

## 1. Proof of Creation & Interactive Media

**Concept status:** architecture candidate built from Freeland commitments, issuer permissions and MFC20 payments.

Provide timestamped proof that a particular work or document existed in a specific form. Store the work and license evidence offchain; anchor a commitment and issuer metadata on chain. Blockchain evidence does not establish legal authorship, originality or patent rights. Patent-sensitive material must never be publicly disclosed through this system.

Later components include licensing, paid access, subscriptions, crowdfunding escrow and revenue distribution. Unique works and NFT rights require a separately reviewed NFT standard rather than fungible `pallet_assets` tokens.

Initial implementation candidate: proof-of-existence registry, followed by media crowdfunding and monetization.

## 2. Agent Payments & Machine Economy

**Concept status:** preferred early applied track, beginning with x402 and Session Keys.

An x402 gateway can issue an HTTP 402 challenge with an amount, recipient, MFC20 asset ID, expiry and unique nonce, then unlock a resource after verifying a finalized payment. It must handle replay, reorgs, quotes, expiration and pending transactions without receiving the payer's secret key.

Session Keys require a dedicated permission pallet: expiry, spending limits, allowed calls, assets and recipients, plus immediate revocation by the primary account. Agent wallets must not expose the primary seed.

An AI proxy marketplace can build on these primitives with metering, escrow, refunds, provider reputation and dispute handling. AI inference remains offchain; blockchain payment does not prove response quality. Start with reviewed providers before considering an open marketplace.

Implementation order: x402 gateway, Session Keys, then the AI marketplace.

## 3. Identity, Credentials & Registries

**Concept status:** requires a separate ZK architecture before implementation.

User registries can reuse typed Freeland commitments, issuer permissions and Merkle proofs. Mutable documents and personal data remain offchain; onchain entries represent versions, status and evidence, not a promise of legal erasure.

ZK identity can support selective statements such as age threshold, credential validity or jurisdiction without exposing the source document. It still depends on issuer trust and requires revocation, recovery, unlinkability and anti-sybil analysis. No raw KYC belongs in public chain storage.

Implementation order: general registries, ZK vendor/circuit evaluation, then a pseudonymous Web3 passport on a devnet.

## 4. Interoperability & Privacy Research

**Concept status:** highest-risk track, deferred until the required security expertise is available.

Token and NFT bridges require explicit custody or light-client designs, validator-key protection, finality rules for both networks, reorg handling, proof verification and economic limits. NFT bridging also depends on a native NFT standard. A multisig bridge must not be presented as trustless and must not hold significant value without an independent audit.

Privacy Pools require circuits, nullifiers, Merkle roots, encrypted notes, relayers, double-spend protection and a separate legal and security review. Research should prioritize confidential transfers with optional selective disclosure rather than claims of complete anonymity or a guaranteed anonymous mixer.

Implementation order: bridge threat model and limited devnet proofs, followed only much later by ZK privacy research and any decision about production suitability.

## Delivery sequence

1. x402, Session Keys and Proof of Creation as the first product designs.
2. General registries, media monetization and the initial AI marketplace.
3. ZK Identity, advanced escrow and the NFT standard.
4. Bridges and Privacy Pools after dedicated security, cryptography and legal review.

Every public roadmap item remains a concept until its corresponding design document, implementation evidence and test network are publicly available.
