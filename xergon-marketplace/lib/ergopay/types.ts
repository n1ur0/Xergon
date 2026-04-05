/**
 * ErgoPay (EIP-20) types for mobile wallet transactions.
 *
 * ErgoPay protocol: dApp prepares a tx, generates a QR code,
 * the mobile wallet scans it to sign, then POSTs the signed tx
 * back to a replyTo URL.
 */

// ---------------------------------------------------------------------------
// Signing request (what the wallet sees when it fetches the QR URL)
// ---------------------------------------------------------------------------

export interface ErgoPaySigningRequest {
  /** Base16-encoded ReducedTransaction */
  unsignedTx: string;
  /** Fee in nanoERG */
  fee: number;
  /** Total input value in nanoERG */
  inputsTotal: number;
  /** Total output value in nanoERG */
  outputsTotal: number;
  /** Box IDs of data inputs */
  dataInputs: string[];
  /** User-friendly summary for wallet display */
  sendTo?: Array<{ address: string; amount: string }>;
}

// ---------------------------------------------------------------------------
// Wallet callback response
// ---------------------------------------------------------------------------

export interface ErgoPayTransactionSent {
  txId: string;
}

// ---------------------------------------------------------------------------
// Client -> Server request to initiate ErgoPay
// ---------------------------------------------------------------------------

export interface ErgoPayRequest {
  /** User's P2PK address */
  senderAddress: string;
  /** Amount to send in nanoERG */
  amountNanoerg: number;
  /** Target address */
  recipientAddress: string;
  /** Optional tokens to send */
  tokens?: Array<{ tokenId: string; amount: number }>;
}

// ---------------------------------------------------------------------------
// QR code payload (what gets encoded in the QR)
// ---------------------------------------------------------------------------

export interface QrCodeData {
  /** URL to fetch ErgoPaySigningRequest from */
  ergoPayUrl: string;
  /** ergopay:// deep link */
  deepLink: string;
  /** If small enough, inline reduced tx base16 */
  reducedTx?: string;
}

// ---------------------------------------------------------------------------
// Server-side stored request state
// ---------------------------------------------------------------------------

export type ErgoPayStatus = "pending" | "signed" | "submitted" | "expired";

export interface StoredErgoPayRequest {
  id: string;
  request: ErgoPayRequest;
  signingRequest: ErgoPaySigningRequest;
  replyTo: string;
  status: ErgoPayStatus;
  signedTx?: string;
  txId?: string;
  createdAt: number;
  expiresAt: number;
}

// ---------------------------------------------------------------------------
// API response types
// ---------------------------------------------------------------------------

export interface ErgoPayRequestResponse {
  requestId: string;
  signingRequest: ErgoPaySigningRequest;
  qrData: QrCodeData;
}

export interface ErgoPayStatusResponse {
  requestId: string;
  status: ErgoPayStatus;
  txId?: string;
  signedTx?: string;
}
