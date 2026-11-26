import assert from "node:assert/strict";
import { ApiPromise, WsProvider } from "@polkadot/api";
import { Keyring } from "@polkadot/keyring";
import { cryptoWaitReady } from "@polkadot/util-crypto";
import { readFile } from "node:fs/promises";
import { shouldFund } from "./bootstrap-mainnet-plan.mjs";

const rpcUrl = process.env.MFC_MAINNET_RPC_URL || "ws://127.0.0.1:9980";
const expectedGenesis = process.env.MFC_MAINNET_EXPECTED_GENESIS_HASH;
const treasuryFile = process.env.MFC_MAINNET_TREASURY;
const relayerFiles = [process.env.MFC_MAINNET_RELAYER_1, process.env.MFC_MAINNET_RELAYER_2];
if (!expectedGenesis || !treasuryFile || relayerFiles.some((file) => !file)) {
  throw new Error("Expected genesis, treasury and two relayers are required");
}
const configPath = process.env.MFC_MAINNET_CONFIG || new URL("../production.mainnet.json", import.meta.url).pathname;
const config = JSON.parse(await readFile(configPath, "utf8"));
assert.equal(config.governanceSignatories.length, 3);

await cryptoWaitReady();
const keyring = new Keyring({ ss58Format: 42, type: "sr25519" });
const document = JSON.parse(await readFile(treasuryFile, "utf8"));
const treasury = keyring.addFromUri(document.secretPhrase);
const recipients = [];
for (const [index, file] of relayerFiles.entries()) {
  const relayer = JSON.parse(await readFile(file, "utf8"));
  recipients.push({ destination: relayer.publicKey, amount: 50_000_000_000n, label: `relayer-${index + 1}` });
}
for (const [index, address] of config.governanceSignatories.entries()) {
  recipients.push({ destination: address, amount: 10_000_000_000n, label: `governance-${index + 1}` });
}
const addresses = recipients.map(({ destination }) => keyring.encodeAddress(destination));
assert.equal(new Set(addresses).size, recipients.length, "Bootstrap recipients must be unique");
assert.ok(!addresses.includes(treasury.address), "Treasury cannot be a bootstrap recipient");

const rpcHost = process.env.MFC_MAINNET_RPC_HOST;
const api = await ApiPromise.create({ provider: new WsProvider(rpcUrl, undefined, rpcHost ? { Host: rpcHost } : {}) });
assert.equal(api.genesisHash.toHex(), expectedGenesis, "Refusing to fund a different chain");
assert.equal((await api.rpc.system.chain()).toString(), "MFCoin 2.0 Mainnet");
assert.equal(api.runtimeVersion.specVersion.toNumber(), 117);

async function transfer({ destination, amount, label }) {
  const account = await api.query.system.account(destination);
  const free = account.data.free.toBigInt();
  if (!shouldFund({ label, free, nonce: account.nonce.toBigInt(), amount })) {
    console.log(`${label}: already funded; skipping`);
    return false;
  }
  const nonce = (await api.rpc.system.accountNextIndex(treasury.address)).toBigInt();
  await new Promise((resolve, reject) => {
    let unsubscribe;
    let settled = false;
    const finish = (error) => {
      if (settled) return;
      settled = true;
      unsubscribe?.();
      if (error) reject(error);
      else resolve();
    };
    api.tx.balances.transferKeepAlive(destination, amount).signAndSend(treasury, { nonce }, ({ status, dispatchError, events }) => {
      if (dispatchError) {
        finish(new Error(`${label}: ${dispatchError.toString()}`));
        return;
      }
      if (!status.isFinalized) return;
      if (!events.some(({ event }) => api.events.system.ExtrinsicSuccess.is(event))) {
        finish(new Error(`${label}: no ExtrinsicSuccess event`));
        return;
      }
      console.log(`${label}: funded in ${status.asFinalized.toHex()}`);
      finish();
    }).then((stop) => { unsubscribe = stop; }).catch(finish);
  });
  const nextNonce = (await api.rpc.system.accountNextIndex(treasury.address)).toBigInt();
  assert.equal(nextNonce, nonce + 1n, `${label}: unexpected treasury nonce after finalization`);
  return true;
}

const initialNonce = (await api.rpc.system.accountNextIndex(treasury.address)).toBigInt();
let submitted = 0;
let skipped = 0;
try {
  for (const recipient of recipients) {
    if (await transfer(recipient)) submitted += 1;
    else skipped += 1;
  }
  const finalNonce = (await api.rpc.system.accountNextIndex(treasury.address)).toBigInt();
  assert.equal(finalNonce, initialNonce + BigInt(submitted), "Treasury nonce does not match the number of submitted transfers");
  console.log(JSON.stringify({ submitted, skipped, treasuryNonce: finalNonce.toString() }));
} finally {
  await api.disconnect();
}
