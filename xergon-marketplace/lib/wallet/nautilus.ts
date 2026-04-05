/**
 * Nautilus wallet integration using EIP-12.
 *
 * Provides high-level helpers for connecting, signing, and querying
 * the Nautilus Ergo browser wallet. Includes retry logic, timeouts,
 * transaction feedback, and connection health checks.
 */

import type {
  EIP12ContextApi,
  ErgoBox,
  SignedTransaction,
  UnsignedTransaction,
} from "@/types/ergo-connector";
import {
  connectWallet,
  disconnectWallet,
  isWalletAvailable,
  getWalletApi,
} from "./eip12";

const WALLET_NAME = "nautilus";

// Module-level cache so callers don't need to reconnect
let _context: EIP12ContextApi | null = null;

// ── Configuration ────────────────────────────────────────────────────────

const CONNECT_MAX_RETRIES = 3;
const CONNECT_RETRY_DELAY_MS = 1_000;
const CONNECT_TIMEOUT_MS = 30_000;
const SIGN_MESSAGE_MAX_RETRIES = 2;
const SIGN_MESSAGE_RETRY_DELAY_MS = 500;

// ── Helpers ──────────────────────────────────────────────────────────────

/** Sleep for the given number of milliseconds. */
function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

/**
 * Wrap a promise with a timeout. Rejects with a TimeoutError if the promise
 * doesn't settle within the given duration.
 */
function withTimeout<T>(promise: Promise<T>, ms: number, label: string): Promise<T> {
  let timer: ReturnType<typeof setTimeout>;
  const timeout = new Promise<never>((_, reject) => {
    timer = setTimeout(
      () => reject(new Error(`${label} timed out after ${ms}ms`)),
      ms
    );
  });
  return Promise.race([promise, timeout]).finally(() => clearTimeout(timer!));
}

/**
 * Retry an async operation up to `maxRetries` times with a delay between attempts.
 * Returns the first successful result, or throws the last error.
 */
async function withRetry<T>(
  fn: () => Promise<T>,
  maxRetries: number,
  delayMs: number,
  label: string
): Promise<T> {
  let lastError: unknown;
  for (let attempt = 0; attempt <= maxRetries; attempt++) {
    try {
      return await fn();
    } catch (err) {
      lastError = err;

      // Don't retry on user rejection
      if (err instanceof Error && /reject|denied|cancelled/i.test(err.message)) {
        throw err;
      }

      if (attempt < maxRetries) {
        await sleep(delayMs);
      }
    }
  }
  throw lastError;
}

// ── Public API ───────────────────────────────────────────────────────────

/**
 * Check if the Nautilus wallet extension is installed and available.
 */
export function isNautilusAvailable(): boolean {
  return isWalletAvailable(WALLET_NAME);
}

/**
 * Connect to Nautilus and return the user's change address.
 *
 * Includes:
 * - Retry up to 3 times with 1s delay between attempts
 * - 30s timeout per attempt
 *
 * @returns The Ergo change address from Nautilus
 * @throws If Nautilus is not installed, user rejects, or timeout
 */
export async function connectNautilus(): Promise<string> {
  const connect = () =>
    withTimeout(connectWallet(WALLET_NAME), CONNECT_TIMEOUT_MS, "Wallet connect");

  _context = await withRetry(connect, CONNECT_MAX_RETRIES, CONNECT_RETRY_DELAY_MS, "Wallet connect");

  const address = await withTimeout(
    _context.get_change_address(),
    CONNECT_TIMEOUT_MS,
    "get_change_address"
  );
  return address;
}

/**
 * Disconnect from Nautilus and clear cached context.
 */
export async function disconnectNautilus(): Promise<void> {
  try {
    await disconnectWallet(WALLET_NAME);
  } catch {
    // Wallet may already be disconnected
  }
  _context = null;
}

/**
 * Check if the Nautilus wallet is currently connected and the cached context
 * is still valid. Uses the EIP-12 isConnected() check.
 */
export async function isNautilusConnected(): Promise<boolean> {
  try {
    if (!_context) return false;
    const wallet = getWalletApi(WALLET_NAME);
    return await wallet.isConnected();
  } catch {
    // If isConnected() throws, the extension is likely gone
    return false;
  }
}

/**
 * Sign an arbitrary message using the Nautilus wallet.
 *
 * @param message - The message string to sign
 * @returns The signature as a hex string
 */
export async function signMessage(
  message: string
): Promise<string> {
  if (!_context) {
    throw new Error("Nautilus is not connected. Call connectNautilus() first.");
  }
  const address = await _context.get_change_address();
  return _context.sign_message(address, message);
}

