/**
 * ErgoPay URI encoding utilities (EIP-20).
 *
 * Handles ergopay:// deep links and URL-based signing request routing.
 */

import type {
  ErgoPaySigningRequest,
  ErgoPayTransactionSent,
  QrCodeData,
} from "./types";

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/** Max size for inline reduced tx in QR code (~2KB). QR version 10 can hold
 *  ~652 bytes in binary mode; version 40 can hold ~2953 bytes. We use 2KB
 *  as a conservative threshold for reliable scanning. */
const INLINE_TX_MAX_BYTES = 2048;

// ---------------------------------------------------------------------------
// Public helpers
// ---------------------------------------------------------------------------

/**
 * Build an ergopay:// deep link URL from a server URL and request ID.
 * Format: ergopay://baseUrl/api/ergopay/request/{id}
 */
export function encodeErgoPayUrl(
  baseUrl: string,
  requestId: string
): string {
  // Strip protocol and path-trailing-slash, build ergopay:// URI
  const clean = baseUrl.replace(/^https?:\/\//, "").replace(/\/+$/, "");
  return `ergopay://${clean}/api/ergopay/request/${requestId}`;
}

/**
 * Build an ergopay:// deep link with inline reduced transaction.
 * Format: ergopay://reducedTx:<base16>
 *
 * This avoids a server round-trip for small transactions.
 */
export function encodeErgoPayDeepLink(
  request: ErgoPaySigningRequest
): string {
  return `ergopay://${request.unsignedTx}`;
}

/**
 * Generate the full QR payload data for an ErgoPay transaction.
 * Chooses between inline (no server round-trip) and URL-based approach.
 */
export function generateQrPayload(
  request: ErgoPaySigningRequest,
  serverUrl: string,
  requestId: string
): QrCodeData {
  const ergoPayUrl = encodeErgoPayUrl(serverUrl, requestId);
  const reducedTx = request.unsignedTx;

  if (isReducedTxSmallEnough(reducedTx)) {
    return {
      ergoPayUrl,
      deepLink: encodeErgoPayDeepLink(request),
      reducedTx,
    };
  }

  return {
    ergoPayUrl,
    deepLink: ergoPayUrl,
  };
}

/**
 * Decode an ErgoPay callback body from the wallet.
 * The wallet POSTs back either:
 *   { txId: string }                          (transaction sent)
 *   { error: string }                          (user rejected / error)
 */
export function decodeErgoPayCallback(
  body: unknown
): ErgoPayTransactionSent | null {
  if (!body || typeof body !== "object") return null;

  const obj = body as Record<string, unknown>;

  // Successful signing: wallet returns txId
  if (typeof obj.txId === "string" && obj.txId.length > 0) {
    return { txId: obj.txId };
  }

  // Some wallets return the full signed transaction
  if (typeof obj.signedTx === "string" && obj.signedTx.length > 0) {
    // Derive a placeholder txId from the signed tx hash
    // In production, you'd submit to the node and get the real txId
    return { txId: hashToTxId(obj.signedTx) };
  }

  return null;
}

/**
 * Check if a reduced transaction is small enough to inline in a QR code.
 */
export function isReducedTxSmallEnough(tx: string): boolean {
  // Base16 encoding: each byte = 2 hex chars
  const byteLength = Math.ceil(tx.length / 2);
  return byteLength <= INLINE_TX_MAX_BYTES;
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/** Simple hash of a hex string to produce a 64-char txId placeholder. */
function hashToTxId(hex: string): string {
  // Simple deterministic hash - not cryptographically secure but sufficient
  // for a placeholder. In production, submit to node for real txId.
  let h1 = 0xdeadbeef;
  let h2 = 0x41c6ce57;
  for (let i = 0; i < hex.length; i++) {
    const ch = hex.charCodeAt(i);
    h1 = Math.imul(h1 ^ ch, 2654435761);
    h2 = Math.imul(h2 ^ ch, 1597334677);
  }
  h1 = Math.imul(h1 ^ (h1 >>> 16), 2246822507) ^ Math.imul(h2 ^ (h2 >>> 13), 3266489909);
  h2 = Math.imul(h2 ^ (h2 >>> 16), 2246822507) ^ Math.imul(h1 ^ (h1 >>> 13), 3266489909);
  const combined = (h2 >>> 0).toString(16).padStart(8, "0") + (h1 >>> 0).toString(16).padStart(8, "0");
  // Repeat to fill 64 chars
  return (combined + combined + combined + combined).slice(0, 64);
}
