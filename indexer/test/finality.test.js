import test from "node:test";
import assert from "node:assert/strict";
import { assertFinalityHealthy, finalityStatus } from "../src/finality.js";

test("finality is healthy within the configured lag", () => {
  assert.deepEqual(finalityStatus(10, 8, 2), {
    bestNumber: 10,
    finalizedNumber: 8,
    lag: 2,
    status: "healthy",
    maximumLag: 2
  });
});

test("finality delay is reported and blocks high-risk operations", () => {
  assert.equal(finalityStatus(20, 4, 6).status, "finality_delayed");
  assert.throws(() => assertFinalityHealthy(20, 4, 6), error => {
    assert.equal(error.code, "finality_delayed");
    return true;
  });
});
