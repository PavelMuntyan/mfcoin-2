import assert from "node:assert/strict";
import { ApiPromise, WsProvider } from "@polkadot/api";
import { Keyring } from "@polkadot/keyring";
import { cryptoWaitReady } from "@polkadot/util-crypto";
import { readFile } from "node:fs/promises";

const rpcUrl = process.env.MFC_MAINNET_RPC_URL || "ws://127.0.0.1:9980";
const expectedGenesis = process.env.MFC_MAINNET_EXPECTED_GENESIS_HASH;
const treasuryFile = process.env.MFC_MAINNET_TREASURY;
const relayerFiles = [process.env.MFC_MAINNET_RELAYER_1, process.env.MFC_MAINNET_RELAYER_2];
const governanceFiles = (process.env.MFC_MAINNET_GOVERNANCE_FILES || "").split(",").filter(Boolean);
if (!expectedGenesis || !treasuryFile || relayerFiles.some((file) => !file) || governanceFiles.length !== 3) {
  throw new Error("Expected genesis, treasury, two relayers and three governance files are required");
}

await cryptoWaitReady();
const keyring = new Keyring({ ss58Format: 42, type: "sr25519" });
const document = JSON.parse(await readFile(treasuryFile, "utf8"));
const treasury = keyring.addFromUri(document.secretPhrase);
const api = await ApiPromise.create({ provider: new WsProvider(rpcUrl) });
assert.equal(api.genesisHash.toHex(), expectedGenesis, "Refusing to fund a different chain");
assert.equal((await api.rpc.system.chain()).toString(), "MFCoin 2.0 Mainnet");
assert.equal(api.runtimeVersion.specVersion.toNumber(), 117);

async function transfer(destination, amount, label) {
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
    api.tx.balances.transferKeepAlive(destination, amount).signAndSend(treasury, ({ status, dispatchError, events }) => {
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
}

try {
  for (const [index, file] of relayerFiles.entries()) {
    const relayer = JSON.parse(await readFile(file, "utf8"));
    await transfer(relayer.publicKey, 50_000_000_000, `relayer-${index + 1}`);
  }
  for (const [index, file] of governanceFiles.entries()) {
    const governance = JSON.parse(await readFile(file, "utf8"));
    await transfer(governance.publicKey, 10_000_000_000, `governance-${index + 1}`);
  }
} finally {
  await api.disconnect();
}
