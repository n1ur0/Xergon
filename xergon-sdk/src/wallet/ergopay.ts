/**
 * ErgoPay signing support (EIP-20).
 *
 * ErgoPay allows signing transactions via QR codes (mobile wallets) or
 * deeplinks (desktop wallets). Two modes are supported:
 *
 * - Static: `ergopay:<base64url-encoded reduced transaction>` -- the entire
 *   reduced transaction is embedded in the URI (suitable for QR codes).
 * - Dynamic: `ergopay://example.com/api/ergo-pay/request/<requestId>` -- the
 *   dApp hosts a server endpoint that serves the signing request.
 *
 * The placeholder `#P2PK_ADDRESS#` can be included in the URI and will be
 * replaced by the wallet with the user's address before the request is made.
 */

import type { UnsignedTransaction } from './eip12';

// ── ErgoPay Types ──

/**
 * A reduced transaction ready for ErgoPay signing.
 *
 * This is the same shape as UnsignedTransaction but may contain
 * additional fields (e.g. dataInputs) expected by ErgoPay wallets.
 */
export interface ReducedTransaction extends UnsignedTransaction {
  /** Optional message displayed to the user by the wallet */
  message?: string;
}

/**
 * An ErgoPay signing request served by a dynamic endpoint.
 */
export interface ErgoPaySigningRequest {
  /** The reduced transaction to be signed */
  reducedTx: ReducedTransaction;
  /** Optional message displayed to the user by the wallet */
  message?: string;
  /** URL where the wallet should POST the signed transaction */
  replyToUrl?: string;
}

/**
 * Response from a wallet after signing an ErgoPay request.
 */
export interface ErgoPayResponse {
  /** The signed transaction */
  signedTx: {
    id: string;
    inputs: Array<{
      boxId: string;
      spendingProof: {
        proofBytes: string;
        extension: Record<string, string>;
      };
    }>;
    dataInputs: Array<{ boxId: string }>;
    outputs: Array<{
      value: number;
      ergoTree: string;
      creationHeight: number;
      assets: Array<{ tokenId: string; amount: number }>;
      additionalRegisters: Record<string, string>;
      transactionId: string;
      index: number;
    }>;
  };
}

// ── Base64URL Encoding / Decoding ──

/**
 * Encode a string to Base64URL (no padding, URL-safe alphabet).
 */
function base64UrlEncode(input: string): string {
  const base64 = Buffer.from(input, 'utf-8').toString('base64');
  return base64.replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
}

/**
 * Decode a Base64URL string back to a plain string.
 */
function base64UrlDecode(input: string): string {
  let base64 = input.replace(/-/g, '+').replace(/_/g, '/');
  while (base64.length % 4 !== 0) {
    base64 += '=';
  }
  return Buffer.from(base64, 'base64').toString('utf-8');
}

// ── ErgoPay URI Generation ──

/**
 * Generate a static ErgoPay URI with a base64url-encoded reduced transaction.
 *
 * The resulting URI can be rendered as a QR code for mobile wallet signing.
 *
 * @param reducedTx - The reduced transaction to encode
 * @returns A URI string starting with `ergopay:`
 *
 * @example
 * ```ts
 * const uri = generateErgoPayUri({ id: '...', inputs: [], dataInputs: [], outputs: [] });
 * // => "ergopay:eyJpZCI6Ii4uLiJ9"
 * ```
 */
export function generateErgoPayUri(reducedTx: ReducedTransaction): string {
  const json = JSON.stringify(reducedTx);
  const encoded = base64UrlEncode(json);
  return `ergopay:${encoded}`;
}

/**
 * Generate a dynamic ErgoPay URI that points to a server endpoint.
 *
 * The wallet will fetch the signing request from the provided URL.
 * If `params.address` is true, the `#P2PK_ADDRESS#` placeholder will be
 * appended for the wallet to substitute the user's address.
 *
 * @param baseUrl - The server base URL (e.g. `https://example.com`)
 * @param requestId - A unique identifier for this signing request
 * @param params - Optional parameters
 * @returns A URI string starting with `ergopay://`
 *
 * @example
 * ```ts
 * const uri = generateErgoPayDynamicUri('https://example.com', 'req-123', { address: true });
 * // => "ergopay://example.com/api/ergo-pay/request/req-123#P2PK_ADDRESS#"
 * ```
 */
export function generateErgoPayDynamicUri(
  baseUrl: string,
  requestId: string,
  params?: { address?: boolean },
): string {
  // Strip trailing slash from baseUrl
  const cleanBase = baseUrl.replace(/\/+$/, '');
  const uri = `ergopay://${cleanBase.replace(/^https?:\/\//, '')}/api/ergo-pay/request/${requestId}`;
  if (params?.address) {
    return `${uri}#P2PK_ADDRESS#`;
  }
  return uri;
}