/**
 * Sign a message with retry logic. Retries up to 2 times with 500ms delay.
 * Handles "wallet busy" errors gracefully.
 *
 * @param message - The message string to sign
 * @returns The signature as a hex string
 */
export async function signMessageWithRetry(
  message: string
): Promise<string> {
  if (!_context) {
    throw new Error("Nautilus is not connected. Call connectNautilus() first.");
  }

  const address = await _context.get_change_address();

  return withRetry(
    () => _context!.sign_message(address, message),
    SIGN_MESSAGE_MAX_RETRIES,
    SIGN_MESSAGE_RETRY_DELAY_MS,
    "sign_message"
  );
}

/**
 * Get the ERG balance from Nautilus (in nanoERG).
 *
 * @returns Balance in nanoERG
 */
export async function getBalanceNanoErg(): Promise<number> {
  if (!_context) {
    throw new Error("Nautilus is not connected. Call connectNautilus() first.");
  }
  return _context.get_balance();
}

/**
 * Get the ERG balance in whole ERG.
 *
 * @returns Balance in ERG (1 ERG = 1e9 nanoERG)
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
    throw new Error("Nautilus is not connected. Call connectNautilus() first.");
  }
  return _context;
}

/**
 * Get UTXOs available for spending from Nautilus.
 */
export async function getUtxos(): Promise<ErgoBox[]> {
  if (!_context) {
    throw new Error("Nautilus is not connected. Call connectNautilus() first.");
  }
  return _context.get_utxos();
}

/**
 * Get UTXOs that are already used in pending transactions.
 */
export async function getUsedUtxos(): Promise<ErgoBox[]> {
  if (!_context) {
    throw new Error("Nautilus is not connected. Call connectNautilus() first.");
  }
  return _context.get_used_utxos();
}

/**
 * Sign an unsigned transaction using Nautilus.
 * This prompts the user to approve the transaction in their wallet.
 *
 * @param tx - The unsigned transaction to sign
 * @returns The signed transaction
 * @throws If Nautilus is not connected or user rejects signing
 */
export async function signTx(tx: UnsignedTransaction): Promise<SignedTransaction> {
  if (!_context) {
    throw new Error("Nautilus is not connected.");
  }
  return _context.sign_tx(tx)
}

/**
 * Submit a signed transaction to the Ergo network via Nautilus.
 *
 * @param tx - The signed transaction to submit
 * @returns The transaction ID (hex string)
 * @throws If Nautilus is not connected or submission fails
 */
export async function submitTx(tx: SignedTransaction): Promise<string> {
  if (!_context) {
    throw new Error("Nautilus is not connected.");
  }
  return _context.submit_tx(tx)
}

/**
 * Sign and submit a transaction in one step.
 * Signs the unsigned tx via the user's wallet, then submits it.
 *
 * @param tx - The unsigned transaction to sign and submit
 * @returns The transaction ID (hex string)
 * @throws If Nautilus is not connected, user rejects, or submission fails
 */
export async function signAndSubmit(tx: UnsignedTransaction): Promise<string> {
  const signed = await signTx(tx)
  return submitTx(signed)
}

// ── Transaction feedback ─────────────────────────────────────────────────

export interface SignTxFeedbackCallbacks {
  /** Called when the wallet has successfully signed the transaction */
  onSigned: (txId: string) => void;
  /** Called when the signed tx has been submitted to the network */
  onSubmitted: (txId: string) => void;
  /** Called if signing or submission fails */
  onError: (error: unknown) => void;
}

/**
 * Sign and submit a transaction with progress callbacks.
 *
 * This provides user feedback at each stage:
 * 1. Waiting for wallet signature (user must approve in Nautilus)
 * 2. Transaction signed successfully
 * 3. Submitting to network
 * 4. Transaction submitted (or error)
 *
 * @param tx - The unsigned transaction
 * @param callbacks - Progress callbacks for each stage
 * @returns The transaction ID (hex string)
 */
export async function signTxWithFeedback(
  tx: UnsignedTransaction,
  callbacks: SignTxFeedbackCallbacks
): Promise<string> {
  if (!_context) {
    const err = new Error("Nautilus is not connected.");
    callbacks.onError(err);
    throw err;
  }

  try {
    // Stage 1: Sign
    const signed = await signTx(tx);

    // Stage 2: Signed successfully
    callbacks.onSigned(signed.id);

    // Stage 3: Submit
    const txId = await submitTx(signed);

    // Stage 4: Submitted successfully
    callbacks.onSubmitted(txId);

    return txId;
  } catch (err) {
    callbacks.onError(err);
    throw err;
  }
}
