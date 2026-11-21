const endpoint = process.env.DEVNET_RPC_URL || "http://127.0.0.1:9944";

async function rpc(method, params = []) {
  const response = await fetch(endpoint, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ jsonrpc: "2.0", id: 1, method, params })
  });
  if (!response.ok) throw new Error(`${method}: HTTP ${response.status}`);
  const payload = await response.json();
  if (payload.error) throw new Error(`${method}: ${payload.error.message}`);
  return payload.result;
}

function blockNumber(header) {
  return header ? Number(header.number) : 0;
}

const first = await rpc("chain_getHeader");
await new Promise(resolve => setTimeout(resolve, 8000));
const second = await rpc("chain_getHeader");
const finalizedHash = await rpc("chain_getFinalizedHead");
const finalized = await rpc("chain_getHeader", [finalizedHash]);
const bestBlock = await rpc("chain_getBlock");
const logs = bestBlock.block.header.digest.logs;
const powSeal = logs.find(log => log.startsWith("0x05706f775f"));
const freelandInfo = logs.find(log => log.startsWith("0x00904d464346") && log.length === 78);

if (blockNumber(second) <= blockNumber(first)) {
  throw new Error(`Best block did not advance: ${first.number} -> ${second.number}`);
}
if (!powSeal) throw new Error("PoW seal not found in the best block");
if (!freelandInfo) throw new Error("FreelandBlockInfo root not found in the best block");

console.log(JSON.stringify({
  firstBest: first.number,
  currentBest: second.number,
  finalized: finalized.number,
  finalizedHash,
  advancing: true,
  powSeal: true,
  freelandInfoRoot: `0x${freelandInfo.slice(-64)}`
}, null, 2));
