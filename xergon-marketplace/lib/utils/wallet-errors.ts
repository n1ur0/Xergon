/**
 * Wallet error parsing utilities.
 *
 * Normalizes raw errors from the EIP-12 wallet connector into structured,
 * user-friendly error objects with a type classification and recoverability flag.
 */

export type WalletErrorType =
  | "NOT_INSTALLED"
  | "REJECTED"
  | "LOCKED"
  | "TIMEOUT"
  | "NETWORK"
  | "UNKNOWN";

export interface ParsedWalletError {
  type: WalletErrorType;
  message: string;
  recoverable: boolean;
}

/** Error patterns and their parsed forms */
const ERROR_PATTERNS: Array<{
  test: (msg: string) => boolean;
  type: WalletErrorType;
  message: string;
  recoverable: boolean;
}> = [
  {
    test: (msg) =>
      /not (available|installed|found)|no.*wallet.*extension/i.test(msg),
    type: "NOT_INSTALLED",
    message:
      "Wallet extension not found. Install Nautilus from https://nautiluswallet.com/.",
    recoverable: false,
  },
  {
    test: (msg) =>
      /reject|denied|cancelled|user.*refused|connection.*rejected/i.test(msg),
    type: "REJECTED",
    message: "Connection cancelled.",
    recoverable: false,
  },
  {
    test: (msg) =>
      /locked|unlock|password|passphrase/i.test(msg),
    type: "LOCKED",
    message: "Please unlock your wallet and try again.",
    recoverable: true,
  },
  {
    test: (msg) =>
      /timeout|timed?\s*out|not responding|took too long/i.test(msg),
    type: "TIMEOUT",
    message: "Wallet is not responding. Try refreshing the page.",
    recoverable: true,
  },
  {
    test: (msg) =>
      /network|fetch|ECONNREFUSED|Failed to fetch|no internet/i.test(msg),
    type: "NETWORK",
    message: "Network error. Check your internet connection and try again.",
    recoverable: true,
  },
];

/**
 * Parse a raw wallet error into a structured { type, message, recoverable } object.
 *
 * Recovers gracefully from non-Error values (strings, objects without message, etc).
 */
export function parseWalletError(error: unknown): ParsedWalletError {
  const msg = extractMessage(error);

  for (const pattern of ERROR_PATTERNS) {
    if (pattern.test(msg)) {
      return {
        type: pattern.type,
        message: pattern.message,
        recoverable: pattern.recoverable,
      };
    }
  }

  return {
    type: "UNKNOWN",
    message: msg || "An unexpected wallet error occurred.",
    recoverable: false,
  };
}

function extractMessage(error: unknown): string {
  if (error instanceof Error) return error.message;
  if (typeof error === "string") return error;
  if (error && typeof error === "object") {
    // Some wallet extensions throw objects with a `message` or `reason` field
    const obj = error as Record<string, unknown>;
    if (typeof obj.message === "string") return obj.message;
    if (typeof obj.reason === "string") return obj.reason;
  }
  return "";
}
