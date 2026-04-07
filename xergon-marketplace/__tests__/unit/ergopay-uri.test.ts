import { describe, it, expect } from "vitest";
import {
  encodeErgoPayUrl,
  encodeErgoPayDeepLink,
  decodeErgoPayCallback,
  isReducedTxSmallEnough,
} from "@/lib/ergopay/uri";

describe("encodeErgoPayUrl", () => {
  it("strips protocol and trailing slash, builds ergopay:// URL", () => {
    expect(encodeErgoPayUrl("https://relay.xergon.io/", "abc123")).toBe(
      "ergopay://relay.xergon.io/api/ergopay/request/abc123"
    );
  });

  it("handles http protocol", () => {
    expect(encodeErgoPayUrl("http://localhost:3000", "xyz")).toBe(
      "ergopay://localhost:3000/api/ergopay/request/xyz"
    );
  });

  it("handles URL without protocol", () => {
    expect(encodeErgoPayUrl("relay.xergon.io", "abc")).toBe(
      "ergopay://relay.xergon.io/api/ergopay/request/abc"
    );
  });
});

describe("encodeErgoPayDeepLink", () => {
  it("encodes the unsigned tx as ergopay:// prefixed", () => {
    const request = { unsignedTx: "abcd1234", fee: 1000000, inputsTotal: 2000000, outputsTotal: 1000000, dataInputs: [] };
    expect(encodeErgoPayDeepLink(request)).toBe("ergopay://abcd1234");
  });
});

describe("decodeErgoPayCallback", () => {
  it("returns txId for successful callback", () => {
    expect(decodeErgoPayCallback({ txId: "hash123" })).toEqual({ txId: "hash123" });
  });

  it("returns null for error-only body", () => {
    expect(decodeErgoPayCallback({ error: "rejected" })).toBeNull();
  });

  it("returns null for null/undefined", () => {
    expect(decodeErgoPayCallback(null)).toBeNull();
    expect(decodeErgoPayCallback(undefined)).toBeNull();
  });

  it("returns null for non-object", () => {
    expect(decodeErgoPayCallback("string")).toBeNull();
    expect(decodeErgoPayCallback(42)).toBeNull();
  });

  it("returns null for empty object", () => {
    expect(decodeErgoPayCallback({})).toBeNull();
  });

  it("handles signedTx field by generating a placeholder txId", () => {
    const result = decodeErgoPayCallback({ signedTx: "aabbccdd" });
    expect(result).not.toBeNull();
    expect(result!.txId).toHaveLength(64);
    expect(result!.txId).toMatch(/^[0-9a-f]{64}$/);
  });
});

describe("isReducedTxSmallEnough", () => {
  it("returns true for a small tx (100 hex chars = 50 bytes)", () => {
    expect(isReducedTxSmallEnough("a".repeat(100))).toBe(true);
  });

  it("returns false for a large tx (10000 hex chars = 5000 bytes)", () => {
    expect(isReducedTxSmallEnough("a".repeat(10000))).toBe(false);
  });

  it("returns true for exactly 4096 hex chars (2048 bytes)", () => {
    expect(isReducedTxSmallEnough("a".repeat(4096))).toBe(true);
  });

  it("returns false for 4098 hex chars (2049 bytes)", () => {
    expect(isReducedTxSmallEnough("a".repeat(4098))).toBe(false);
  });
});
