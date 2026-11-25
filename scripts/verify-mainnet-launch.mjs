import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { ApiPromise, WsProvider } from "@polkadot/api";
import { Keyring } from "@polkadot/keyring";
import { cryptoWaitReady } from "@polkadot/util-crypto";

const expectedGenesis = process.env.MFC_MAINNET_EXPECTED_GENESIS_HASH;
const config = JSON.parse(await readFile(process.env.MFC_MAINNET_CONFIG || new URL("../production.mainnet.json", import.meta.url), "utf8"));
const endpoints = (process.env.MFC_MAINNET_ENDPOINTS || "").split(",").filter(Boolean).map((entry) => {
  const separator = entry.indexOf("=");
  assert.ok(separator > 0, `Invalid endpoint: ${entry}`);
  return { label: entry.slice(0, separator), url: entry.slice(separator + 1) };
});
const required = ["validator-1", "validator-2", "validator-3", "validator-4", "archive-primary", "archive-failover"];
assert.ok(expectedGenesis, "MFC_MAINNET_EXPECTED_GENESIS_HASH is required");
assert.deepEqual(endpoints.map(({ label }) => label).sort(), [...required].sort());

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
async function connect(endpoint) {
  const provider = new WsProvider(endpoint.url, 2_500);
  const api = await ApiPromise.create({ provider, noInitWarn: true });
  assert.equal(api.genesisHash.toHex(), expectedGenesis, `${endpoint.label}: wrong genesis`);
  assert.equal((await api.rpc.system.chain()).toString(), "MFCoin 2.0 Mainnet", `${endpoint.label}: wrong chain`);
  assert.equal(api.runtimeVersion.specVersion.toNumber(), 117, `${endpoint.label}: wrong runtime`);
  const health = await api.rpc.system.health();
  assert.ok(health.peers.toNumber() >= 1, `${endpoint.label}: no peers`);
  assert.equal(health.isSyncing.isTrue, false, `${endpoint.label}: still syncing`);
  const finalizedHash = await api.rpc.chain.getFinalizedHead();
  const finalized = await api.rpc.chain.getHeader(finalizedHash);
  console.log(JSON.stringify({ endpoint: endpoint.label, peers: health.peers.toNumber(), finalized: finalized.number.toNumber() }));
  return { ...endpoint, api, provider, finalized: finalized.number.toNumber() };
}

const connections = [];
try {
  for (const endpoint of endpoints) connections.push(await connect(endpoint));
  const primary = connections.find(({ label }) => label === "validator-1");
  const api = primary.api;
  const start = primary.finalized;
  const deadline = Date.now() + 120_000;
  let finalizedHash;
  let finalizedNumber = start;
  while (Date.now() < deadline && finalizedNumber < start + 3) {
    await sleep(2_000);
    finalizedHash = await api.rpc.chain.getFinalizedHead();
    finalizedNumber = (await api.rpc.chain.getHeader(finalizedHash)).number.toNumber();
  }
  assert.ok(finalizedNumber >= start + 3, `Finality stalled at ${finalizedNumber}`);
  finalizedHash = await api.rpc.chain.getFinalizedHead();

  const authorities = await api.query.grandpa.authorities();
  assert.equal(authorities.length, 4);
  const evidence = await primary.provider.send("mfc_validatorEvidence", [finalizedHash.toHex()]);
  assert.equal(evidence.verified, true);
  assert.ok(evidence.precommitCount >= 3, `Only ${evidence.precommitCount} GRANDPA precommits`);

  assert.equal((await api.query.mfcRewards.enabled()).isTrue, true);
  assert.equal((await api.query.mfcRewards.activationBlock()).toNumber(), config.rewards.activationBlock);
  assert.equal((await api.query.mfcRewards.treasuryAccount()).unwrap().toString(), config.rewards.treasuryAccount);
  const issuanceDeadline = Date.now() + 300_000;
  let issuance = await api.query.balances.totalIssuance();
  let minted = await api.query.mfcRewards.mintedTotal();
  while (Date.now() < issuanceDeadline && minted.isZero()) {
    await sleep(3_000);
    issuance = await api.query.balances.totalIssuance();
    minted = await api.query.mfcRewards.mintedTotal();
  }
  assert.ok(minted.toBigInt() > 0n, "No Main Net reward has been minted");
  assert.equal(issuance.toBigInt(), 1_000_000_000_000n + minted.toBigInt(), "Unexplained issuance");

  const reference = (await api.rpc.chain.getHeader(finalizedHash)).number.toNumber();
  finalizedNumber = reference;
  for (const archive of connections.filter(({ label }) => label.startsWith("archive-"))) {
    const archiveHash = await archive.api.rpc.chain.getFinalizedHead();
    const archiveFinalized = (await archive.api.rpc.chain.getHeader(archiveHash)).number.toNumber();
    const archiveHealth = await archive.api.rpc.system.health();
    assert.equal(archiveHealth.isSyncing.isTrue, false, `${archive.label}: archive resumed syncing`);
    assert.ok(archiveFinalized + 8 >= reference, `${archive.label}: archive is behind ${reference} at ${archiveFinalized}`);
  }

  if (process.env.MFC_MAINNET_PROBE_KEY) {
    await cryptoWaitReady();
    const document = JSON.parse(await readFile(process.env.MFC_MAINNET_PROBE_KEY, "utf8"));
    const keyring = new Keyring({ ss58Format: 42, type: "sr25519" });
    const sender = keyring.addFromUri(document.secretPhrase);
    const hash = await new Promise((resolve, reject) => {
      let unsubscribe;
      let settled = false;
      const finish = (error, value) => {
        if (settled) return;
        settled = true;
        unsubscribe?.();
        if (error) reject(error);
        else resolve(value);
      };
      api.tx.balances.transferKeepAlive(sender.address, 1_000_000).signAndSend(sender, ({ status, dispatchError, events, txHash }) => {
        if (dispatchError) {
          finish(new Error(dispatchError.toString()));
          return;
        }
        if (!status.isFinalized) return;
        if (!events.some(({ event }) => api.events.system.ExtrinsicSuccess.is(event))) {
          finish(new Error("Broadcast probe has no ExtrinsicSuccess event"));
          return;
        }
        finish(null, txHash.toHex());
      }).then((stop) => { unsubscribe = stop; }).catch(finish);
    });
    console.log(JSON.stringify({ authorSubmitExtrinsic: "finalized", hash }));
  }

  console.log(JSON.stringify({
    genesis: expectedGenesis,
    specVersion: 117,
    finalized: finalizedNumber,
    validatorEvidence: evidence,
    totalIssuance: issuance.toString(),
    minted: minted.toString(),
    archivesSynchronized: true
  }, null, 2));
} finally {
  await Promise.all(connections.map(({ api }) => api.disconnect()));
}
