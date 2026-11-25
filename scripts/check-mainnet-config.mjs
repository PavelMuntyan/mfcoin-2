import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const config = JSON.parse(readFileSync(new URL("../production.mainnet.json", import.meta.url), "utf8"));
const source = readFileSync(new URL("../production.mainnet.json", import.meta.url), "utf8");
const example = JSON.parse(readFileSync(new URL("../production.example.json", import.meta.url), "utf8"));

assert.equal(config.network.name, "MFCoin 2.0 Mainnet");
assert.equal(config.network.id, "mfcoin2-mainnet");
assert.equal(config.validators.length, 4);
assert.equal(new Set(config.validators).size, config.validators.length);
assert.equal(config.governanceSignatories.length, 3);
assert.equal(new Set(config.governanceSignatories).size, config.governanceSignatories.length);
assert.deepEqual(config.governanceSignatories, [...config.governanceSignatories].sort());
assert.equal(config.endowedAccounts.length, 1);
assert.equal(config.endowedAccounts[0][0], config.rewards.treasuryAccount);
assert.equal(config.endowedAccounts[0][1], 1_000_000_000_000);
assert.ok(config.rewards.activationBlock > 0);
assert.ok(config.validators.every((account) => !example.validators.includes(account)));
assert.ok(config.governanceSignatories.every((account) => !example.governanceSignatories.includes(account)));
assert.ok(!example.endowedAccounts.some(([account]) => account === config.rewards.treasuryAccount));
assert.ok(!/(secretPhrase|privateKey|mnemonic|seedPhrase)/.test(source));

console.log(JSON.stringify({
  network: config.network.id,
  validators: config.validators.length,
  governanceSignatories: config.governanceSignatories.length,
  endowedAccounts: config.endowedAccounts.length,
  treasuryBaseUnits: config.endowedAccounts[0][1],
  activationBlock: config.rewards.activationBlock
}));
