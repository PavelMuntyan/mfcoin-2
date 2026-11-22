import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import fs from "node:fs/promises";
import path from "node:path";

const root = path.resolve(".devnet/grandpa-evidence-v1");
const binary = path.resolve("target/release/solochain-template-node");
const children = [];
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));

async function rpc(method, params = []) {
  const response = await fetch("http://127.0.0.1:9956", {
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
    await sleep(400);
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

try {
  await fs.rm(root, { recursive: true, force: true });
  await fs.mkdir(root, { recursive: true });
  const child = spawn(binary, ["--dev", "--base-path", root, "--rpc-port", "9956", "--no-mdns", "--no-prometheus"], {
    env: { ...process.env, MFCOIN_GRANDPA_JUSTIFICATION_PERIOD: "1" },
    stdio: ["ignore", "ignore", "pipe"]
  });
  children.push(child);

  const finalizedHash = await waitFor("finalized block", async () => {
    const hash = await rpc("chain_getFinalizedHead");
    const header = await rpc("chain_getHeader", [hash]);
    return Number(header.number) >= 4 ? hash : null;
  });
  const evidence = await rpc("mfc_validatorEvidence", [finalizedHash]);
  console.log(JSON.stringify({ evidence }, null, 2));
  assert.equal(evidence.verified, true);
  assert.equal(evidence.blockHash, finalizedHash);
  assert.ok(evidence.precommitCount >= 1);
  assert.ok(evidence.precommitAuthorities.includes("5FA9nQDVg267DEd8m1ZypXLBnvN7SFxYwV7ndqSYGiN9TTpu"));
  assert.equal(evidence.precommitAuthorities.length, evidence.precommitCount);

  const genesisHash = await rpc("chain_getBlockHash", [0]);
  await assert.rejects(() => rpc("mfc_validatorEvidence", [genesisHash]), /justification|decode|target/i);

  console.log(JSON.stringify({ liveGrandpaEvidence: true, evidence, missingJustificationRejected: true }, null, 2));
} catch (error) {
  console.error(error);
  process.exitCode = 1;
} finally {
  await stopAll();
}
