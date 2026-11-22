import fs from "node:fs";

const [secretPath, rpcPort, publicKey] = process.argv.slice(2);
if (!secretPath || !rpcPort || !publicKey) throw new Error("Usage: insert-grandpa-key.mjs <secret> <rpcPort> <publicKey>");

async function rpc(method, params) {
  const response = await fetch(`http://127.0.0.1:${rpcPort}`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ jsonrpc: "2.0", id: 1, method, params }),
    signal: AbortSignal.timeout(5000)
  });
  const payload = await response.json();
  if (payload.error) throw new Error(payload.error.message);
  return payload.result;
}

if (await rpc("author_hasKey", [publicKey, "gran"])) {
  console.log(JSON.stringify({ inserted: false, alreadyPresent: true }));
} else {
  const { secretPhrase } = JSON.parse(fs.readFileSync(secretPath, "utf8"));
  if (!secretPhrase) throw new Error("Missing GRANDPA secret phrase");
  await rpc("author_insertKey", ["gran", secretPhrase, publicKey]);
  console.log(JSON.stringify({ inserted: true }));
}
