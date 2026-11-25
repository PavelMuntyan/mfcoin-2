import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { ApiPromise, WsProvider } from "@polkadot/api";
import { Keyring } from "@polkadot/keyring";
import { blake2AsHex, createKeyMulti, cryptoWaitReady, encodeAddress } from "@polkadot/util-crypto";
import { u8aToHex } from "@polkadot/util";

const rpcUrl = process.env.MFCOIN_RPC_URL || "ws://127.0.0.1:9950";
const expectedGenesis = process.env.MFCOIN_GENESIS_HASH;
const wasmPath = process.env.MFCOIN_RUNTIME_WASM;
const governanceFiles = (process.env.MFCOIN_GOVERNANCE_FILES || "").split(",").filter(Boolean);
const governanceThreshold = Number(process.env.MFCOIN_GOVERNANCE_THRESHOLD || 2);
const executeDelay = Number(process.env.MFCOIN_UPGRADE_DELAY || 28_800);
if (!expectedGenesis || !wasmPath || governanceFiles.length < 2) {
  throw new Error("MFCOIN_GENESIS_HASH, MFCOIN_RUNTIME_WASM and at least two MFCOIN_GOVERNANCE_FILES are required");
}
if (!Number.isInteger(governanceThreshold) || governanceThreshold < 2 || governanceThreshold > governanceFiles.length || governanceThreshold * 3 < governanceFiles.length * 2) {
  throw new Error("MFCOIN_GOVERNANCE_THRESHOLD must be between two thirds and all governance signers");
}
if (!Number.isInteger(executeDelay) || executeDelay < 28_800 || executeDelay > 100_800) throw new Error("Upgrade delay must be between 28800 and 100800 blocks");

