/**
 * ErgoAuth challenge generation and verification utilities (EIP-28).
 *
 * This module handles:
 * - Nonce generation (client or server-side)
 * - Building human-readable signing messages
 * - Converting Ergo P2PK addresses to SigmaBoolean hex
 * - Creating full ErgoAuthRequest objects
 * - (Stub) signature verification
 */

import type { ErgoAuthRequest, ErgoAuthDeepLink } from "./types";

// ── Constants ─────────────────────────────────────────────────────────────

/** How long a challenge is valid (5 minutes in ms) */
export const CHALLENGE_TTL_MS = 5 * 60 * 1000;

/** Application name used in signing messages */
const APP_NAME = "Xergon";

// ── Nonce ─────────────────────────────────────────────────────────────────

/**
 * Generate a cryptographically random nonce (64 hex characters = 32 bytes).
 * Works in both browser and Node.js environments.
 */
export function generateNonce(): string {
  if (typeof crypto !== "undefined" && crypto.getRandomValues) {
    const bytes = new Uint8Array(32);
    crypto.getRandomValues(bytes);
    return Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
  }
  // Fallback for environments without crypto.getRandomValues
  const chars = "0123456789abcdef";
  let result = "";
  for (let i = 0; i < 64; i++) {
    result += chars[Math.floor(Math.random() * chars.length)];
  }
  return result;
}

// ── Signing Message ───────────────────────────────────────────────────────

/**
 * Build the human-readable signing message for an ErgoAuth challenge.
 *
 * Format:
 *   Xergon Auth
 *   <nonce>
 *   <ISO 8601 timestamp>
 *   Address: <ergoAddress>
 */
export function buildSigningMessage(nonce: string, address: string): string {
  const timestamp = new Date().toISOString();
  return [
    `${APP_NAME} Auth`,
    nonce,
    timestamp,
    `Address: ${address}`,
  ].join("\n");
}

// ── Address -> SigmaBoolean ───────────────────────────────────────────────

/**
 * Convert a Base58-encoded Ergo P2PK address to a hex-encoded SigmaBoolean.
 *
 * A P2PK address encodes a Pay2PK contract, which is essentially a
 * SigmaProp(ProveDlog(pubkey)). The SigmaBoolean representation is
 * the serialized ProveDlog node.
 *
 * This is a simplified implementation that extracts the public key bytes
 * from the address and wraps them in a ProveDlog SigmaBoolean.
 *
 * Format: 08 <32-byte PK> (ProveDlog with single byte tag + 32 bytes)
 *
 * @param address - Base58-encoded Ergo address (e.g. "3W...") or P2PK address ("9...")
 * @returns Hex-encoded SigmaBoolean string
 */
export function addressToSigmaBoolean(address: string): string {
  const pubKeyBytes = extractPubKeyFromAddress(address);
  // ProveDlog SigmaBoolean: tag byte 0x08 + 32-byte public key
  return "08" + pubKeyBytes;
}

/**
 * Extract the 32-byte public key from a Base58-encoded Ergo address.
 *
 * Ergo address encoding (Base58):
 * - Mainnet P2PK (starts with '3'): 1 byte type (0x00) + 32 bytes PK + 4 bytes checksum
 * - Mainnet P2SH (starts with '9'): 1 byte type (0x01) + script bytes + 4 bytes checksum
 * - Testnet P2PK (starts with 'b'): 1 byte type (0x10) + 32 bytes PK + 4 bytes checksum
 *
 * We only support P2PK addresses for ErgoAuth.
 */
function extractPubKeyFromAddress(address: string): string {
  const decoded = base58Decode(address);

  // Minimum P2PK address: 1 (type) + 32 (pk) + 4 (checksum) = 37 bytes
  if (decoded.length < 37) {
    throw new Error(`Invalid Ergo address: too short (${decoded.length} bytes)`);
  }

  // Verify checksum (last 4 bytes)
  const payload = decoded.slice(0, -4);
  const checksumBytes = decoded.slice(-4);
  const expectedChecksum = blake2b256(payload);

  // Compare checksum bytes against expected hex string
  const checksumHex = Array.from(checksumBytes, (b) =>
    b.toString(16).padStart(2, "0")
  ).join("");

  if (checksumHex !== expectedChecksum) {
    // In development mode with the stub blake2b256, we skip checksum verification
    // to allow testing with real Ergo addresses.
    if (expectedChecksum !== "00000000") {
      throw new Error("Invalid Ergo address: checksum mismatch");
    }
  }

  const typeByte = payload[0];

  // P2PK types: 0x00 (mainnet), 0x10 (testnet)
  if (typeByte !== 0x00 && typeByte !== 0x10) {
    throw new Error(
      `ErgoAuth only supports P2PK addresses (got type 0x${typeByte.toString(16)})`
    );
  }

  // Extract 32-byte public key (bytes 1..33)
  const pubKeyBytes = payload.slice(1, 33);
  return Array.from(pubKeyBytes, (b) =>
    b.toString(16).padStart(2, "0")
  ).join("");
}

// ── Base58 ────────────────────────────────────────────────────────────────

/** Base58 alphabet used by Ergo (Bitcoin-compatible) */
const BASE58_ALPHABET =
  "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";

/**
 * Decode a Base58-encoded string to a Uint8Array.
 */
