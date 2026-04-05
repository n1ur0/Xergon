/**
 * HMAC-SHA256 signature generation and verification using Web Crypto API.
 *
 * Works in browsers (native SubtleCrypto) and Node.js 20+ (global.crypto).
 */

/**
 * Encode a string as UTF-8 Uint8Array.
 */
function encodeUtf8(str: string): Uint8Array {
  return new TextEncoder().encode(str);
}

/**
 * Decode a hex string to Uint8Array.
 */
function hexToBytes(hex: string): Uint8Array {
  const bytes = new Uint8Array(hex.length / 2);
  for (let i = 0; i < hex.length; i += 2) {
    bytes[i / 2] = parseInt(hex.substring(i, i + 2), 16);
  }
  return bytes;
}

/**
 * Encode Uint8Array as hex string.
 */
function bytesToHex(bytes: Uint8Array): string {
  return Array.from(bytes)
    .map((b) => b.toString(16).padStart(2, '0'))
    .join('');
}

/**
 * Import an HMAC key from raw bytes.
 */
async function importHmacKey(keyBytes: Uint8Array): Promise<CryptoKey> {
  return crypto.subtle.importKey(
    'raw',
    keyBytes.buffer as ArrayBuffer,
    { name: 'HMAC', hash: 'SHA-256' },
    false,
    ['sign', 'verify'],
  );
}

/**
 * Generate HMAC-SHA256 signature of a message using a hex-encoded private key.
 *
 * @param message - The string to sign (typically JSON body + timestamp)
 * @param privateKeyHex - Private key as hex string
 * @returns Signature as hex string
 */
export async function hmacSign(
  message: string,
  privateKeyHex: string,
): Promise<string> {
  const keyBytes = hexToBytes(privateKeyHex);
  const key = await importHmacKey(keyBytes);
  const data = encodeUtf8(message);
  const sig = await crypto.subtle.sign('HMAC', key, data.buffer as ArrayBuffer);
  return bytesToHex(new Uint8Array(sig));
}

/**
 * Verify an HMAC-SHA256 signature.
 *
 * @param message - The original message
 * @param signatureHex - Signature as hex string
 * @param privateKeyHex - Private key as hex string
 * @returns Whether the signature is valid
 */
export async function hmacVerify(
  message: string,
  signatureHex: string,
  privateKeyHex: string,
): Promise<boolean> {
  const keyBytes = hexToBytes(privateKeyHex);
  const key = await importHmacKey(keyBytes);
  const data = encodeUtf8(message);
  const sigBytes = hexToBytes(signatureHex);
  return crypto.subtle.verify('HMAC', key, sigBytes.buffer as ArrayBuffer, data.buffer as ArrayBuffer);
}

/**
 * Build the signed payload string for Xergon HMAC auth.
 *
 * Format: JSON body + timestamp (Unix seconds)
 */
export function buildHmacPayload(
  body: string,
  timestamp: number,
): string {
  return `${body}${timestamp}`;
}
