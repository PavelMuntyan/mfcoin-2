import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { AddressError, decodeAddress, encodeAddress, validateAddress } from "./address.js";

const vectors = JSON.parse(readFileSync(new URL("./test-vectors.json", import.meta.url), "utf8"));

function expectError(code, action) {
  assert.throws(action, error => error instanceof AddressError && error.code === code);
}

test("canonical vectors round-trip through independent encode and decode", () => {
  for (const vector of vectors.valid) {
    assert.equal(encodeAddress(vector.hrp, vector.publicKey), vector.address);
    const decoded = decodeAddress(vector.address);
    assert.equal(decoded.hrp, vector.hrp);
    assert.equal(decoded.network, vector.hrp === "mfcx" ? "mainnet" : "testnet");
    assert.equal(decoded.accountId, vector.publicKey);
    assert.equal(validateAddress(vector.hrp, vector.address), vector.publicKey);
    assert.equal(vector.address.length, vector.hrp.length + 60);
  }
});

test("network mismatch is rejected even with a valid checksum", () => {
  const vector = vectors.valid[0];
  expectError("network_mismatch", () => validateAddress("tmfcx", vector.address));
});

test("checksum and single-character changes are rejected", () => {
  const address = vectors.valid[0].address;
  const replacement = address.at(-1) === "q" ? "p" : "q";
  expectError("invalid_checksum", () => decodeAddress(`${address.slice(0, -1)}${replacement}`));
  expectError("invalid_checksum", () => decodeAddress(`${address.slice(0, 12)}q${address.slice(13)}`));
});

test("uppercase and mixed case are rejected", () => {
  const address = vectors.valid[0].address;
  expectError("invalid_case", () => decodeAddress(address.toUpperCase()));
  expectError("invalid_case", () => decodeAddress(`M${address.slice(1)}`));
});

test("wrong HRP, length and separator are rejected", () => {
  const address = vectors.valid[0].address;
  expectError("invalid_hrp", () => decodeAddress(address.replace("mfcx", "mf0x")));
  expectError("invalid_hrp", () => decodeAddress(address.slice(1)));
  expectError("invalid_length", () => decodeAddress(address.slice(0, -1)));
  expectError("invalid_length", () => decodeAddress(`${address}q`));
});

test("forbidden and non-ASCII characters are rejected", () => {
  assert.throws(
    () => decodeAddress("mfcx1q2w3e4r5t6y7u8i9o0p0a8s7d6f5g4h3j2k1l0z9x8c7v6b5n4m"),
    AddressError
  );
  const address = vectors.valid[0].address;
  expectError("invalid_character", () => decodeAddress(`${address.slice(0, 10)}i${address.slice(11)}`));
  expectError("invalid_character", () => decodeAddress(`${address.slice(0, 10)}o${address.slice(11)}`));
  expectError("invalid_character", () => decodeAddress(`${address.slice(0, 10)}b${address.slice(11)}`));
  expectError("invalid_character", () => decodeAddress(`${address.slice(0, 10)}б${address.slice(11)}`));
});

test("invalid account IDs and HRPs cannot be encoded", () => {
  expectError("invalid_hrp", () => encodeAddress("mf0x", vectors.valid[0].publicKey));
  expectError("invalid_account_id", () => encodeAddress("mfcx", "0x00"));
  expectError("invalid_account_id", () => encodeAddress("mfcx", vectors.valid[0].publicKey.slice(2)));
});
