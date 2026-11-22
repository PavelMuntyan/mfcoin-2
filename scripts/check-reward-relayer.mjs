import assert from "node:assert/strict";
import { spawn, spawnSync } from "node:child_process";
import { createWriteStream } from "node:fs";
import fs from "node:fs/promises";
import path from "node:path";

const root = path.resolve(".devnet/reward-relayer-v1");
const binary = path.resolve("target/release/solochain-template-node");
const relayer = path.resolve("target/release/examples/reward_relayer");
const example = path.resolve("target/release/examples/validator_rewards");
const registerPayout = path.resolve("target/release/examples/register_validator_payout");
const queuePath = path.join(root, "queue.json");
const children = [];

async function rpc(method, params = []) {
  const response = await fetch("http://127.0.0.1:9958", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ jsonrpc: "2.0", id: 1, method, params }),
    signal: AbortSignal.timeout(4000)
  });
  const body = await response.json();
  if (body.error) throw new Error(body.error.message);
  return body.result;
}

async function waitFor(description, check, timeout = 90_000) {
  const started = Date.now();
  let lastError;
  while (Date.now() - started < timeout) {
    try {
      const result = await check();
      if (result) return result;
    } catch (error) {
      lastError = error;
    }
    await new Promise(resolve => setTimeout(resolve, 400));
  }
  throw new Error(`Timed out: ${description}${lastError ? ` (${lastError.message})` : ""}`);
}

async function stopAll() {
  await Promise.all(children.map(child => new Promise(resolve => {
    if (child.exitCode !== null) return resolve();
    child.once("exit", resolve);
    child.kill("SIGINT");
    setTimeout(() => child.kill("SIGKILL"), 20_000).unref();
  })));
}

function run(binary, env) {
  const result = spawnSync(binary, [], {
    env: { ...process.env, DEVNET_WS_URL: "ws://127.0.0.1:9958", ...env },
    encoding: "utf8",
    timeout: 180_000,
    maxBuffer: 16 * 1024 * 1024
  });
  if (result.status !== 0) throw new Error(`Relayer example failed: ${result.stdout}\n${result.stderr}`);
  return result.stdout;
}

try {
  await fs.rm(root, { recursive: true, force: true });
  await fs.mkdir(root, { recursive: true });
  const log = createWriteStream(path.join(root, "node.log"));
  const child = spawn(binary, ["--dev", "--base-path", root, "--rpc-port", "9958", "--no-mdns", "--no-prometheus"], {
    env: { ...process.env, MFCOIN_GRANDPA_JUSTIFICATION_PERIOD: "1" },
    stdio: ["ignore", "pipe", "pipe"]
  });
  child.stdout.pipe(log);
  child.stderr.pipe(log);
  children.push(child);

  await waitFor("mature finalized reward obligation", async () => {
    const hash = await rpc("chain_getFinalizedHead");
    const header = await rpc("chain_getHeader", [hash]);
    return Number(header.number) >= 21;
  });
  const relayerEnv = {
    MFC_RELAYER_ONCE: "1",
    MFC_RELAYER_START_TARGET: "11",
    MFC_RELAYER_QUEUE: queuePath
  };
  const first = run(relayer, relayerEnv);
  assert.match(first, /validator_relay_finalized=true target=11/);
  let queue = JSON.parse(await fs.readFile(queuePath, "utf8"));
  assert.equal(queue.nextTarget ?? queue.next_target, 12);
  assert.equal(queue.pending.length, 0);

  const claim = run(example, {});
  assert.match(claim, /validator_credit_claimed=true/);
  assert.match(claim, /duplicate_release_rejected=true/);
  const registration = run(registerPayout, {});
  assert.match(registration, /validator_payout_registered=true/);
  const second = run(relayer, relayerEnv);
  assert.doesNotMatch(second, /Release failed/);
  queue = JSON.parse(await fs.readFile(queuePath, "utf8"));
  assert.equal(queue.pending.length, 0);
  const totals = await rpc("mfc_rewardTotals", [null]);
  assert.ok(BigInt(totals.validatorPaid) > 0n);
  console.log(first.trim());
  console.log(claim.trim());
  console.log(JSON.stringify({ durableRelayerQueue: true, restartSafe: true, totals }, null, 2));
} catch (error) {
  console.error(error);
  console.error(`Node logs: ${root}`);
  process.exitCode = 1;
} finally {
  await stopAll();
}
