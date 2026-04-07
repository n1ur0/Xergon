/**
 * Offline wallet utilities for key generation, address derivation,
 * and message signing/verification.
 *
 * Uses @noble/curves/secp256k1 for all cryptographic operations.
 * These utilities enable server-side signing without a browser wallet.
 */

import { secp256k1 } from '@noble/curves/secp256k1.js';

// ── Ergo P2PK Address Constants ──

/** Ergo mainnet address type prefix byte. */
const ERGO_MAINNET_PREFIX = 0x00;

/** Base58 alphabet used by Ergo addresses. */
const BASE58_ALPHABET = '123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz';

/**
 * Encode a byte array to Base58.
 */
function base58Encode(bytes: Uint8Array): string {
  let num = 0n;
  for (const b of bytes) {
    num = num * 256n + BigInt(b);
  }

  let result = '';
  while (num > 0n) {
    const remainder = Number(num % 58n);
    result = BASE58_ALPHABET[remainder] + result;
    num = num / 58n;
  }

  // Prepend '1' for each leading zero byte
  for (const b of bytes) {
    if (b === 0) {
      result = '1' + result;
    } else {
      break;
    }
  }

  return result;
}

/**
 * Decode a Base58 string to a byte array.
 */
function base58Decode(str: string): Uint8Array {
  let num = 0n;
  for (const ch of str) {
    const idx = BASE58_ALPHABET.indexOf(ch);
    if (idx === -1) throw new Error(`Invalid Base58 character: ${ch}`);
    num = num * 58n + BigInt(idx);
  }

  // Count leading '1's (which represent leading zero bytes)
  let leadingZeros = 0;
  for (const ch of str) {
    if (ch === '1') leadingZeros++;
    else break;
  }

  // Convert bigint to bytes
  const hex = num.toString(16).padStart(2, '0');
  const bytes = new Uint8Array(
    Buffer.from(hex.length % 2 ? '0' + hex : hex, 'hex'),
  );

  // Prepend leading zero bytes
  return new Uint8Array([...new Uint8Array(leadingZeros), ...bytes]);
}

/**
 * Compute a 32-byte hash for address checksum.
 *
 * Attempts to use blake2b256 from @noble/hashes if available
 * (which is the correct algorithm for Ergo addresses).
 * Falls back to double-SHA-256 for environments without @noble/hashes.
 */
async function addressHash(data: Uint8Array): Promise<Uint8Array> {
  try {
    // eslint-disable-next-line @typescript-eslint/no-require-imports
    const { blake2b } = await import('@noble/hashes/blake2b.js');
    return blake2b(data, { dkLen: 32 });
  } catch {
    // Fallback: double SHA-256 (produces 32-byte hash for checksum)
    const first = await crypto.subtle.digest('SHA-256', data as BufferSource);
    const second = await crypto.subtle.digest('SHA-256', first);
    return new Uint8Array(second);
  }
}

/**
 * Derive a P2PK address from a hex-encoded compressed public key (33 bytes).
 *
 * The address is encoded as Base58 of: version(1) + pubkey(33) + checksum(4).
 *
 * NOTE: For production-correct Ergo addresses, install `@noble/hashes`.
 * Without it, the fallback uses double-SHA-256 which produces a valid-looking
 * but cryptographically different checksum.
 *
 * @param publicKey - Hex-encoded compressed public key (33 bytes, 66 hex chars)
 * @returns The Ergo P2PK address string
 *
 * @example
 * ```ts
 * const address = await deriveAddress('02ab12cd...');
 * // => "9eZ24..."
 * ```
 */
export async function deriveAddress(publicKey: string): Promise<string> {
  const pk = hexToBytes(publicKey);

  if (pk.length !== 33) {
    throw new Error(`Invalid public key length: expected 33 bytes, got ${pk.length}`);
  }

  // Build the address payload: version byte + public key bytes
  const payload = new Uint8Array([ERGO_MAINNET_PREFIX, ...pk]);

  // Compute checksum (first 4 bytes of hash)
  const hash = await addressHash(payload);
  const checksum = hash.slice(0, 4);

  // Encode as Base58
  const addressBytes = new Uint8Array([...payload, ...checksum]);
  return base58Encode(addressBytes);
}

