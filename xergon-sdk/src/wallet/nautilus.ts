/**
 * Nautilus-specific wallet helpers.
 *
 * High-level wrappers for connecting to, signing with, and querying
 * the Nautilus Ergo browser wallet.
 */

import type { EIP12ContextApi, ErgoBox, SignedTransaction, UnsignedTransaction } from './eip12';

const WALLET_NAME = 'nautilus';

// Module-level cache so callers don't need to reconnect
let _context: EIP12ContextApi | null = null;

/**
 * Check if the Nautilus wallet extension is installed and available.
 */
export function isNautilusAvailable(): boolean {
  if (typeof window === 'undefined') return false;
  const connector = (window as any).ergoConnector;
  return !!connector?.[WALLET_NAME];
}

/**
 * Get the Nautilus auth API.
 */
function getNautilusApi(): import('./eip12').EIP12AuthApi {
  if (typeof window === 'undefined') {
    throw new Error('Window is not available (SSR?).');
  }
  const wallet = (window as any).ergoConnector?.[WALLET_NAME];
  if (!wallet) {
    throw new Error(
      'Nautilus wallet is not available. Install it from https://nautiluswallet.com/',
    );
  }
  return wallet as import('./eip12').EIP12AuthApi;
}

/**
 * Connect to Nautilus and return the user's change address.
 */
export async function connectNautilus(): Promise<string> {
  const wallet = getNautilusApi();
  const connected = await wallet.connect();
  if (!connected) {
    throw new Error('Connection to Nautilus was rejected by the user.');
  }
  _context = await wallet.getContext();
  return _context.get_change_address();
}

/**
 * Disconnect from Nautilus and clear cached context.
 */
export async function disconnectNautilus(): Promise<void> {
  try {
    const wallet = getNautilusApi();
    await wallet.disconnect();
  } catch {
    // Wallet may already be disconnected
  }
  _context = null;
}

/**
 * Sign an arbitrary message using Nautilus.
 */
export async function signMessage(message: string): Promise<string> {
  if (!_context) {
    throw new Error('Nautilus is not connected. Call connectNautilus() first.');
  }
  const address = await _context.get_change_address();
  return _context.sign_message(address, message);
}

/**
 * Get the ERG balance from Nautilus (in nanoERG).
 */
export async function getBalanceNanoErg(): Promise<number> {
  if (!_context) {
    throw new Error('Nautilus is not connected. Call connectNautilus() first.');
  }
  return _context.get_balance();
}

/**
 * Get the ERG balance in whole ERG.
 */
export async function getBalance(): Promise<number> {
  const nano = await getBalanceNanoErg();
  return nano / 1e9;
}

/**
 * Get the cached EIP-12 context, or throw if not connected.
 */
export function getContext(): EIP12ContextApi {
  if (!_context) {
    throw new Error('Nautilus is not connected. Call connectNautilus() first.');
  }
  return _context;
}

/**
 * Get UTXOs available for spending from Nautilus.
 */
export async function getUtxos(): Promise<ErgoBox[]> {
  if (!_context) {
    throw new Error('Nautilus is not connected. Call connectNautilus() first.');
  }
  return _context.get_utxos();
}

/**
 * Get UTXOs that are already used in pending transactions.
 */
export async function getUsedUtxos(): Promise<ErgoBox[]> {
  if (!_context) {
    throw new Error('Nautilus is not connected. Call connectNautilus() first.');
  }
  return _context.get_used_utxos();
}

/**
 * Sign an unsigned transaction using Nautilus.
 */
export async function signTx(tx: UnsignedTransaction): Promise<SignedTransaction> {
  if (!_context) {
    throw new Error('Nautilus is not connected.');
  }
  return _context.sign_tx(tx);
}

/**
 * Submit a signed transaction to the Ergo network via Nautilus.
 */
export async function submitTx(tx: SignedTransaction): Promise<string> {
  if (!_context) {
    throw new Error('Nautilus is not connected.');
  }
  return _context.submit_tx(tx);
}

/**
 * Sign and submit a transaction in one step.
 */
export async function signAndSubmit(tx: UnsignedTransaction): Promise<string> {
  const signed = await signTx(tx);
  return submitTx(signed);
}
