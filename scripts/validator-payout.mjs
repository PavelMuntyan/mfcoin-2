const DOMAIN = new TextEncoder().encode("mfc:validator-payout:v0");

function hashBytes(value, label) {
  if (typeof value !== "string" || !/^0x[0-9a-fA-F]{64}$/.test(value)) {
    throw new Error(`${label} must be a 0x-prefixed 32-byte hash`);
  }
  return Uint8Array.from(Buffer.from(value.slice(2), "hex"));
}

function uint64(value, label) {
  const parsed = BigInt(value);
  if (parsed < 0n || parsed > 0xffffffffffffffffn) throw new Error(`${label} is outside the u64 range`);
  return parsed;
}

export function validatorPayoutPayload({ genesis, setId, authority, payout, nonce }) {
  const genesisBytes = hashBytes(genesis, "Genesis");
  const authorityBytes = hashBytes(authority, "Authority");
  const payoutBytes = hashBytes(payout, "Payout");
  const parsedSetId = uint64(setId, "Set ID");
  const parsedNonce = uint64(nonce, "Nonce");
  const payload = new Uint8Array(1 + DOMAIN.length + 32 + 8 + 32 + 32 + 8);
  const view = new DataView(payload.buffer);
  let offset = 0;
  payload[offset] = DOMAIN.length << 2;
  offset += 1;
  payload.set(DOMAIN, offset);
  offset += DOMAIN.length;
  payload.set(genesisBytes, offset);
  offset += 32;
  view.setBigUint64(offset, parsedSetId, true);
  offset += 8;
  payload.set(authorityBytes, offset);
  offset += 32;
  payload.set(payoutBytes, offset);
  offset += 32;
  view.setBigUint64(offset, parsedNonce, true);
  return payload;
}
