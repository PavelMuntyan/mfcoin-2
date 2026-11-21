import http from "node:http";

const kinds = new Set(["passport", "citizen", "election", "document", "revocation"]);
const hashPattern = /^0x[0-9a-fA-F]{64}$/;

function json(response, status, value) {
  const body = JSON.stringify(value);
  response.writeHead(status, {
    "content-type": "application/json; charset=utf-8",
    "content-length": Buffer.byteLength(body),
    "cache-control": "no-store"
  });
  response.end(body);
}

export function createApiServer(sql, rpc, indexer) {
  return http.createServer(async (request, response) => {
    try {
      if (request.method !== "GET") {
        json(response, 405, { error: "method_not_allowed" });
        return;
      }
      const url = new URL(request.url, "http://localhost");
      const parts = url.pathname.split("/").filter(Boolean);

      if (url.pathname === "/health") {
        await sql`SELECT 1`;
        json(response, 200, { database: "ok", indexer: await indexer.finality() });
        return;
      }
      if (url.pathname === "/freeland/finality") {
        json(response, 200, await indexer.finality());
        return;
      }
      if (parts[0] !== "freeland") {
        json(response, 404, { error: "not_found" });
        return;
      }

      if (parts[1] === "block-info" && parts.length === 3 && hashPattern.test(parts[2])) {
        const rows = await sql`
          SELECT * FROM freeland_block_info WHERE block_hash = ${parts[2]}
        `;
        json(response, rows[0] ? 200 : 404, rows[0] ?? { error: "not_found" });
        return;
      }

      if (parts[1] === "records" && parts.length === 3 && hashPattern.test(parts[2])) {
        const status = url.searchParams.get("status");
        if (status && !["included", "orphaned", "finalized"].includes(status)) {
          json(response, 400, { error: "invalid_status" });
          return;
        }
        const rows = status
          ? await sql`
              SELECT * FROM freeland_records
              WHERE subject_commitment = ${parts[2]} AND status = ${status}
              ORDER BY block_number DESC, extrinsic_index DESC LIMIT 100
            `
          : await sql`
              SELECT * FROM freeland_records
              WHERE subject_commitment = ${parts[2]}
              ORDER BY block_number DESC, extrinsic_index DESC LIMIT 100
            `;
        json(response, 200, rows);
        return;
      }

      if (parts[1] === "proofs" && parts.length === 4 && kinds.has(parts[2]) && hashPattern.test(parts[3])) {
        const at = url.searchParams.get("at");
        if (at && !hashPattern.test(at)) {
          json(response, 400, { error: "invalid_block_hash" });
          return;
        }
        const proof = await rpc("freeland_registryProof", [parts[2], parts[3], at || null]);
        let status = null;
        if (at) {
          const rows = await sql`SELECT status FROM freeland_block_info WHERE block_hash = ${at}`;
          status = rows[0]?.status ?? "unknown";
        }
        json(response, 200, { ...proof, blockHash: at, status });
        return;
      }

      json(response, 404, { error: "not_found" });
    } catch (error) {
      json(response, 500, { error: "internal_error", message: error.message });
    }
  });
}
