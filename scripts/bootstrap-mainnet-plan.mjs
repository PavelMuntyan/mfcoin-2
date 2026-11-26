import assert from "node:assert/strict";

export function shouldFund({ label, free, nonce, amount }) {
  if (free >= amount) return false;
  assert.equal(free, 0n, `${label}: partial balance requires manual review; refusing to top up`);
  assert.equal(nonce, 0n, `${label}: empty account has already been used; refusing to fund again`);
  return true;
}
