import { describe, it, expect } from "vitest";
import {
  generateNonce,
  buildSigningMessage,
  buildErgoAuthDeepLink,
  parseErgoAuthDeepLink,
  CHALLENGE_TTL_MS,
} from "@/lib/ergoauth/challenge";

describe("generateNonce", () => {
  it("returns a 64-character hex string", () => {
    const nonce = generateNonce();
    expect(nonce).toHaveLength(64);
    expect(nonce).toMatch(/^[0-9a-f]{64}$/);
  });

  it("returns unique values on successive calls", () => {
    const nonce1 = generateNonce();
    const nonce2 = generateNonce();
    expect(nonce1).not.toBe(nonce2);
  });
});

describe("buildSigningMessage", () => {
  it("includes the app name, nonce, and address", () => {
    const nonce = "a".repeat(64);
    const address = "3WxT1BtF4WkF2hY3j3eJcMbPGPpKqBZ9rGs";
    const msg = buildSigningMessage(nonce, address);

    expect(msg).toContain("Xergon Auth");
    expect(msg).toContain(nonce);
    expect(msg).toContain(`Address: ${address}`);
  });

  it("contains a valid ISO 8601 timestamp", () => {
    const msg = buildSigningMessage("a".repeat(64), "3WxT1BtF4WkF2hY3j3eJcMbPGPpKqBZ9rGs");
    const lines = msg.split("\n");
    expect(lines.length).toBe(4);
    // Line 2 should be a valid ISO date
    expect(new Date(lines[2]).getTime()).not.toBeNaN();
  });

  it("joins parts with newlines", () => {
    const msg = buildSigningMessage("a".repeat(64), "3Wtest");
    expect(msg).toContain("\n");
  });
});

describe("CHALLENGE_TTL_MS", () => {
  it("equals 5 minutes", () => {
    expect(CHALLENGE_TTL_MS).toBe(5 * 60 * 1000);
  });
});

describe("buildErgoAuthDeepLink", () => {
  it("produces an ergoauth:// URL with all required params", () => {
    const request = {
      address: "3WxT1BtF4WkF2hY3j3eJcMbPGPpKqBZ9rGs",
      signingMessage: "Xergon Auth\nabc\n2024-01-01T00:00:00.000Z\nAddress: 3WxT1BtF4WkF2hY3j3eJcMbPGPpKqBZ9rGs",
      sigmaBoolean: "08abc123",
      userMessage: "Sign to authenticate with Xergon",
      messageSeverity: "INFORMATION" as const,
      replyTo: "https://example.com/callback",
    };

    const url = buildErgoAuthDeepLink(request);
    expect(url).toMatch(/^ergoauth:\/\/\?/);
    expect(url).toContain("address=3WxT1BtF4WkF2hY3j3eJcMbPGPpKqBZ9rGs");
    expect(url).toContain("sigmaBoolean=08abc123");
    expect(url).toContain("messageSeverity=INFORMATION");
    expect(url).toContain("replyTo=");
  });
});

describe("parseErgoAuthDeepLink", () => {
  it("round-trips with buildErgoAuthDeepLink", () => {
    const request = {
      address: "3WxT1BtF4WkF2hY3j3eJcMbPGPpKqBZ9rGs",
      signingMessage: "Xergon Auth\ntestnonce\n2024-01-01T00:00:00.000Z\nAddress: 3WxT1BtF4WkF2hY3j3eJcMbPGPpKqBZ9rGs",
      sigmaBoolean: "08deadbeef",
      userMessage: "Sign to authenticate with Xergon",
      messageSeverity: "INFORMATION" as const,
      replyTo: "https://example.com/callback",
    };

    const url = buildErgoAuthDeepLink(request);
    const parsed = parseErgoAuthDeepLink(url);

    expect(parsed.address).toBe(request.address);
    expect(parsed.signingMessage).toBe(request.signingMessage);
    expect(parsed.sigmaBoolean).toBe(request.sigmaBoolean);
    expect(parsed.userMessage).toBe(request.userMessage);
    expect(parsed.messageSeverity).toBe(request.messageSeverity);
    expect(parsed.replyTo).toBe(request.replyTo);
  });

  it("defaults messageSeverity to INFORMATION when missing", () => {
    const url = "ergoauth://?address=3Wtest&signingMessage=test&sigmaBoolean=08ab&userMessage=msg&replyTo=https://example.com";
    const parsed = parseErgoAuthDeepLink(url);
    expect(parsed.messageSeverity).toBe("INFORMATION");
  });

  it("defaults fields to empty strings when params missing", () => {
    const url = "ergoauth://?";
    const parsed = parseErgoAuthDeepLink(url);
    expect(parsed.address).toBe("");
    expect(parsed.signingMessage).toBe("");
    expect(parsed.sigmaBoolean).toBe("");
  });
});
