import assert from "node:assert/strict";
import { spawn, spawnSync } from "node:child_process";
import { createWriteStream } from "node:fs";
import fs from "node:fs/promises";
import path from "node:path";

const binary = path.resolve("target/release/solochain-template-node");
const root = path.resolve(".devnet/multinode-v1");
const checkIndexer = process.env.CHECK_INDEXER === "1";
const indexerDirectory = path.resolve("indexer");
const syntheticSubject = `0x${"11".repeat(32)}`;
const children = new Set();
const results = {};

await fs.rm(root, { recursive: true, force: true });
await fs.mkdir(root, { recursive: true });

function sleep(ms) {
  return new Promise(resolve => setTimeout(resolve, ms));
}

async function rpc(port, method, params = []) {
  const response = await fetch(`http://127.0.0.1:${port}`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ jsonrpc: "2.0", id: 1, method, params }),
    signal: AbortSignal.timeout(3000)
  });
  if (!response.ok) throw new Error(`${method}: HTTP ${response.status}`);
  const payload = await response.json();
  if (payload.error) throw new Error(`${method}: ${payload.error.message}`);
  return payload.result;
}

async function waitFor(description, check, timeoutMs = 90_000) {
  const started = Date.now();
  let lastError;
  while (Date.now() - started < timeoutMs) {
    try {
      const value = await check();
      if (value) return value;
    } catch (error) {
      lastError = error;
    }
    await sleep(250);
  }
  throw new Error(`Timed out: ${description}${lastError ? ` (${lastError.message})` : ""}`);
}

function blockNumber(header) {
  return Number(header.number);
}

async function best(port) {
  return blockNumber(await rpc(port, "chain_getHeader"));
}

async function finalized(port) {
  const hash = await rpc(port, "chain_getFinalizedHead");
  const header = await rpc(port, "chain_getHeader", [hash]);
  return { hash, number: blockNumber(header) };
}

async function startNode({ scenario, name, rpcPort, p2pPort, mining = true, bootnodes = [] }) {
  const basePath = path.join(root, scenario, name);
  await fs.mkdir(basePath, { recursive: true });
  const keyPath = path.join(basePath, "chains/local_testnet/network/secret_ed25519");
  if (!await fs.access(keyPath).then(() => true, () => false)) {
    const key = spawnSync(binary, ["key", "generate-node-key", "--base-path", basePath, "--chain", "local"], { encoding: "utf8" });
    if (key.status !== 0) throw new Error(`Could not generate ${name} network key: ${key.stderr}`);
  }
  const args = [
    "--chain", "local",
    "--base-path", basePath,
    "--rpc-port", String(rpcPort),
    "--port", String(p2pPort),
    "--no-prometheus",
    "--no-mdns",
    `--${name}`
  ];
  if (!mining) args.push("--disable-pow-mining");
  for (const bootnode of bootnodes) args.push("--bootnodes", bootnode);
  const log = createWriteStream(path.join(root, scenario, `${name}.log`), { flags: "a" });
  const child = spawn(binary, args, { stdio: ["ignore", "pipe", "pipe"] });
  child.stdout.pipe(log);
  child.stderr.pipe(log);
  const entry = { child, scenario, name, log };
  children.add(entry);
  child.once("exit", () => children.delete(entry));
  await waitFor(`${name} RPC`, () => rpc(rpcPort, "chain_getHeader"), 45_000);
  return entry;
}

async function stopNode(entry) {
  if (!entry || !children.has(entry)) return;
  const exited = new Promise(resolve => entry.child.once("exit", resolve));
  entry.child.kill("SIGINT");
  const graceful = await Promise.race([
    exited.then(() => true),
    sleep(20_000).then(() => false)
  ]);
  if (!graceful) {
    entry.child.kill("SIGKILL");
    await exited;
  }
  entry.log.end();
  children.delete(entry);
}

async function stopAll() {
  await Promise.all([...children].map(stopNode));
  if (checkIndexer) {
    spawnSync("docker", ["compose", "stop"], { cwd: indexerDirectory, stdio: "ignore" });
  }
}

async function startIndexer() {
  const database = spawnSync("docker", ["compose", "up", "-d", "--wait"], { cwd: indexerDirectory, encoding: "utf8" });
  if (database.status !== 0) throw new Error(`Could not start indexer PostgreSQL: ${database.stderr}`);
  const log = createWriteStream(path.join(root, "indexer.log"));
  const child = spawn("npm", ["start"], {
    cwd: indexerDirectory,
    env: { ...process.env, CHAIN_RPC_URL: "http://127.0.0.1:9945", INDEXER_RESET: "1" },
    stdio: ["ignore", "pipe", "pipe"]
  });
  child.stdout.pipe(log);
  child.stderr.pipe(log);
  const entry = { child, scenario: "infra", name: "indexer", log };
  children.add(entry);
  child.once("exit", () => children.delete(entry));
}

async function submitRecordOnBob() {
  const example = spawnSync(path.resolve("target/release/examples/freeland_devnet"), [], {
    env: { ...process.env, DEVNET_WS_URL: "ws://127.0.0.1:9945", FREELAND_WAIT_FOR_FINALITY: "0" },
    encoding: "utf8",
    timeout: 120_000
  });
  if (example.status !== 0) throw new Error(`Could not submit Bob record: ${example.stdout}\n${example.stderr}`);
  await waitFor("Bob record included", async () => {
    const records = await fetch(`http://127.0.0.1:8787/freeland/records/${syntheticSubject}`).then(response => response.json());
    return records.some(record => record.status === "included");
  });
}

