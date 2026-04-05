/**
 * EIP-12 wallet connector types and helpers.
 *
 * Provides TypeScript type definitions for the Ergo dApp connector
 * (EIP-12) injected by wallet extensions like Nautilus.
 */

// ── EIP-12 Core Types ──

export interface ErgoBoxCandidate {
  value: number;
  ergoTree: string;
  creationHeight: number;
  assets: ErgoAsset[];
  additionalRegisters: Record<string, string>;
  transactionId: string;
  index: number;
}

export interface ErgoBox {
  boxId: string;
  value: number;
  ergoTree: string;
  creationHeight: number;
  assets: ErgoAsset[];
  additionalRegisters: Record<string, string>;
  transactionId: string;
  index: number;
  spentTransactionId?: string;
}

export interface ErgoAsset {
  tokenId: string;
  amount: number;
  name?: string;
  decimals?: number;
}

export interface ErgoTransactionInput {
  boxId: string;
  spendingProof?: {
    proofBytes: string;
    extension: Record<string, string>;
  };
}

export interface ErgoDataInput {
  boxId: string;
}

export interface UnsignedTransaction {
  id: string;
  inputs: ErgoTransactionInput[];
  dataInputs: ErgoDataInput[];
  outputs: ErgoBoxCandidate[];
}

export interface SignedTransaction {
  id: string;
  inputs: ErgoTransactionInput[];
  dataInputs: ErgoDataInput[];
  outputs: ErgoBoxCandidate[];
  size: number;
}

export interface TokenBalance {
  tokenId: string;
  amount: number;
  name?: string;
  decimals?: number;
}

// ── EIP-12 Context API ──

export interface EIP12ContextApi {
  get_change_address: () => Promise<string>;
  get_addresses: () => Promise<string[]>;
  get_current_height: () => Promise<number>;
  get_balance: (token_id?: string) => Promise<number>;
  get_utxos: (amount?: number, token_id?: string) => Promise<ErgoBox[]>;
  get_used_utxos: () => Promise<ErgoBox[]>;
  get_tokens_balance: () => Promise<TokenBalance[]>;
  sign_tx: (tx: UnsignedTransaction) => Promise<SignedTransaction>;
  submit_tx: (tx: SignedTransaction) => Promise<string>;
  sign_message: (address: string, message: string) => Promise<string>;
}

// ── EIP-12 Auth API ──

export interface EIP12AuthApi {
  connect: () => Promise<boolean>;
  disconnect: () => Promise<boolean>;
  getContext: () => Promise<EIP12ContextApi>;
  isConnected: () => Promise<boolean>;
}

// ── ErgoConnector (window.ergoConnector) ──

export interface ErgoConnector {
  nautilus?: EIP12AuthApi;
  [walletName: string]: EIP12AuthApi | undefined;
}