await cryptoWaitReady();
const api = await ApiPromise.create({ provider: new WsProvider(rpcUrl, 2_500, {}, 10_000) });
await api.isReady;
try {
  assert.equal(api.genesisHash.toHex(), expectedGenesis, "RPC genesis mismatch");
  const keyring = new Keyring({ ss58Format: 42, type: "sr25519" });
  const signers = [];
  for (const file of governanceFiles) {
    const document = JSON.parse(await readFile(file, "utf8"));
    const pair = keyring.addFromUri(document.secretPhrase);
    signers.push(pair);
  }
  signers.sort((a, b) => Buffer.from(a.publicKey).compare(Buffer.from(b.publicKey)));
  const publicKeys = signers.map((pair) => pair.publicKey);
  const governanceAddress = encodeAddress(createKeyMulti(publicKeys, governanceThreshold), 42);
  const storedGovernance = await api.query.mfcGovernance.governanceAccount();
  assert.ok(storedGovernance.isSome, "Governance is disabled");
  assert.equal(encodeAddress(storedGovernance.unwrap().toU8a(), 42), governanceAddress, "Governance keys do not match the on-chain multisig account");
  const runtime = await api.rpc.state.getRuntimeVersion();
  console.log(`Connected to runtime ${runtime.specVersion.toString()} at ${api.genesisHash.toHex()}; governance ${governanceThreshold}-of-${signers.length} verified`);

  const submit = (extrinsic, pair, label) => new Promise((resolve, reject) => {
    let unsubscribe;
    extrinsic.signAndSend(pair, ({ status, dispatchError, events }) => {
      if (dispatchError) {
        const error = dispatchError.isModule ? api.registry.findMetaError(dispatchError.asModule) : null;
        reject(new Error(`${label}: ${error ? `${error.section}.${error.name}` : dispatchError.toString()}`));
        return;
      }
      if (!status.isFinalized) return;
      if (!events.some(({ event }) => api.events.system.ExtrinsicSuccess.is(event))) {
        reject(new Error(`${label}: no ExtrinsicSuccess event`));
        return;
      }
      unsubscribe?.();
      resolve({ blockHash: status.asFinalized.toHex(), events, hash: extrinsic.hash.toHex() });
    }).then((stop) => { unsubscribe = stop; }).catch(reject);
  });

  if (process.env.MFCOIN_EXECUTE_PROPOSAL_ID) {
    const proposalId = process.env.MFCOIN_EXECUTE_PROPOSAL_ID;
    const proposal = await api.query.mfcGovernance.proposals(proposalId);
    assert.ok(proposal.isSome, "Upgrade proposal was not found");
    assert.ok(proposal.unwrap().isSetCode, "Governance proposal is not a runtime upgrade");
    const scheduled = proposal.unwrap().asSetCode;
    const wasm = await readFile(wasmPath);
    assert.equal(blake2AsHex(wasm), blake2AsHex(scheduled.code), "Runtime artifact does not match the scheduled proposal");
    const executeAt = scheduled.executeAt.toNumber();
    while (true) {
      const finalizedHash = await api.rpc.chain.getFinalizedHead();
      const finalized = await api.rpc.chain.getHeader(finalizedHash);
      if (finalized.number.toNumber() >= executeAt) break;
      await new Promise((resolve) => setTimeout(resolve, 3_000));
    }
    await submit(api.tx.mfcGovernance.execute(proposalId), signers[0], "runtime execute");
    console.log("Governance execute finalized");
  } else {
  const wasm = await readFile(wasmPath);
  assert.ok(wasm.length > 100_000 && wasm.length <= 3_000_000, "Unexpected runtime WASM size");
  const code = u8aToHex(wasm);
  const best = await api.rpc.chain.getHeader();
  const executeAt = best.number.toNumber() + executeDelay;
  const proposalCall = api.tx.mfcGovernance.setCode(code, executeAt);
  const proposalId = blake2AsHex(proposalCall.method.toU8a().slice(1));
  console.log(`Runtime WASM bytes: ${wasm.length}; proposal ${proposalId}; execute at block ${executeAt}`);
  if (process.env.MFCOIN_UPGRADE_DRY_RUN === "1") {
    console.log("Dry run complete; no transaction was signed or submitted");
    await api.disconnect();
    process.exit(0);
  }

  const otherSignatories = (pair) => publicKeys.filter((key) => Buffer.from(key).compare(Buffer.from(pair.publicKey)) !== 0);
  const maxWeight = { refTime: 100_000_000_000, proofSize: 3_100_000 };

  const first = await submit(api.tx.multisig.asMulti(governanceThreshold, otherSignatories(signers[0]), null, proposalCall, maxWeight), signers[0], "first approval");
  const block = await api.rpc.chain.getBlock(first.blockHash);
  const extrinsicIndex = block.block.extrinsics.findIndex((extrinsic) => extrinsic.hash.toHex() === first.hash);
  assert.ok(extrinsicIndex >= 0, "First approval is missing from its finalized block");
  const header = await api.rpc.chain.getHeader(first.blockHash);
  const timepoint = { height: header.number.toNumber(), index: extrinsicIndex };
  console.log(`First approval finalized at ${timepoint.height}/${timepoint.index}`);

  let lastApproval;
  for (let index = 1; index < governanceThreshold; index += 1) {
    lastApproval = await submit(api.tx.multisig.asMulti(governanceThreshold, otherSignatories(signers[index]), timepoint, proposalCall, maxWeight), signers[index], `approval ${index + 1}`);
  }
  const executed = lastApproval.events.find(({ event }) => api.events.multisig.MultisigExecuted.is(event));
  assert.ok(executed, "MultisigExecuted event is missing");
  const result = executed.event.data[4];
  if (result.isErr) throw new Error(`Multisig execution failed: ${result.asErr.toString()}`);
  const proposal = await api.query.mfcGovernance.proposals(proposalId);
  assert.ok(proposal.isSome, "Upgrade proposal was not stored");
  console.log("Required approvals executed; proposal stored with the required timelock");
  if (process.env.MFCOIN_UPGRADE_SCHEDULE_ONLY === "1") {
    console.log(`Proposal ${proposalId} is scheduled for block ${executeAt}; no runtime execution was requested`);
    await api.disconnect();
    process.exit(0);
  }

  while (true) {
    const finalizedHash = await api.rpc.chain.getFinalizedHead();
    const finalized = await api.rpc.chain.getHeader(finalizedHash);
    if (finalized.number.toNumber() >= executeAt) break;
    await new Promise((resolve) => setTimeout(resolve, 3_000));
  }
  await submit(api.tx.mfcGovernance.execute(proposalId), signers[0], "runtime execute");
  console.log("Governance execute finalized");
  }
} finally {
  await api.disconnect();
}

const upgraded = await ApiPromise.create({ provider: new WsProvider(rpcUrl, 2_500, {}, 10_000) });
await upgraded.isReady;
try {
  assert.equal(upgraded.genesisHash.toHex(), expectedGenesis);
  const runtime = await upgraded.rpc.state.getRuntimeVersion();
  assert.equal(runtime.specVersion.toNumber(), Number(process.env.MFCOIN_TARGET_SPEC_VERSION || 117));
  const nextAssetId = await upgraded.query.assets.nextAssetId();
  assert.ok(nextAssetId.isSome);
  assert.equal(nextAssetId.unwrap().toNumber(), 1);
  const finalizedHash = await upgraded.rpc.chain.getFinalizedHead();
  const finalized = await upgraded.rpc.chain.getHeader(finalizedHash);
  console.log(JSON.stringify({ specVersion: runtime.specVersion.toNumber(), nextAssetId: nextAssetId.unwrap().toNumber(), finalizedNumber: finalized.number.toNumber(), finalizedHash: finalizedHash.toHex() }, null, 2));
} finally {
  await upgraded.disconnect();
}
