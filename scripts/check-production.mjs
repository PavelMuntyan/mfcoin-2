import assert from "node:assert/strict";
import { spawn, spawnSync } from "node:child_process";
import { createWriteStream } from "node:fs";
import fs from "node:fs/promises";
import path from "node:path";

const binary = path.resolve("target/release/solochain-template-node");
const example = path.resolve("target/release/examples/freeland_devnet");
const root = path.resolve(".devnet/production-candidate-v1");
const env = {
  ...process.env,
  MFCOIN_PRODUCTION_CONFIG: path.resolve("production.example.json"),
  MFCOIN_GRANDPA_JUSTIFICATION_PERIOD: "1"
};
const children = new Set();

function sleep(ms) {
  return new Promise(resolve => setTimeout(resolve, ms));
}

async function rpc(port, method, params = []) {
  const response = await fetch(`http://127.0.0.1:${port}`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ jsonrpc: "2.0", id: 1, method, params }),
    signal: AbortSignal.timeout(4000)
  });
  if (!response.ok) throw new Error(`${method}: HTTP ${response.status}`);
  const body = await response.json();
  if (body.error) throw new Error(`${method}: ${body.error.message}`);
  return body.result;
}

async function waitFor(description, check, timeout = 120_000) {
  const started = Date.now();
  let lastError;
  while (Date.now() - started < timeout) {
    try {
      const result = await check();
      if (result) return result;
    } catch (error) {
      lastError = error;
    }
    await sleep(300);
  }
  throw new Error(`Timed out: ${description}${lastError ? ` (${lastError.message})` : ""}`);
}

async function finalized(port) {
  const hash = await rpc(port, "chain_getFinalizedHead");
  const header = await rpc(port, "chain_getHeader", [hash]);
  return { hash, number: Number(header.number) };
}

async function startNode(name, rpcPort, p2pPort, bootnodes = []) {
  const basePath = path.join(root, name);
  await fs.mkdir(basePath, { recursive: true });
  const key = spawnSync(binary, ["key", "generate-node-key", "--base-path", basePath, "--chain", "production"], {
    env,
    encoding: "utf8"
  });
  if (key.status !== 0) throw new Error(`Could not generate ${name} network key: ${key.stderr}`);

  const payoutAccounts = {
    alice: "5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY",
    bob: "5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty"
  };
  const args = [
    "--chain", "production",
    "--base-path", basePath,
    "--rpc-port", String(rpcPort),
    "--port", String(p2pPort),
    "--no-mdns",
    "--no-prometheus",
    "--pow-payout-account", payoutAccounts[name],
    `--${name}`
  ];
  for (const bootnode of bootnodes) args.push("--bootnodes", bootnode);
  const log = createWriteStream(path.join(root, `${name}.log`), { flags: "a" });
  const child = spawn(binary, args, { env, stdio: ["ignore", "pipe", "pipe"] });
  child.stdout.pipe(log);
  child.stderr.pipe(log);
  const entry = { child, log };
  children.add(entry);
  child.once("exit", () => {
    children.delete(entry);
    log.end();
  });
  await waitFor(`${name} RPC`, () => rpc(rpcPort, "chain_getHeader"), 45_000);
}

async function stopNode(entry) {
  if (!children.has(entry)) return;
  const exited = new Promise(resolve => entry.child.once("exit", resolve));
  entry.child.kill("SIGINT");
  const graceful = await Promise.race([exited.then(() => true), sleep(20_000).then(() => false)]);
  if (!graceful) {
    entry.child.kill("SIGKILL");
    await exited;
  }
  children.delete(entry);
}

async function stopAll() {
  await Promise.all([...children].map(stopNode));
}

try {
  await fs.rm(root, { recursive: true, force: true });
  await fs.mkdir(root, { recursive: true });

  const exported = spawnSync(binary, ["export-chain-spec", "--chain", "production"], {
    env,
    encoding: "utf8",
    maxBuffer: 64 * 1024 * 1024
  });
  if (exported.status !== 0) throw new Error(`Could not export production genesis: ${exported.stderr}`);
  const spec = JSON.parse(exported.stdout);
  const genesis = spec.genesis.runtimeGenesis;
  const runtime = genesis.config ?? genesis.patch;
  assert.ok(runtime, "Missing production runtime genesis configuration");
  assert.equal(spec.chainType, "Live");
  assert.equal(runtime.sudo, undefined);
  assert.equal(runtime.aura, undefined);
  assert.equal(runtime.template, undefined);
  assert.equal(runtime.grandpa.authorities.length, 2);
  assert.ok(runtime.grandpa.authorities.every(([id]) => typeof id === "string"));
  assert.ok(runtime.freelandRecords.governanceAccount);

  await startNode("alice", 9960, 30440);
  await waitFor("Alice PoW blocks", async () => Number((await rpc(9960, "chain_getHeader")).number) >= 2);
  const peerId = await rpc(9960, "system_localPeerId");
  await startNode("bob", 9961, 30441, [`/ip4/127.0.0.1/tcp/30440/p2p/${peerId}`]);
  await waitFor("two-validator finality", async () => (await finalized(9960)).number >= 3);

  const governance = spawnSync(example, [], {
    env: { ...env, DEVNET_WS_URL: "ws://127.0.0.1:9960" },
    encoding: "utf8",
    timeout: 180_000,
    maxBuffer: 16 * 1024 * 1024
  });
  if (governance.status !== 0) throw new Error(`Governance example failed: ${governance.stdout}\n${governance.stderr}`);

  const { head, evidence } = await waitFor("two-validator GRANDPA evidence", async () => {
    const head = await finalized(9960);
    const evidence = await rpc(9960, "mfc_validatorEvidence", [head.hash]);
    return { head, evidence };
  });
  assert.equal(evidence.verified, true);
  assert.equal(evidence.precommitCount, 2);
  const info = await rpc(9960, "freeland_latestBlockInfo", [head.hash]);
  const record = await rpc(9960, "freeland_latestRecord", ["passport", `0x${"11".repeat(32)}`, head.hash]);
  assert.ok(record, "Synthetic passport record was not finalized");
  assert.ok(info?.infoRoot, "Missing Freeland info root");
  assert.equal(await rpc(9960, "system_chain"), "MFCoin 2.0");
  const chainType = await rpc(9960, "system_chainType");
  assert.equal(chainType.chainType || chainType, "Live");

  console.log(governance.stdout.trim());
  console.log(JSON.stringify({
    productionCandidate: true,
    isolatedLocalDevKeys: true,
    chain: "MFCoin 2.0",
    finalizedHeight: head.number,
    finalizedHash: head.hash,
    validatorEvidence: evidence,
    infoRoot: info.infoRoot,
    registryRoot: info.registryRoot,
    subjectCommitment: record.subjectCommitment
  }, null, 2));
} catch (error) {
  console.error(error);
  console.error(`Node logs: ${root}`);
  process.exitCode = 1;
} finally {
  await stopAll();
}
