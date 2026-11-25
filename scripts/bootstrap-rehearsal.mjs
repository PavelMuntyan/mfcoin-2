import { ApiPromise, WsProvider } from "@polkadot/api";
import { Keyring } from "@polkadot/keyring";
import { cryptoWaitReady } from "@polkadot/util-crypto";
import { readFile } from "node:fs/promises";

const rpcUrl = process.env.MFC_REHEARSAL_RPC_URL || "ws://127.0.0.1:9970";
const treasuryFile = process.env.MFC_REHEARSAL_TREASURY;
const relayerFiles = (process.env.MFC_REHEARSAL_RELAYER_1 && process.env.MFC_REHEARSAL_RELAYER_2)
  ? [process.env.MFC_REHEARSAL_RELAYER_1, process.env.MFC_REHEARSAL_RELAYER_2]
  : [];
const governanceFiles = (process.env.MFC_REHEARSAL_GOVERNANCE_FILES || "").split(",").filter(Boolean);
if (!treasuryFile || relayerFiles.length !== 2 || governanceFiles.length !== 3) {
  throw new Error("Treasury, two relayers and three governance files are required");
}

await cryptoWaitReady();
const keyring = new Keyring({ ss58Format: 42, type: "sr25519" });
const document = JSON.parse(await readFile(treasuryFile, "utf8"));
const treasury = keyring.addFromUri(document.secretPhrase);
const api = await ApiPromise.create({ provider: new WsProvider(rpcUrl) });

async function transfer(destination, amount, label) {
  await new Promise((resolve, reject) => {
    let unsubscribe;
    api.tx.balances.transferKeepAlive(destination, amount).signAndSend(treasury, ({ status, dispatchError, events }) => {
      if (dispatchError) {
        reject(new Error(`${label}: ${dispatchError.toString()}`));
        return;
      }
      if (!status.isFinalized) return;
      if (!events.some(({ event }) => api.events.system.ExtrinsicSuccess.is(event))) {
        reject(new Error(`${label}: no ExtrinsicSuccess event`));
        return;
      }
      unsubscribe?.();
      console.log(`${label}: funded in ${status.asFinalized.toHex()}`);
      resolve();
    }).then((stop) => { unsubscribe = stop; }).catch(reject);
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
