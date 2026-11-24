import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { ApiPromise, WsProvider } from "@polkadot/api";
import { Keyring } from "@polkadot/keyring";
import { cryptoWaitReady } from "@polkadot/util-crypto";

const rpcUrl = process.env.MFCOIN_RPC_URL || "ws://127.0.0.1:9960";
const expectedGenesis = process.env.MFCOIN_GENESIS_HASH;
const files = (process.env.MFCOIN_GOVERNANCE_FILES || "").split(",").filter(Boolean);
if (!expectedGenesis || files.length < 2) throw new Error("MFCOIN_GENESIS_HASH and two MFCOIN_GOVERNANCE_FILES are required");
await cryptoWaitReady();
const api = await ApiPromise.create({ provider: new WsProvider(rpcUrl, 2_500, {}, 10_000) });
await api.isReady;

try {
  assert.equal(api.genesisHash.toHex(), expectedGenesis, "RPC genesis mismatch");
  assert.equal((await api.rpc.state.getRuntimeVersion()).specVersion.toNumber(), Number(process.env.MFCOIN_TARGET_SPEC_VERSION || 115));
  const keyring = new Keyring({ ss58Format: 42, type: "sr25519" });
  const owner = keyring.addFromUri(JSON.parse(await readFile(files[0], "utf8")).secretPhrase);
  const recipient = keyring.addFromUri(JSON.parse(await readFile(files[1], "utf8")).secretPhrase);
  assert.equal((await api.query.assets.nextAssetId()).toString(), "1", "Smoke test requires a fresh asset registry");

  const submit = (tx, label) => new Promise((resolve, reject) => {
    let unsubscribe;
    tx.signAndSend(owner, ({ status, dispatchError }) => {
      if (dispatchError) {
        const error = dispatchError.isModule ? api.registry.findMetaError(dispatchError.asModule) : null;
        reject(new Error(`${label}: ${error ? `${error.section}.${error.name}` : dispatchError.toString()}`));
        return;
      }
      if (!status.isFinalized) return;
      unsubscribe?.();
      resolve(status.asFinalized.toHex());
    }).then((stop) => { unsubscribe = stop; }).catch(reject);
  });

  await submit(api.tx.assets.create(1, owner.address, 1), "create");
  await submit(api.tx.assets.setMetadata(1, "Community Test Token", "CTT", 6), "metadata");
  await submit(api.tx.assets.mint(1, owner.address, 1_000_000_000), "mint");
  await submit(api.tx.assets.transfer(1, recipient.address, 100_000_000), "transfer");
  await submit(api.tx.assets.burn(1, owner.address, 50_000_000), "burn");

  const finalizedHash = await api.rpc.chain.getFinalizedHead();
  const at = await api.at(finalizedHash);
  const details = (await at.query.assets.asset(1)).unwrap();
  const metadata = await at.query.assets.metadata(1);
  const ownerBalance = (await at.query.assets.account(1, owner.address)).unwrap();
  const recipientBalance = (await at.query.assets.account(1, recipient.address)).unwrap();
  assert.equal(details.supply.toString(), "950000000");
  assert.equal(ownerBalance.balance.toString(), "850000000");
  assert.equal(recipientBalance.balance.toString(), "100000000");
  assert.equal(metadata.name.toUtf8(), "Community Test Token");
  assert.equal(metadata.symbol.toUtf8(), "CTT");
  assert.equal(metadata.decimals.toNumber(), 6);
  assert.equal((await at.query.assets.nextAssetId()).toString(), "2");
  console.log(JSON.stringify({ assetId: 1, name: metadata.name.toUtf8(), symbol: metadata.symbol.toUtf8(), supply: details.supply.toString(), ownerBalance: ownerBalance.balance.toString(), recipientBalance: recipientBalance.balance.toString(), nextAssetId: "2", finalizedHash: finalizedHash.toHex() }, null, 2));
} finally {
  await api.disconnect();
}
