/**
 * Xergon Wallet module.
 *
 * Re-exports EIP-12 types, Nautilus helpers, HMAC utilities,
 * ErgoPay signing support, and offline wallet utilities.
 */

export type {
  ErgoBoxCandidate,
  ErgoBox,
  ErgoAsset,
  ErgoTransactionInput,
  ErgoDataInput,
  UnsignedTransaction,
  SignedTransaction,
  TokenBalance,
  EIP12ContextApi,
  EIP12AuthApi,
  ErgoConnector,
} from './eip12';

export {
  isNautilusAvailable,
  connectNautilus,
  disconnectNautilus,
  signMessage,
  getBalanceNanoErg,
  getBalance,
  getContext,
  getUtxos,
  getUsedUtxos,
  signTx,
  submitTx,
  signAndSubmit,
} from './nautilus';

export { hmacSign, hmacVerify, buildHmacPayload } from './hmac';

// ── ErgoPay (EIP-20) ──

export type {
  ReducedTransaction,
  ErgoPaySigningRequest,
  ErgoPayResponse,
} from './ergopay';

export {
  generateErgoPayUri,
  generateErgoPayDynamicUri,
  createErgoPaySigningRequest,
  parseErgoPayUri,
  verifyErgoPayResponse,
} from './ergopay';

// ── Offline Wallet Utilities ──

export {
  deriveAddress,
  derivePublicKey,
  generateKeypair,
  signMessage as signMessageOffline,
  verifySignature,
} from './offline';
