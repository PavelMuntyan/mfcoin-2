import fs from "node:fs";

const [queuePath, rpcPort = "9951"] = process.argv.slice(2);
if (!queuePath) throw new Error("Usage: prune-relayer-queue.mjs <queue> [rpcPort]");
if (!fs.existsSync(queuePath)) {
  console.log(JSON.stringify({ queue: queuePath, expired: 0, missing: true }));
  process.exit(0);
}

async function reward(target) {
  const response = await fetch(`http://127.0.0.1:${rpcPort}`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ jsonrpc: "2.0", id: 1, method: "mfc_validatorReward", params: [target, null] }),
    signal: AbortSignal.timeout(5000)
  });
  const payload = await response.json();
  if (payload.error) throw new Error(payload.error.message);
  return payload.result;
}

const queue = JSON.parse(fs.readFileSync(queuePath, "utf8"));
const pending = [];
for (const entry of queue.pending || []) {
  if (await reward(entry.target) !== null) pending.push(entry);
}
const expired = (queue.pending || []).length - pending.length;
if (expired > 0) {
  queue.pending = pending;
  const temporary = `${queuePath}.tmp`;
  fs.writeFileSync(temporary, JSON.stringify(queue));
  fs.renameSync(temporary, queuePath);
}
console.log(JSON.stringify({ queue: queuePath, expired, remaining: pending.length }));
