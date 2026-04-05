/**
 * Type declarations for the Ergo dApp connector (EIP-12).
 * Injected by wallet extensions like Nautilus.
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
  /** Get the change address for the connected wallet */
  get_change_address: () => Promise<string>;

  /** Get all addresses managed by the wallet */
  get_addresses: () => Promise<string[]>;

  /** Get current height of the Ergo node */
  get_current_height: () => Promise<number>;

  /** Get the full balance of the wallet in nanoERG */
  get_balance: (token_id?: string) => Promise<number>;

  /** Get all UTXOs available to spend */
  get_utxos: (amount?: number, token_id?: string) => Promise<ErgoBox[]>;

  /** Get UTXOs that are already used in a transaction */
  get_used_utxos: () => Promise<ErgoBox[]>;

  /** Get all token balances */
  get_tokens_balance: () => Promise<TokenBalance[]>;

  /** Sign a serialized unsigned transaction */
  sign_tx: (tx: UnsignedTransaction) => Promise<SignedTransaction>;

  /** Submit a signed transaction to the network */
  submit_tx: (tx: SignedTransaction) => Promise<string>;

  /** Sign an arbitrary message */
  sign_message: (address: string, message: string) => Promise<string>;
}

// ── EIP-12 Auth API ──

export interface EIP12AuthApi {
  /** Request connection to the wallet. Returns true if approved. */
  connect: () => Promise<boolean>;

  /** Disconnect from the wallet */
  disconnect: () => Promise<boolean>;

  /** Get the EIP-12 context for interacting with the wallet */
  getContext: () => Promise<EIP12ContextApi>;

  /** Check if the wallet is connected */
  isConnected: () => Promise<boolean>;
}

// ── ErgoConnector (window.ergoConnector) ──

export interface ErgoConnector {
  nautilus?: EIP12AuthApi;
  [walletName: string]: EIP12AuthApi | undefined;
}

// ── Window extension ──

declare global {
  interface Window {
    ergoConnector?: ErgoConnector;
  }
}

export {};
