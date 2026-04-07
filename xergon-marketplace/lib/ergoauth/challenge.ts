/**
 * ErgoAuth challenge generation and verification utilities (EIP-28).
 *
 * This module handles:
 * - Nonce generation (client or server-side)
 * - Building human-readable signing messages
 * - Converting Ergo P2PK addresses to SigmaBoolean hex
 * - Creating full ErgoAuthRequest objects
 * - Sigma protocol Schnorr signature verification
 */

import type { ErgoAuthRequest, ErgoAuthDeepLink } from "./types";
import { blake2b } from "@noble/hashes/blake2.js";
import { bytesToHex, hexToBytes, concatBytes } from "@noble/hashes/utils.js";
import { secp256k1 } from "@noble/curves/secp256k1.js";

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

  if (checksumHex !== expectedChecksum.slice(0, 8)) {
    throw new Error("Invalid Ergo address: checksum mismatch");
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
 * Uses @noble/hashes for a correct, cross-platform Blake2b-256 implementation.
 * Returns the 32-byte hash as a lowercase hex string (64 hex characters).
 */
function blake2b256(data: Uint8Array): string {
  const hash = blake2b(data, { dkLen: 32 });
  return bytesToHex(hash);
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

// ── Sigma Protocol Schnorr Verification ────────────────────────────────────

/**
 * Reduce a 32-byte big-endian hash to a scalar modulo the secp256k1 curve order.
 */
function modCurveOrder(hash: Uint8Array): Uint8Array {
  const hashInt = BigInt("0x" + bytesToHex(hash));
  const n = secp256k1.Point.CURVE().n;
  const reduced = hashInt % n;
  return hexToBytes(reduced.toString(16).padStart(64, "0"));
}

/**
 * Verify a signed message proof against an Ergo P2PK address.
 *
 * Implements Sigma protocol Schnorr verification:
 * 1. Extract public key from the address (32-byte x-coordinate)
 * 2. Reconstruct the compressed public key point (even-y, per Ergo convention)
 * 3. Build the SigmaBoolean bytes (0x08 + compressed pk)
 * 4. Parse the Schnorr proof: type(1) + challenge e(32) + response z(32)
 * 5. Recover the nonce point: R = z*G + e*pk
 * 6. Recompute the Fiat-Shamir challenge: e' = blake2b256(sigmaBoolean || R || message)
 * 7. Reduce to scalar mod curve order and compare with the proof's challenge
 *
 * @param address - The Ergo P2PK address
 * @param message - The original signing message
 * @param proof - Hex-encoded Schnorr proof bytes (65 bytes: type + e + z)
 * @returns true if the cryptographic proof verifies, false otherwise
 */
export async function verifySignedMessage(
  address: string,
  message: string,
  proof: string
): Promise<boolean> {
  try {
    // 1. Extract 32-byte public key x-coordinate from address
    const pubKeyHex = extractPubKeyFromAddress(address);
    const pubKeyXBytes = hexToBytes(pubKeyHex);

    // 2. Reconstruct compressed public key (Ergo convention: even y for 32-byte keys)
    const compressedPkHex = "02" + pubKeyHex;
    let pk: InstanceType<typeof secp256k1.Point>;
    try {
      pk = secp256k1.Point.fromHex(compressedPkHex);
    } catch {
      // If even-y fails, try odd-y (shouldn't happen for valid addresses)
      pk = secp256k1.Point.fromHex("03" + pubKeyHex);
    }

    // 3. Build SigmaBoolean bytes: ProveDlog tag (0x08) + compressed pk (33 bytes)
    const sigmaBooleanBytes = concatBytes(new Uint8Array([0x08]), hexToBytes(compressedPkHex));

    // 4. Parse the Schnorr proof
    const proofBytes = hexToBytes(proof);
    let challenge: Uint8Array;
    let response: Uint8Array;

    if (proofBytes.length >= 65 && proofBytes[0] === 0x01) {
      // Standard Sigma SchnorrProof: type(0x01) + e(32) + z(32) = 65 bytes
      challenge = proofBytes.slice(1, 33);
      response = proofBytes.slice(33, 65);
    } else if (proofBytes.length >= 64) {
      // Raw signature without type prefix
      challenge = proofBytes.slice(0, 32);
      response = proofBytes.slice(32, 64);
    } else {
      return false;
    }

    // 5. Recover the nonce point R = z*G + e*pk
    const G = secp256k1.Point.BASE;
    const e = BigInt("0x" + bytesToHex(challenge));
    const z = BigInt("0x" + bytesToHex(response));

    const R = G.multiply(z).add(pk.multiply(e));

    // R must not be the point at infinity
    if (R.equals(secp256k1.Point.ZERO)) return false;

    // 6. Recompute the Fiat-Shamir challenge
    //    e' = blake2b256(sigmaBoolean_bytes || R_compressed || message_bytes)
    const RCompressed = R.toBytes(true);
    const messageBytes = new TextEncoder().encode(message);
    const preimage = concatBytes(sigmaBooleanBytes, RCompressed, messageBytes);
    const hash = blake2b(preimage, { dkLen: 32 });
    const ePrime = modCurveOrder(hash);

    // 7. Compare: accept iff e' == e
    return bytesToHex(challenge) === bytesToHex(ePrime);
  } catch {
    return false;
  }
}
