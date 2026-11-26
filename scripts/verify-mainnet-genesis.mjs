import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { hexToU8a, u8aToHex } from "@polkadot/util";
import { blake2AsHex, createKeyMulti, cryptoWaitReady, decodeAddress, xxhashAsHex } from "@polkadot/util-crypto";

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
const decodeLe = (bytes) => bytes.reduceRight((result, byte) => result * 256n + BigInt(byte), 0n);
const decodeU128 = (value) => {
  const bytes = hexToU8a(value);
  assert.equal(bytes.length, 16);
  return decodeLe(bytes);
};
const accountPrefix = storageValue("System", "Account");
const accountId = (address) => u8aToHex(decodeAddress(address));
const accountKey = (address) => concat(accountPrefix, blake2AsHex(accountId(address), 128), accountId(address));

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
  assert.equal(bytes.length, 80, `Invalid AccountInfo at ${key}`);
  return {
    key,
    free: decodeLe(bytes.slice(16, 32)),
    reserved: decodeLe(bytes.slice(32, 48)),
    frozen: decodeLe(bytes.slice(48, 64))
  };
});
for (const account of accounts) {
  assert.equal(account.reserved, 0n, `Unexpected reserved balance at ${account.key}`);
  assert.equal(account.frozen, 0n, `Unexpected frozen balance at ${account.key}`);
}
const funded = accounts.filter(({ free }) => free > 0n);
assert.equal(funded.length, 1);
assert.equal(funded[0].key, accountKey(config.rewards.treasuryAccount));
assert.equal(funded[0].free, issuance);
assert.equal(config.endowedAccounts.length, 1);
assert.equal(config.endowedAccounts[0][0], config.rewards.treasuryAccount);
assert.equal(BigInt(config.endowedAccounts[0][1]), issuance);

assert.equal(config.validators.length, 4);
assert.equal(new Set(config.validators).size, config.validators.length);
const authorities = top[storageValue("Grandpa", "Authorities")];
assert.ok(authorities, "GRANDPA authorities are missing");
const authorityBytes = hexToU8a(authorities);
assert.equal(authorityBytes[0], 0x10, "Expected exactly four GRANDPA authorities");
assert.equal(authorityBytes.length, 1 + config.validators.length * 40);
config.validators.forEach((validator, index) => {
  const offset = 1 + index * 40;
  assert.equal(u8aToHex(authorityBytes.slice(offset, offset + 32)), accountId(validator), `GRANDPA authority ${index + 1} differs from config`);
  assert.equal(decodeLe(authorityBytes.slice(offset + 32, offset + 40)), 1n, `GRANDPA authority ${index + 1} has an invalid weight`);
});

assert.equal(config.governanceSignatories.length, 3);
assert.equal(new Set(config.governanceSignatories).size, config.governanceSignatories.length);
const governanceAccount = createKeyMulti(config.governanceSignatories.map((address) => decodeAddress(address)), 2);
assert.equal(top[storageValue("MfcGovernance", "GovernanceAccount")], u8aToHex(governanceAccount), "Governance multisig differs from the configured 2-of-3 signatories");
assert.equal(top[storageValue("MfcRewards", "TreasuryAccount")], accountId(config.rewards.treasuryAccount), "Rewards treasury differs from config");
assert.equal(top[storageValue("MfcRewards", "Enabled")], "0x01");
const activationBlock = hexToU8a(top[storageValue("MfcRewards", "ActivationBlock")]);
assert.equal(activationBlock.length, 4);
assert.equal(decodeLe(activationBlock), BigInt(config.rewards.activationBlock));

console.log(JSON.stringify({
  network: spec.id,
  specVersion: 117,
  validators: config.validators.length,
  governanceSignatories: config.governanceSignatories.length,
  governanceAccount: u8aToHex(governanceAccount),
  totalIssuance: issuance.toString(),
  fundedAccounts: funded.length,
  nonZeroReservedOrFrozenAccounts: 0,
  treasury: config.rewards.treasuryAccount
}, null, 2));