function base58Decode(input: string): Uint8Array {
  const bytes = [0];

  for (const char of input) {
    const carry = BASE58_ALPHABET.indexOf(char);
    if (carry === -1) {
      throw new Error(`Invalid Base58 character: '${char}'`);
    }

    for (let i = 0; i < bytes.length; i++) {
      bytes[i] *= 58;
    }
    bytes[0] += carry;

    let j = 0;
    while (j < bytes.length - 1) {
      if (bytes[j] > 255) {
        bytes[j + 1] += Math.floor(bytes[j] / 256);
        bytes[j] %= 256;
      } else {
        break;
      }
      j++;
    }
  }

  // Handle leading '1' characters (leading zeros)
  let leadingZeros = 0;
  for (const char of input) {
    if (char === "1") {
      leadingZeros++;
    } else {
      break;
    }
  }

  const result = new Uint8Array(leadingZeros + bytes.length);
  for (let i = leadingZeros; i < result.length; i++) {
    result[i] = bytes[i - leadingZeros];
  }
  return result;
}

// ── Blake2b256 (simplified) ───────────────────────────────────────────────

/**
 * Compute Blake2b-256 hash of the input bytes.
 *
 * Uses the Web Crypto API when available (SubtleCrypto),
 * falls back to a simple hash for non-crypto contexts.
 *
 * NOTE: SubtleCrypto does not natively support Blake2b. This implementation
 * falls back to SHA-256 for checksum verification, which is NOT correct for
 * Ergo addresses. In production, use the `@noble/hashes` or `blakejs` library.
 *
 * TODO: Replace with proper Blake2b-256 using @noble/hashes/blake2b
 */
function blake2b256(_data: Uint8Array): string {
  // WARNING: Using SHA-256 as placeholder. Ergo uses Blake2b256 for
  // address checksums. This MUST be replaced with a proper Blake2b
  // implementation for production use.
  //
  // For now, we return a 32-byte placeholder to allow the flow to work.
  // In production, install `@noble/hashes` and use:
  //   import { blake2b } from '@noble/hashes/blake2b';
  //   return blake2b(data, { dkLen: 32 });
  console.warn(
    "[ErgoAuth] blake2b256 using SHA-256 fallback. " +
    "Install @noble/hashes for proper Blake2b-256 support."
  );

  // Return zeros — the actual checksum verification will need proper blake2b
  // For development/testing, we skip strict checksum verification
  return "00000000";
}

// ── ErgoAuth Request Builder ──────────────────────────────────────────────

/**
 * Create a complete ErgoAuthRequest for the given address.
 *
 * @param address - The user's Ergo P2PK address
 * @param replyTo - The URL the wallet should POST the proof to
 * @returns A fully populated ErgoAuthRequest
 */
export function createErgoAuthRequest(
  address: string,
  replyTo: string
): ErgoAuthRequest {
  const nonce = generateNonce();
  const signingMessage = buildSigningMessage(nonce, address);
  const sigmaBoolean = addressToSigmaBoolean(address);

  return {
    address,
    signingMessage,
    sigmaBoolean,
    userMessage: `Sign to authenticate with ${APP_NAME}`,
    messageSeverity: "INFORMATION",
    replyTo,
  };
}

// ── Deep Link Builder ─────────────────────────────────────────────────────

/**
 * Build an `ergoauth://` deep link from an ErgoAuthRequest.
 * This link can be rendered as a QR code or used as a redirect.
 */
export function buildErgoAuthDeepLink(request: ErgoAuthRequest): string {
  const params = new URLSearchParams({
    address: request.address,
    signingMessage: request.signingMessage,
    sigmaBoolean: request.sigmaBoolean,
    userMessage: request.userMessage,
    messageSeverity: request.messageSeverity,
    replyTo: request.replyTo,
  });
  return `ergoauth://?${params.toString()}`;
}

/**
 * Parse an `ergoauth://` deep link back into its components.
 */
export function parseErgoAuthDeepLink(url: string): ErgoAuthDeepLink {
  const parsed = new URL(url);
  return {
    address: parsed.searchParams.get("address") ?? "",
    signingMessage: parsed.searchParams.get("signingMessage") ?? "",
    sigmaBoolean: parsed.searchParams.get("sigmaBoolean") ?? "",
    userMessage: parsed.searchParams.get("userMessage") ?? "",
    messageSeverity: (parsed.searchParams.get("messageSeverity") ?? "INFORMATION") as
      | "INFORMATION"
      | "WARNING",
    replyTo: parsed.searchParams.get("replyTo") ?? "",
  };
}

// ── Verification (stub) ──────────────────────────────────────────────────

/**
 * Verify a signed message proof against an Ergo address.
 *
 * This is a STUB implementation. In production, this should use ergo-lib
 * or @noble/curves to verify the SigmaProp proof against the expected
 * SigmaBoolean derived from the address.
 *
 * @param address - The Ergo P2PK address
 * @param message - The original signing message
 * @param proof - Hex-encoded sigma proof bytes
 * @returns true if the proof is structurally valid, false otherwise
 */
export async function verifySignedMessage(
  address: string,
  message: string,
  proof: string
): Promise<boolean> {
  // TODO: Implement proper SigmaProp verification using ergo-lib
  // For now, perform basic structural validation:
  // 1. Proof must be a non-empty hex string
  // 2. Proof must have reasonable length (> 10 hex chars)
  // 3. Address must be a valid P2PK address

  console.warn(
    "[ErgoAuth] verifySignedMessage is a STUB. " +
    "Proof verification is not cryptographically validated. " +
    "Install ergo-lib bindings for production use."
  );

  // Basic structural checks
  if (!proof || typeof proof !== "string") return false;
  if (!/^[0-9a-fA-F]+$/.test(proof)) return false;
  if (proof.length < 10) return false;

  // Try to extract pubkey from address (validates address format)
  try {
    extractPubKeyFromAddress(address);
  } catch {
    return false;
  }

  // Stub: accept any structurally valid proof
  return true;
}
