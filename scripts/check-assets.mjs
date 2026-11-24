import assert from "node:assert/strict";
import { ApiPromise, WsProvider } from "@polkadot/api";
import { Keyring } from "@polkadot/keyring";
import { cryptoWaitReady } from "@polkadot/util-crypto";

await cryptoWaitReady();
const api = await ApiPromise.create({ provider: new WsProvider(process.env.DEVNET_WS_URL || "ws://127.0.0.1:9944") });
await api.isReady;

try {
  const keyring = new Keyring({ type: "sr25519" });
  const bob = keyring.addFromUri("//Bob");
  const alice = keyring.addFromUri("//Alice");
  const nextId = Number((await api.query.assets.nextAssetId()).toString());
  assert.equal(nextId, 1, "Run this check against a fresh devnet");

  const submit = (tx, label) => new Promise((resolve, reject) => {
    let unsubscribe;
    tx.signAndSend(bob, ({ status, dispatchError, events }) => {
      if (dispatchError) {
        const error = dispatchError.isModule ? api.registry.findMetaError(dispatchError.asModule) : null;
        reject(new Error(`${label}: ${error ? `${error.section}.${error.name}` : dispatchError.toString()}`));
        return;
      }
      if (!status.isFinalized) return;
      unsubscribe?.();
      resolve({ hash: status.asFinalized.toHex(), events: events.map(({ event }) => `${event.section}.${event.method}`) });
    }).then((stop) => { unsubscribe = stop; }).catch(reject);
  });

  const created = await submit(api.tx.assets.create(1, bob.address, 1), "create");
  assert.ok(created.events.includes("assets.Created"));
  await submit(api.tx.assets.setMetadata(1, "Test Community Token", "TCT", 6), "metadata");
  await submit(api.tx.assets.mint(1, bob.address, 1_000_000_000), "mint");
  await submit(api.tx.assets.transfer(1, alice.address, 100_000_000), "transfer");
  await submit(api.tx.assets.burn(1, bob.address, 100_000_000), "burn");
  await assert.rejects(submit(api.tx.assets.create(3, bob.address, 100), "wrong ID"), /assets.BadAssetId/);
  await submit(api.tx.assets.create(2, bob.address, 100), "next asset");

  const finalized = await api.rpc.chain.getFinalizedHead();
  const at = await api.at(finalized);
  const asset = (await at.query.assets.asset(1)).unwrap();
  const metadata = await at.query.assets.metadata(1);
  const bobBalance = (await at.query.assets.account(1, bob.address)).unwrap();
  const aliceBalance = (await at.query.assets.account(1, alice.address)).unwrap();
  assert.equal((await at.query.assets.nextAssetId()).toString(), "3");
  assert.equal(asset.supply.toString(), "900000000");
  assert.equal(bobBalance.balance.toString(), "800000000");
  assert.equal(aliceBalance.balance.toString(), "100000000");
  assert.equal(metadata.name.toUtf8(), "Test Community Token");
  assert.equal(metadata.symbol.toUtf8(), "TCT");
  assert.equal(metadata.decimals.toNumber(), 6);

  console.log(JSON.stringify({
    nextAssetId: "3",
    assetId: 1,
    name: metadata.name.toUtf8(),
    symbol: metadata.symbol.toUtf8(),
    supply: asset.supply.toString(),
    ownerBalance: bobBalance.balance.toString(),
    recipientBalance: aliceBalance.balance.toString(),
    finalizedHash: finalized.toHex()
  }, null, 2));
} finally {
  await api.disconnect();
}
