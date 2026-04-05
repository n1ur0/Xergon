/**
 * EIP-12 wallet utilities for detecting and connecting to Ergo wallets.
 *
 * Re-exports from the SDK wallet module for marketplace compatibility.
 */

import type { EIP12AuthApi, EIP12ContextApi } from "@/types/ergo-connector";
export type { EIP12AuthApi, EIP12ContextApi } from "@/types/ergo-connector";

/**
 * Check if any Ergo wallet extension is available.
 */
export function hasErgoConnector(): boolean {
  return typeof window !== "undefined" && !!window.ergoConnector;
}

/**
 * Check if a specific wallet is available via ergoConnector.
 */
export function isWalletAvailable(walletName: string): boolean {
  if (!hasErgoConnector()) return false;
  return !!window.ergoConnector![walletName];
}

/**
 * Get a list of available wallet names.
 */
export function getAvailableWallets(): string[] {
  if (!hasErgoConnector()) return [];
  return Object.keys(window.ergoConnector!);
}

/**
 * Connect to a wallet and return its EIP-12 context.
 *
 * @param walletName - The wallet identifier (e.g. "nautilus")
 * @returns The EIP-12 context API
 * @throws Error if wallet not available or user rejects connection
 */
export async function connectWallet(
  walletName: string
): Promise<EIP12ContextApi> {
  const wallet = getWalletApi(walletName);
  const connected = await wallet.connect();

  if (!connected) {
    throw new Error(`Connection to ${walletName} was rejected by the user.`);
  }

  return wallet.getContext();
}

/**
 * Disconnect from a wallet.
 *
 * @param walletName - The wallet identifier
 */
export async function disconnectWallet(walletName: string): Promise<void> {
  const wallet = getWalletApi(walletName);
  await wallet.disconnect();
}

/**
 * Get the EIP-12 auth API for a wallet without connecting.
 * Useful for checking connection status.
 */
export function getWalletApi(walletName: string): EIP12AuthApi {
  const wallet = window.ergoConnector?.[walletName];
  if (!wallet) {
    throw new Error(
      `Wallet "${walletName}" is not available. Make sure the extension is installed.`
    );
  }
  return wallet;
}

/**
 * Get the EIP-12 context for an already-connected wallet.
 */
export async function getWalletContext(
  walletName: string
): Promise<EIP12ContextApi> {
  const wallet = getWalletApi(walletName);
  return wallet.getContext();
}
