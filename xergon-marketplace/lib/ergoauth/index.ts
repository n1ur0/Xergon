/**
 * ErgoAuth (EIP-28) module barrel export.
 *
 * Provides wallet authentication for Ergo wallets that don't support
 * the EIP-12 browser connector API.
 */

export type {
  ErgoAuthRequest,
  ErgoAuthResponse,
  ErgoAuthSession,
  PendingChallenge,
  ErgoAuthWalletType,
  ErgoAuthDeepLink,
} from "./types";

export {
  generateNonce,
  buildSigningMessage,
  addressToSigmaBoolean,
  createErgoAuthRequest,
  buildErgoAuthDeepLink,
  parseErgoAuthDeepLink,
  verifySignedMessage,
  CHALLENGE_TTL_MS,
} from "./challenge";
