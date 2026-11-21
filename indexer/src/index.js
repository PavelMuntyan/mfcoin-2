import postgres from "postgres";
import { createRpc } from "./rpc.js";
import { FreelandIndexer } from "./indexer.js";
import { createApiServer } from "./server.js";

const databaseUrl = process.env.DATABASE_URL ||
  "postgres://mfcoin_indexer:indexer_dev_only@127.0.0.1:55432/mfcoin_indexer_dev";
const chainRpcUrl = process.env.CHAIN_RPC_URL || "http://127.0.0.1:9944";
const host = process.env.INDEXER_HOST || "127.0.0.1";
const port = Number(process.env.INDEXER_PORT || 8787);
const pollInterval = Number(process.env.INDEXER_POLL_INTERVAL_MS || 500);

const sql = postgres(databaseUrl, { max: 10 });
const rpc = createRpc(chainRpcUrl);
const indexer = new FreelandIndexer(sql, rpc, {
  maximumFinalityLag: Number(process.env.FINALITY_MAX_LAG || 6)
});

let chainReady = false;
const chainWaitAttempts = Number(process.env.CHAIN_RPC_WAIT_ATTEMPTS || 600);
for (let attempt = 0; attempt < chainWaitAttempts && !chainReady; attempt += 1) {
  try {
    await rpc("chain_getBlockHash", [0]);
    chainReady = true;
  } catch {
    await new Promise(resolve => setTimeout(resolve, 500));
  }
}
if (!chainReady) throw new Error(`Chain RPC is unavailable at ${chainRpcUrl}`);
await indexer.init();
if (process.env.INDEXER_RESET === "1") {
  await sql`TRUNCATE freeland_records, freeland_block_info, indexer_state RESTART IDENTITY CASCADE`;
  await indexer.init();
}

const server = createApiServer(sql, rpc, indexer);
server.listen(port, host, () => {
  console.log(`Freeland indexer API listening on http://${host}:${port}`);
});

let syncing = false;
async function synchronize() {
  if (syncing) return;
  syncing = true;
  try {
    await indexer.sync();
  } catch (error) {
    console.error(`Sync failed: ${error.message}`);
  } finally {
    syncing = false;
  }
}

await synchronize();
const timer = setInterval(synchronize, pollInterval);

async function shutdown() {
  clearInterval(timer);
  server.close();
  await sql.end();
}
process.once("SIGINT", shutdown);
process.once("SIGTERM", shutdown);
