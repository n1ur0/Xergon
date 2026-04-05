/**
 * EIP-12 connector utilities for ErgoAuth integration.
 *
 * This module wraps the existing `lib/wallet/eip12.ts` connector and adds
 * ErgoAuth-specific utilities for detecting available wallets and
 * determining which authentication flow to use.
 *
 * Flow selection:
 * - If EIP-12 wallets are detected (Nautilus, SAFEW, etc.) -> use EIP-12 connector
 * - If no EIP-12 wallets -> fall back to ErgoAuth (EIP-28) deep link flow
 */

import {
  getAvailableWallets,
  hasErgoConnector,
  isWalletAvailable,
  connectWallet,
  disconnectWallet,
  getWalletContext,
  getWalletApi,
} from "@/lib/wallet/eip12";
import type { EIP12ContextApi, EIP12AuthApi } from "@/types/ergo-connector";

export type {
  EIP12ContextApi,
  EIP12AuthApi,
} from "@/types/ergo-connector";

export {
  getAvailableWallets,
  hasErgoConnector,
  isWalletAvailable,
  connectWallet,
  disconnectWallet,
  getWalletContext,
  getWalletApi,
};

// ── Wallet metadata ──────────────────────────────────────────────────────

/** Known Ergo wallet extensions and their display info */
export interface WalletInfo {
  /** Internal wallet name (used with ergoConnector) */
  name: string;
  /** Human-readable display name */
  displayName: string;
  /** URL to the wallet's homepage or Chrome Web Store */
  homepage: string;
  /** Emoji icon for the wallet */
  icon: string;
}

/** Registry of known Ergo wallet extensions */
export const KNOWN_WALLETS: WalletInfo[] = [
  {
    name: "nautilus",
    displayName: "Nautilus",
    homepage: "https://nautiluswallet.com/",
    icon: "🐚",
  },
  {
    name: "safew",
    displayName: "SAFEW",
    homepage: "https://safew.io/",
    icon: "🛡️",
  },
  {
    name: "rosen",
    displayName: "Rosen",
    homepage: "https://rosen.tech/",
    icon: "🌹",
  },
  {
    name: "yoroi",
    displayName: "Yoroi",
    homepage: "https://yoroi-wallet.com/#/ergo",
    icon: "🦉",
  },
];

/**
 * Get the WalletInfo for a given wallet name, or null if unknown.
 */
export function getWalletInfo(name: string): WalletInfo | null {
  return KNOWN_WALLETS.find((w) => w.name === name) ?? null;
}

/**
 * Detect which known wallet extensions are installed and available.
 * Returns info objects for each detected wallet.
 */
export function detectAvailableWallets(): WalletInfo[] {
  const available = getAvailableWallets();
  return KNOWN_WALLETS.filter((w) => available.includes(w.name));
}

/**
 * Check if any EIP-12 wallet is available for authentication.
 * Returns true if at least one wallet extension is detected.
 */
export function hasWalletExtension(): boolean {
  return hasErgoConnector() && getAvailableWallets().length > 0;
}

/**
 * Check if a specific wallet is currently connected.
 */
export async function isWalletConnected(walletName: string): Promise<boolean> {
  try {
    const api = getWalletApi(walletName);
    return await api.isConnected();
  } catch {
    return false;
  }
}

/**
 * Connect to a wallet and return both the context and the change address.
 * Convenience wrapper that combines connectWallet + get_change_address.
 */
export async function connectAndGetAddress(
  walletName: string
): Promise<{ context: EIP12ContextApi; address: string }> {
  const context = await connectWallet(walletName);
  const address = await context.get_change_address();
  return { context, address };
}
