import assert from "node:assert/strict";
import { spawn, spawnSync } from "node:child_process";
import { createWriteStream } from "node:fs";
import fs from "node:fs/promises";
import path from "node:path";

const binary = path.resolve("target/release/solochain-template-node");
const root = path.resolve(".devnet/production-rewards-v1");
const raw = path.join(root, "chain-spec-raw.json");
const children = new Set();
const delayFinality = process.env.MFC_TEST_DELAY_FINALITY === "1";
const env = {
  ...process.env,
  MFCOIN_PRODUCTION_CONFIG: path.resolve("production-rewards.example.json"),
  MFCOIN_GRANDPA_JUSTIFICATION_PERIOD: "1"
};

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
  const body = await response.json();
  if (body.error) throw new Error(body.error.message);
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

async function startNode(name, rpcPort, p2pPort, bootnodes = [], disableGrandpa = false) {
  const basePath = path.join(root, name);
  await fs.mkdir(basePath, { recursive: true });
  const key = spawnSync(binary, ["key", "generate-node-key", "--base-path", basePath, "--chain", raw], {
    env,
    encoding: "utf8"
  });
  if (key.status !== 0) throw new Error(`Could not generate ${name} network key: ${key.stderr}`);
  const payoutAccounts = {
    alice: "5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY",
    bob: "5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty"
  };
  const args = [
    "--chain", raw,
    "--base-path", basePath,
    "--rpc-port", String(rpcPort),
    "--port", String(p2pPort),
    "--no-mdns",
    "--no-prometheus",
    "--pow-payout-account", payoutAccounts[name],
    `--${name}`
  ];
  if (disableGrandpa) args.push("--disable-grandpa");
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
  return entry;
}

async function stopNode(entry) {
  if (!children.has(entry)) return;
  await new Promise(resolve => {
    entry.child.once("exit", resolve);
    entry.child.kill("SIGINT");
    setTimeout(() => entry.child.kill("SIGKILL"), 20_000).unref();
  });
}

async function stopAll() {
  await Promise.all([...children].map(stopNode));
}

function runExample(binary, environment = {}) {
  const result = spawnSync(binary, [], {
    env: { ...process.env, DEVNET_WS_URL: "ws://127.0.0.1:9960", ...environment },
    encoding: "utf8",
    timeout: 180_000,
    maxBuffer: 16 * 1024 * 1024
  });
  if (result.status !== 0) throw new Error(`Example failed: ${result.stdout}\n${result.stderr}`);
  return result.stdout;
}

try {
  await fs.rm(root, { recursive: true, force: true });
  await fs.mkdir(root, { recursive: true });
  const exported = spawnSync(binary, ["export-chain-spec", "--raw", "--chain", "production"], {
    env,
    encoding: "utf8",
    maxBuffer: 64 * 1024 * 1024
  });
  if (exported.status !== 0) throw new Error(`Could not export reward testnet: ${exported.stderr}`);
  await fs.writeFile(raw, exported.stdout);
  const runtimeExport = spawnSync(binary, ["export-chain-spec", "--chain", "production"], {
    env,
    encoding: "utf8",
    maxBuffer: 64 * 1024 * 1024
  });
  if (runtimeExport.status !== 0) throw new Error(`Could not inspect reward testnet: ${runtimeExport.stderr}`);
  const spec = JSON.parse(runtimeExport.stdout);
  const genesis = spec.genesis.runtimeGenesis;
  const runtime = genesis.config ?? genesis.patch;
  assert.equal(spec.id, "mfcoin-rewards-test");
  assert.equal(runtime.mfcRewards.enabled, true);
  assert.equal(runtime.mfcRewards.activationBlock, 12);

  let alice = await startNode("alice", 9960, 30440, [], delayFinality);
  await waitFor("Alice PoW blocks", async () => Number((await rpc(9960, "chain_getHeader")).number) >= (delayFinality ? 24 : 2));
  if (delayFinality) {
    const genesisHash = await rpc(9960, "chain_getBlockHash", [0]);
    assert.equal(await rpc(9960, "chain_getFinalizedHead"), genesisHash);
    await stopNode(alice);
    alice = await startNode("alice", 9960, 30440);
    assert.ok(alice);
  }
  const peerId = await rpc(9960, "system_localPeerId");
  await startNode("bob", 9961, 30441, [`/ip4/127.0.0.1/tcp/30440/p2p/${peerId}`]);
  await waitFor("mature two-validator finality", async () => {
    const hash = await rpc(9960, "chain_getFinalizedHead");
    const header = await rpc(9960, "chain_getHeader", [hash]);
    return Number(header.number) >= 22;
  });

  const relayer = runExample(path.resolve("target/release/examples/reward_relayer"), {
    MFC_RELAYER_ONCE: "1",
    MFC_RELAYER_START_TARGET: "12",
    MFC_RELAYER_QUEUE: path.join(root, "queue.json")
  });
  assert.match(relayer, /validator_relay_finalized=true/);
  if (delayFinality) assert.match(relayer, /validator_relay_unfinalized_skipped=true/);
  const claim = runExample(path.resolve("target/release/examples/validator_rewards"));
  assert.match(claim, /validator_credit_claimed=true/);
  assert.match(claim, /duplicate_release_rejected=true/);
  const totals = await rpc(9960, "mfc_rewardTotals", [null]);
  assert.ok(BigInt(totals.validatorPaid) > 0n);
  assert.equal(
    BigInt(totals.minted),
    BigInt(totals.powPaid) + BigInt(totals.treasuryPaid) + BigInt(totals.validatorPaid)
  );
  console.log(relayer.trim());
  console.log(claim.trim());
  console.log(JSON.stringify({ productionRewardTestnet: true, totals }, null, 2));
} catch (error) {
  console.error(error);
  console.error(`Node logs: ${root}`);
  process.exitCode = 1;
} finally {
  await stopAll();
}
