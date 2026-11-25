import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { hexToU8a, u8aToHex } from "@polkadot/util";
import { blake2AsHex, cryptoWaitReady, decodeAddress, xxhashAsHex } from "@polkadot/util-crypto";

const configPath = process.env.MFC_MAINNET_CONFIG || new URL("../production.mainnet.json", import.meta.url).pathname;
const specPath = process.env.MFC_MAINNET_RAW_SPEC;
if (!specPath) throw new Error("MFC_MAINNET_RAW_SPEC is required");
const config = JSON.parse(readFileSync(configPath, "utf8"));
const spec = JSON.parse(readFileSync(specPath, "utf8"));
const top = spec.genesis?.raw?.top;
assert.ok(top, "Raw genesis storage is missing");
await cryptoWaitReady();

const concat = (...values) => `0x${values.map((value) => value.slice(2)).join("")}`;
const storageValue = (pallet, item) => concat(xxhashAsHex(pallet, 128), xxhashAsHex(item, 128));
const decodeU128 = (value) => {
  const bytes = hexToU8a(value);
  assert.equal(bytes.length, 16);
  return bytes.reduceRight((result, byte) => result * 256n + BigInt(byte), 0n);
};
const accountPrefix = storageValue("System", "Account");
const accountKey = (address) => {
  const id = u8aToHex(decodeAddress(address));
  return concat(accountPrefix, blake2AsHex(id, 128), id);
};

assert.equal(spec.name, "MFCoin 2.0 Mainnet");
assert.equal(spec.id, "mfcoin2-mainnet");
assert.equal(spec.chainType, "Live");
assert.deepEqual(spec.properties, { ss58Format: 42, tokenDecimals: 12, tokenSymbol: "MFC" });
assert.equal(top[storageValue("Sudo", "Key")], undefined);
assert.equal(top[storageValue("Aura", "Authorities")], undefined);

const upgrade = top[storageValue("System", "LastRuntimeUpgrade")];
assert.ok(upgrade, "System.LastRuntimeUpgrade is missing");
const compact = hexToU8a(upgrade);
assert.equal(compact[0] & 3, 1, "Expected a two-byte compact spec version");
assert.equal(((compact[0] | (compact[1] << 8)) - 1) / 4, 117);

const issuance = decodeU128(top[storageValue("Balances", "TotalIssuance")]);
assert.equal(issuance, 1_000_000_000_000n);
const accounts = Object.entries(top).filter(([key]) => key.startsWith(accountPrefix)).map(([key, value]) => {
  const bytes = hexToU8a(value);
  assert.ok(bytes.length >= 32, `Invalid AccountInfo at ${key}`);
  return { key, free: decodeU128(u8aToHex(bytes.slice(16, 32))) };
});
const funded = accounts.filter(({ free }) => free > 0n);
assert.equal(funded.length, 1);
assert.equal(funded[0].key, accountKey(config.rewards.treasuryAccount));
assert.equal(funded[0].free, issuance);
assert.equal(config.endowedAccounts.length, 1);
assert.equal(config.endowedAccounts[0][0], config.rewards.treasuryAccount);
assert.equal(BigInt(config.endowedAccounts[0][1]), issuance);

const authorities = top[storageValue("Grandpa", "Authorities")];
assert.ok(authorities, "GRANDPA authorities are missing");
assert.equal(authorities.slice(0, 4), "0x10");
assert.equal((authorities.length - 4) / 2, 4 * 40);
assert.equal(config.validators.length, 4);
assert.equal(config.governanceSignatories.length, 3);

console.log(JSON.stringify({
  network: spec.id,
  specVersion: 117,
  validators: config.validators.length,
  governanceSignatories: config.governanceSignatories.length,
  totalIssuance: issuance.toString(),
  fundedAccounts: funded.length,
  treasury: config.rewards.treasuryAccount
}, null, 2));