async function checkOrphanedRecord() {
  await waitFor("Bob record orphaned", async () => {
    const records = await fetch(`http://127.0.0.1:8787/freeland/records/${syntheticSubject}?status=orphaned`).then(response => response.json());
    return records.length > 0;
  });
  results.indexerOrphaned = true;
}

function bootnode(peerId, port = 30333) {
  return `/ip4/127.0.0.1/tcp/${port}/p2p/${peerId}`;
}

async function quorumScenario() {
  const scenario = "quorum";
  await fs.mkdir(path.join(root, scenario), { recursive: true });
  const alice = await startNode({ scenario, name: "alice", rpcPort: 9944, p2pPort: 30333 });
  await waitFor("Alice authors initial blocks", () => best(9944).then(number => number >= 2));
  const peerId = await rpc(9944, "system_localPeerId");
  const boot = bootnode(peerId);
  let bob = await startNode({ scenario, name: "bob", rpcPort: 9945, p2pPort: 30334, bootnodes: [boot] });
  let charlie = await startNode({ scenario, name: "charlie", rpcPort: 9946, p2pPort: 30335, mining: false, bootnodes: [boot] });
  let dave = await startNode({ scenario, name: "dave", rpcPort: 9947, p2pPort: 30336, mining: false, bootnodes: [boot] });

  await waitFor("four-validator finality", async () => {
    const head = await finalized(9944);
    return head.number >= 3;
  }, 120_000);
  results.quorumBeforeLoss = await finalized(9944);
  if (process.env.CHECK_EVIDENCE === "1") {
    const { head, evidence } = await waitFor("four-validator GRANDPA evidence", async () => {
      const head = await finalized(9944);
      const evidence = await rpc(9944, "mfc_validatorEvidence", [head.hash]);
      return { head, evidence };
    });
    results.quorumBeforeLoss = head;
    assert.equal(evidence.verified, true);
    assert.ok(evidence.precommitCount >= 3, `Expected at least 3 precommit votes, got ${evidence.precommitCount}`);
    results.validatorEvidence = evidence;
  }

  await Promise.all([stopNode(bob), stopNode(dave)]);
  await sleep(2000);
  const stalled = await finalized(9944);
  const bestAtLoss = await best(9944);
  await waitFor("PoW continues without finality", () => best(9944).then(number => number >= bestAtLoss + 5));
  await sleep(3000);
  const afterLoss = await finalized(9944);
  assert.equal(afterLoss.hash, stalled.hash, "Finality advanced after half the validators went offline");
  assert.ok(await best(9944) >= bestAtLoss + 5, "PoW did not continue without quorum");
  results.finalityStalledAt = afterLoss.number;

  bob = await startNode({ scenario, name: "bob", rpcPort: 9945, p2pPort: 30334, bootnodes: [boot] });
  dave = await startNode({ scenario, name: "dave", rpcPort: 9947, p2pPort: 30336, mining: false, bootnodes: [boot] });
  await waitFor("finality recovery", async () => {
    const head = await finalized(9944);
    return head.number > afterLoss.number;
  }, 120_000);
  results.finalityRecoveredAt = (await finalized(9944)).number;
  await stopAll();
}

async function reorgScenario() {
  const scenario = "reorg";
  await fs.mkdir(path.join(root, scenario), { recursive: true });
  if (checkIndexer) await startIndexer();
  await startNode({ scenario, name: "alice", rpcPort: 9944, p2pPort: 30333 });
  await waitFor("isolated Alice chain", () => best(9944).then(number => number >= 4));
  const peerId = await rpc(9944, "system_localPeerId");
  let bob = await startNode({ scenario, name: "bob", rpcPort: 9945, p2pPort: 30334 });
  await waitFor("isolated Bob chain", () => best(9945).then(number => number >= 4));
  if (checkIndexer) {
    await waitFor("indexer API", async () => {
      const response = await fetch("http://127.0.0.1:8787/health");
      return response.ok;
    }, 120_000);
  }

  const aliceForkHash = await rpc(9944, "chain_getBlockHash", [2]);
  const bobForkHash = await rpc(9945, "chain_getBlockHash", [2]);
  assert.notEqual(aliceForkHash, bobForkHash, "Isolated nodes unexpectedly produced the same chain");
  if (checkIndexer) await submitRecordOnBob();
  const orphanBest = await best(9945);
  await stopNode(bob);
  const targetBest = orphanBest + 5;
  await waitFor("longer Alice chain", () => best(9944).then(number => number >= targetBest));
  const canonicalHash = await rpc(9944, "chain_getBlockHash", [2]);

  bob = await startNode({ scenario, name: "bob", rpcPort: 9945, p2pPort: 30334, mining: false, bootnodes: [bootnode(peerId)] });
  await waitFor("Bob adopts longer chain", async () => {
    return await rpc(9945, "chain_getBlockHash", [2]) === canonicalHash;
  }, 120_000);
  const adoptedHash = await rpc(9945, "chain_getBlockHash", [2]);
  assert.equal(adoptedHash, canonicalHash, "Bob did not adopt Alice's canonical hash");
  assert.notEqual(adoptedHash, bobForkHash, "Bob kept the orphaned fork");
  assert.equal((await finalized(9945)).number, 0, "Fork should not finalize without a validator quorum");
  if (checkIndexer) await checkOrphanedRecord();
  results.reorgDetails = { orphanedHash: bobForkHash, adoptedHash, height: await best(9945) };
  await stopAll();
}

try {
  await quorumScenario();
  await reorgScenario();
  console.log(JSON.stringify({ ...results, multiNode: true, quorumLoss: true, quorumRecovery: true, reorg: true }, null, 2));
} catch (error) {
  console.error(error);
  console.error(`Node logs: ${root}`);
  await stopAll();
  process.exitCode = 1;
}
