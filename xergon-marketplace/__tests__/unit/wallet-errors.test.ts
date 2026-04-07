import { describe, it, expect } from "vitest";
import { parseWalletError } from "@/lib/utils/wallet-errors";

describe("parseWalletError", () => {
  it("parses NOT_INSTALLED errors", () => {
    const result = parseWalletError(new Error("Wallet extension not found"));
    expect(result.type).toBe("NOT_INSTALLED");
    expect(result.recoverable).toBe(false);
    expect(result.message).toContain("Install Nautilus");
  });

  it("parses REJECTED errors", () => {
    const result = parseWalletError(new Error("User rejected the connection"));
    expect(result.type).toBe("REJECTED");
    expect(result.recoverable).toBe(false);
  });

  it("parses LOCKED errors", () => {
    const result = parseWalletError(new Error("Wallet is locked"));
    expect(result.type).toBe("LOCKED");
    expect(result.recoverable).toBe(true);
  });

  it("parses TIMEOUT errors", () => {
    const result = parseWalletError(new Error("Connection timed out"));
    expect(result.type).toBe("TIMEOUT");
    expect(result.recoverable).toBe(true);
  });

  it("parses NETWORK errors", () => {
    const result = parseWalletError(new Error("Failed to fetch"));
    expect(result.type).toBe("NETWORK");
    expect(result.recoverable).toBe(true);
  });

  it("parses ECONNREFUSED as NETWORK", () => {
    const result = parseWalletError(new Error("ECONNREFUSED"));
    expect(result.type).toBe("NETWORK");
  });

  it("returns UNKNOWN for unrecognized errors", () => {
    const result = parseWalletError(new Error("Something completely unexpected"));
    expect(result.type).toBe("UNKNOWN");
    expect(result.recoverable).toBe(false);
  });

  it("handles string errors", () => {
    const result = parseWalletError("Wallet not available");
    expect(result.type).toBe("NOT_INSTALLED");
  });

  it("handles object errors with message field", () => {
    const result = parseWalletError({ message: "connection rejected" });
    expect(result.type).toBe("REJECTED");
  });

  it("handles object errors with reason field", () => {
    const result = parseWalletError({ reason: "Wallet is locked" });
    expect(result.type).toBe("LOCKED");
  });

  it("handles null/undefined errors", () => {
    const result = parseWalletError(null);
    expect(result.type).toBe("UNKNOWN");
    expect(result.message).toContain("unexpected");
  });
});
