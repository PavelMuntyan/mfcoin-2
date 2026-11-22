import fs from "node:fs";

const [secretPath, rpcPort, publicKey] = process.argv.slice(2);
if (!secretPath || !rpcPort || !publicKey) throw new Error("Usage: insert-grandpa-key.mjs <secret> <rpcPort> <publicKey>");
const { secretPhrase } = JSON.parse(fs.readFileSync(secretPath, "utf8"));
if (!secretPhrase) throw new Error("Missing GRANDPA secret phrase");
const response = await fetch(`http://127.0.0.1:${rpcPort}`, {
  method: "POST",
  headers: { "content-type": "application/json" },
  body: JSON.stringify({
    jsonrpc: "2.0",
    id: 1,
    method: "author_insertKey",
    params: ["gran", secretPhrase, publicKey]
  }),
  signal: AbortSignal.timeout(5000)
});
const payload = await response.json();
if (payload.error) throw new Error(payload.error.message);
console.log(JSON.stringify({ inserted: true }));
