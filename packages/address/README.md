# MFCoin address format v0

MFCoin uses Bech32m for user-facing account addresses. The same 32-byte `AccountId` can also be represented as SS58 for interoperability, but `mfcx` and `tmfcx` are the canonical MFCoin formats.

## Networks

- `mfcx`: MFCoin mainnet.
- `tmfcx`: MFCoin testnets.

The HRP separates networks so an address from one network cannot accidentally be used on another.

## Binary payload

The payload is exactly 33 bytes:

```text
version (1 byte, 0x00) || account_id (32 bytes)
```

Version `0` denotes a standard Sr25519 `AccountId32`. Other versions are rejected until explicitly specified. No secret key, seed, personal data or KYC payload is included.

## Text encoding

- Bech32m with the standard Bech32 alphabet and a six-symbol checksum.
- The 33 payload bytes are converted from 8 to 5 bits with zero padding at the end.
- Only lowercase ASCII is accepted. Uppercase and mixed-case addresses are invalid.
- `mfcx` addresses are exactly 64 characters; `tmfcx` addresses are exactly 65 characters.
- Decoder rejects unknown HRPs, wrong lengths, invalid characters, non-zero or incomplete padding, bad checksums, wrong payload lengths and unknown versions.

Addresses are identifiers, not passwords or proof of ownership. Ownership is proven only by signing with the corresponding private key.