// ── ErgoPay Signing Request ──

/**
 * Create an ErgoPay signing request object from an unsigned transaction.
 *
 * This can be served from a dynamic endpoint for wallets to fetch.
 *
 * @param unsignedTx - The unsigned transaction (from buildProviderRegistrationTx, etc.)
 * @param replyToUrl - Optional URL where the wallet should POST the signed result
 * @returns An ErgoPaySigningRequest object ready to be served as JSON
 *
 * @example
 * ```ts
 * const request = createErgoPaySigningRequest(unsignedTx, 'https://example.com/callback');
 * // Serve this as JSON from your dynamic endpoint
 * ```
 */
export function createErgoPaySigningRequest(
  unsignedTx: UnsignedTransaction,
  replyToUrl?: string,
): ErgoPaySigningRequest {
  const reducedTx: ReducedTransaction = {
    ...unsignedTx,
  };

  const request: ErgoPaySigningRequest = {
    reducedTx,
  };

  if (replyToUrl) {
    request.replyToUrl = replyToUrl;
  }

  return request;
}

// ── ErgoPay URI Parsing ──

/**
 * Parse an ErgoPay URI and determine if it is static or dynamic.
 *
 * - Static URIs contain the reduced transaction as base64url data.
 * - Dynamic URIs contain a server URL to fetch the signing request from.
 *
 * @param uri - The ErgoPay URI to parse
 * @returns An object with the type and decoded data
 * @throws Error if the URI format is invalid
 *
 * @example
 * ```ts
 * const parsed = parseErgoPayUri('ergopay:eyJpZCI6Ii4uLiJ9');
 * // => { type: 'static', data: { id: '...' } }
 * ```
 */
export function parseErgoPayUri(uri: string): { type: 'static' | 'dynamic'; data: any } {
  if (!uri.startsWith('ergopay:')) {
    throw new Error('Invalid ErgoPay URI: must start with "ergopay:"');
  }

  // Dynamic URIs use ergopay:// (with double slash)
  if (uri.startsWith('ergopay://')) {
    const urlPart = uri.slice('ergopay://'.length);
    return {
      type: 'dynamic',
      data: {
        url: `https://${urlPart}`,
      },
    };
  }

  // Static URIs use ergopay: (single colon, no double slash)
  const encoded = uri.slice('ergopay:'.length);
  if (!encoded) {
    throw new Error('Invalid ErgoPay URI: no data after "ergopay:"');
  }

  try {
    const json = base64UrlDecode(encoded);
    const data = JSON.parse(json);
    return { type: 'static', data };
  } catch (e) {
    throw new Error(`Invalid ErgoPay URI: failed to decode base64url data: ${e}`);
  }
}

// ── ErgoPay Response Validation ──

/**
 * Perform basic validation of an ErgoPay response from a wallet.
 *
 * Checks that the response has a signedTx with the required fields.
 * This does NOT verify the cryptographic proofs -- that should be done
 * by the Ergo node when submitting the transaction.
 *
 * @param response - The ErgoPay response object to validate
 * @returns true if the response structure is valid
 *
 * @example
 * ```ts
 * const isValid = verifyErgoPayResponse(walletResponse);
 * if (isValid) {
 *   // Submit signedTx to the Ergo node
 * }
 * ```
 */
export function verifyErgoPayResponse(response: ErgoPayResponse): boolean {
  if (!response || typeof response !== 'object') {
    return false;
  }

  const { signedTx } = response;
  if (!signedTx || typeof signedTx !== 'object') {
    return false;
  }

  // Must have a valid transaction ID (hex string)
  if (typeof signedTx.id !== 'string' || signedTx.id.length === 0) {
    return false;
  }

  // Must have at least one input with spending proof
  if (!Array.isArray(signedTx.inputs) || signedTx.inputs.length === 0) {
    return false;
  }

  for (const input of signedTx.inputs) {
    if (typeof input.boxId !== 'string' || !input.spendingProof) {
      return false;
    }
    const { proofBytes, extension } = input.spendingProof;
    if (typeof proofBytes !== 'string') {
      return false;
    }
    if (typeof extension !== 'object' || extension === null) {
      return false;
    }
  }

  // Must have outputs
  if (!Array.isArray(signedTx.outputs) || signedTx.outputs.length === 0) {
    return false;
  }

  for (const output of signedTx.outputs) {
    if (
      typeof output.value !== 'number' ||
      typeof output.ergoTree !== 'string' ||
      typeof output.creationHeight !== 'number'
    ) {
      return false;
    }
  }

  // dataInputs is optional but must be an array if present
  if (signedTx.dataInputs !== undefined && !Array.isArray(signedTx.dataInputs)) {
    return false;
  }

  return true;
}