/**
 * Derive the public key from a hex-encoded secret key (32 bytes).
 *
 * @param secretKey - Hex-encoded secp256k1 secret key (32 bytes, 64 hex chars)
 * @returns Hex-encoded compressed public key (33 bytes, 66 hex chars)
 *
 * @example
 * ```ts
 * const publicKey = derivePublicKey('0123abcd...');
 * // => "02ab12cd..."
 * ```
 */
export function derivePublicKey(secretKey: string): string {
  const sk = hexToBytes(secretKey);

  if (sk.length !== 32) {
    throw new Error(`Invalid secret key length: expected 32 bytes, got ${sk.length}`);
  }

  const pk = secp256k1.getPublicKey(sk, true);
  return bytesToHex(pk);
}

/**
 * Generate a new secp256k1 keypair.
 *
 * @returns An object with hex-encoded secretKey (32 bytes) and publicKey (33 bytes)
 *
 * @example
 * ```ts
 * const { secretKey, publicKey } = generateKeypair();
 * const address = await deriveAddress(publicKey);
 * ```
 */
export function generateKeypair(): { secretKey: string; publicKey: string } {
  const privateKey = secp256k1.utils.randomSecretKey();
  const publicKey = secp256k1.getPublicKey(privateKey, true);
  return {
    secretKey: bytesToHex(privateKey),
    publicKey: bytesToHex(publicKey),
  };
}

/**
 * Sign a message using secp256k1 (for ErgoAuth and similar protocols).
 *
 * The message is hashed with SHA-256 before signing, following the
 * Ergo wallet signing convention.
 *
 * @param message - The message string to sign
 * @param secretKey - Hex-encoded secp256k1 secret key (32 bytes)
 * @returns Hex-encoded signature (64 bytes, 128 hex chars)
 *
 * @example
 * ```ts
 * const signature = await signMessage('Hello Ergo', secretKey);
 * const valid = await verifySignature('Hello Ergo', signature, publicKey);
 * ```
 */
export async function signMessage(message: string, secretKey: string): Promise<string> {
  const sk = hexToBytes(secretKey);

  if (sk.length !== 32) {
    throw new Error(`Invalid secret key length: expected 32 bytes, got ${sk.length}`);
  }

  // Hash the message with SHA-256 first (Ergo convention)
  const msgBytes = new TextEncoder().encode(message);
  const hash = await crypto.subtle.digest('SHA-256', msgBytes as BufferSource);
  const hashBytes = new Uint8Array(hash);

  // Sign the hash -- returns raw 64-byte compact signature
  const signature = secp256k1.sign(hashBytes, sk);
  return bytesToHex(signature);
}

/**
 * Verify a message signature using secp256k1.
 *
 * The message is hashed with SHA-256 before verification, matching
 * the signing convention used by `signMessage`.
 *
 * @param message - The original message string
 * @param signature - Hex-encoded signature (64 bytes)
 * @param publicKey - Hex-encoded compressed public key (33 bytes)
 * @returns true if the signature is valid
 *
 * @example
 * ```ts
 * const valid = await verifySignature('Hello Ergo', signature, publicKey);
 * ```
 */
export async function verifySignature(
  message: string,
  signature: string,
  publicKey: string,
): Promise<boolean> {
  const sig = hexToBytes(signature);
  const pk = hexToBytes(publicKey);

  if (sig.length !== 64) {
    throw new Error(`Invalid signature length: expected 64 bytes, got ${sig.length}`);
  }

  if (pk.length !== 33 && pk.length !== 65) {
    throw new Error(`Invalid public key length: expected 33 or 65 bytes, got ${pk.length}`);
  }

  // Hash the message the same way as signing
  const msgBytes = new TextEncoder().encode(message);
  const hash = await crypto.subtle.digest('SHA-256', msgBytes as BufferSource);
  const hashBytes = new Uint8Array(hash);

  try {
    return secp256k1.verify(sig, hashBytes, pk);
  } catch {
    return false;
  }
}

// ── Hex Conversion Helpers ──

/**
 * Convert a hex string to a Uint8Array.
 */
function hexToBytes(hex: string): Uint8Array {
  const cleaned = hex.replace(/^0x/, '');
  if (cleaned.length % 2 !== 0) {
    throw new Error(`Invalid hex string: odd length (${cleaned.length})`);
  }
  return new Uint8Array(Buffer.from(cleaned, 'hex'));
}

/**
 * Convert a Uint8Array to a hex string (no 0x prefix).
 */
function bytesToHex(bytes: Uint8Array): string {
  return Buffer.from(bytes).toString('hex');
}
