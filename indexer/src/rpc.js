export function createRpc(endpoint) {
  if (!endpoint) throw new Error("CHAIN_RPC_URL is required");
  return async function rpc(method, params = []) {
    const response = await fetch(endpoint, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ jsonrpc: "2.0", id: 1, method, params }),
      signal: AbortSignal.timeout(10_000)
    });
    if (!response.ok) throw new Error(`${method}: HTTP ${response.status}`);
    const payload = await response.json();
    if (payload.error) throw new Error(`${method}: ${payload.error.message}`);
    return payload.result;
  };
}

export function blockNumber(header) {
  return Number(header.number);
}
