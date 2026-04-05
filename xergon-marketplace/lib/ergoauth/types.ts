/**
 * Type definitions for the ErgoAuth protocol (EIP-28).
 *
 * ErgoAuth is a challenge-response authentication mechanism for Ergo
 * wallets that don't expose an EIP-12 connector (e.g. mobile wallets).
 */

/** Request sent by the backend for the wallet to sign */
export interface ErgoAuthRequest {
  /** The Ergo address that must sign the challenge */
  address: string;
  /** The human-readable message that was signed */
  signingMessage: string;
  /** Hex-encoded SigmaBoolean (serialized ErgoTree for P2PK) */
  sigmaBoolean: string;
  /** Message shown to the user in their wallet */
  userMessage: string;
  /** Severity level for the wallet UI */
  messageSeverity: "INFORMATION" | "WARNING";
  /** URL the wallet should POST the signed proof to */
  replyTo: string;
}

/** Response sent by the wallet after signing */
export interface ErgoAuthResponse {
  /** Hex-encoded sigma proof bytes */
  proof: string;
  /** The original signingMessage that was signed */
  signedMessage: string;
}

/** Server-side session after successful ErgoAuth verification */
export interface ErgoAuthSession {
  /** Authenticated Ergo address */
  address: string;
  /** Access token (opaque string or JWT) */
  accessToken: string;
  /** Unix timestamp when the token expires */
  expiresAt: number;
}

/** Internal representation of a pending challenge */
export interface PendingChallenge {
  /** Random nonce for this challenge */
  nonce: string;
  /** The Ergo address this challenge is for */
  address: string;
  /** The signing message that was generated */
  signingMessage: string;
  /** The sigmaBoolean for the address */
  sigmaBoolean: string;
  /** When this challenge expires (unix ms) */
  expiresAt: number;
}

/** Wallet type for ErgoAuth connections */
export type ErgoAuthWalletType = "ergoauth";

/**
 * `ergoauth://` deep link structure.
 * Format: ergoauth://?address=...&signingMessage=...&sigmaBoolean=...&userMessage=...&messageSeverity=...&replyTo=...
 */
export interface ErgoAuthDeepLink {
  address: string;
  signingMessage: string;
  sigmaBoolean: string;
  userMessage: string;
  messageSeverity: "INFORMATION" | "WARNING";
  replyTo: string;
}
