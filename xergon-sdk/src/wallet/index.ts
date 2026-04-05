/**
 * Xergon Wallet module.
 *
 * Re-exports EIP-12 types, Nautilus helpers, and HMAC utilities.
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
