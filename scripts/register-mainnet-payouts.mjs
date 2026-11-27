import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { ApiPromise, WsProvider } from "@polkadot/api";
import { Keyring } from "@polkadot/keyring";
import { cryptoWaitReady } from "@polkadot/util-crypto";
import { u8aToHex } from "@polkadot/util";
import WebSocket from "ws";
import { validatorPayoutPayload } from "./validator-payout.mjs";

globalThis.WebSocket = WebSocket;

const rpcUrl = process.env.MFC_MAINNET_RPC_URL || "ws://127.0.0.1:9980";
const expectedGenesis = process.env.MFC_MAINNET_EXPECTED_GENESIS_HASH;
const relayerFile = process.env.MFC_MAINNET_RELAYER;
const validatorFiles = (process.env.MFC_MAINNET_VALIDATOR_FILES || "").split(":").filter(Boolean);
const validatorIndexes = (process.env.MFC_MAINNET_VALIDATOR_INDEXES || "").split(",").filter(Boolean).map(Number);
if (!expectedGenesis || !relayerFile || validatorFiles.length === 0 || validatorFiles.length !== validatorIndexes.length) {
  throw new Error("Expected genesis, one relayer and matching validator files/indexes are required");
}
assert.ok(validatorIndexes.every(index => Number.isInteger(index) && index >= 1 && index <= 4));
assert.equal(new Set(validatorIndexes).size, validatorIndexes.length);
const configPath = process.env.MFC_MAINNET_CONFIG || new URL("../production.mainnet.json", import.meta.url).pathname;
const founderConfigPath = process.env.MFC_MAINNET_FOUNDER_CONFIG || new URL("../founder-payouts.mainnet.json", import.meta.url).pathname;
const config = JSON.parse(await readFile(configPath, "utf8"));
const { founderPayouts } = JSON.parse(await readFile(founderConfigPath, "utf8"));
assert.equal(founderPayouts.length, 4);
assert.equal(new Set(founderPayouts).size, 4);

await cryptoWaitReady();
const ed25519 = new Keyring({ ss58Format: 42, type: "ed25519" });
const sr25519 = new Keyring({ ss58Format: 42, type: "sr25519" });
const validators = [];
for (const [position, file] of validatorFiles.entries()) {
  const document = JSON.parse(await readFile(file, "utf8"));
  const pair = ed25519.addFromUri(document.secretPhrase);
  validators.push({ index: validatorIndexes[position], pair, publicKey: u8aToHex(pair.publicKey) });
}
const relayerDocument = JSON.parse(await readFile(relayerFile, "utf8"));
const relayer = sr25519.addFromUri(relayerDocument.secretPhrase);
assert.equal(sr25519.encodeAddress(relayer.publicKey), relayerDocument.publicKey, "Relayer public key does not match its secret document");

const rpcHost = process.env.MFC_MAINNET_RPC_HOST;
const provider = new WsProvider(rpcUrl, undefined, rpcHost ? { Host: rpcHost } : {});
const api = await ApiPromise.create({ provider });
try {
  assert.equal(api.genesisHash.toHex(), expectedGenesis, "Refusing to register payouts on a different chain");
  assert.equal((await api.rpc.system.chain()).toString(), "MFCoin 2.0 Mainnet");
  const setId = (await api.query.grandpa.currentSetId()).toBigInt();
  assert.ok(setId <= BigInt(Number.MAX_SAFE_INTEGER));
  const entries = (await api.query.mfcRewards.validatorSets(setId)).toJSON();
  assert.ok(Array.isArray(entries));
  assert.equal(entries.length, config.validators.length);
  const authorities = new Map(entries.map(([authority]) => {
    assert.match(authority, /^0x[0-9a-f]{64}$/);
    return [authority, true];
  }));
  for (const validator of validators) {
    assert.ok(authorities.has(validator.publicKey), `Validator ${validator.index} is not a GRANDPA authority in set ${setId}`);
  }

  for (const validator of validators) {
    const index = validator.index;
    const authority = validator.publicKey;
    const payout = founderPayouts[index - 1];
    assert.ok(authorities.has(authority), `Validator ${index} is not a GRANDPA authority in set ${setId}`);
    assert.equal(sr25519.encodeAddress(payout), payout, `Founder ${index} payout is invalid`);
    const rpcSetId = Number(setId);
    const registered = await provider.send("mfc_validatorPayout", [rpcSetId, authority, null]);
    if (registered !== null && sr25519.encodeAddress(registered) === payout) {
      console.log(JSON.stringify({ validator: index, setId: setId.toString(), registered: true, alreadyPresent: true }));
      continue;
    }
    const nonce = BigInt(await provider.send("mfc_validatorPayoutNonce", [rpcSetId, authority, null]));
    const payload = validatorPayoutPayload({ genesis: expectedGenesis, setId, authority, payout: u8aToHex(sr25519.decodeAddress(payout)), nonce });
    const signature = u8aToHex(validator.pair.sign(payload));
    const relayerNonce = (await api.rpc.system.accountNextIndex(relayer.address)).toBigInt();
    const registrationBlock = await new Promise((resolve, reject) => {
      let unsubscribe;
      let settled = false;
      const finish = (error, blockHash) => {
        if (settled) return;
        settled = true;
        unsubscribe?.();
        if (error) reject(error);
        else resolve(blockHash);
      };
      api.tx.mfcRewards.registerValidatorPayout(setId.toString(), authority, payout, nonce.toString(), signature)
        .signAndSend(relayer, { nonce: relayerNonce }, ({ status, dispatchError, events }) => {
          if (dispatchError) {
            finish(new Error(`Validator ${index}: ${dispatchError.toString()}`));
            return;
          }
          if (!status.isFinalized) return;
          if (!events.some(({ event }) => api.events.system.ExtrinsicSuccess.is(event))) {
            finish(new Error(`Validator ${index}: no ExtrinsicSuccess event`));
            return;
          }
          finish(null, status.asFinalized.toHex());
        }).then(stop => { unsubscribe = stop; }).catch(finish);
    });
    const confirmed = await provider.send("mfc_validatorPayout", [rpcSetId, authority, registrationBlock]);
    assert.ok(confirmed !== null && sr25519.encodeAddress(confirmed) === payout, `Validator ${index}: finalized payout mismatch`);
    console.log(JSON.stringify({ validator: index, setId: setId.toString(), nonce: nonce.toString(), registered: true, alreadyPresent: false, block: registrationBlock }));
  }
} finally {
  await api.disconnect();
}
