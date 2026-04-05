/**
 * Centralized API configuration and wallet auth helpers.
 *
 * This module now delegates to the Xergon SDK for core HTTP functionality
 * while maintaining the same config and wallet helpers used throughout
 * the marketplace.
 */

import { XergonClient, XergonError } from '@xergon/sdk';

export const API_BASE = (process.env.NEXT_PUBLIC_API_BASE || 'http://127.0.0.1:9090') + '/v1';
export const RELAY_BASE = API_BASE;

// ── SDK Client singleton ──

/** Shared SDK client instance. Configure auth via authenticate() or setPublicKey(). */
export const sdk = new XergonClient({ baseUrl: API_BASE });

// ── Wallet auth helpers ──

const PK_KEY = "xergon_wallet_pk";
const ADDRESS_KEY = "xergon_wallet_address";

export function getWalletPk(): string | null {
  if (typeof window === "undefined") return null;
  return localStorage.getItem(PK_KEY);
}

export function setWalletPk(pk: string | null) {
  if (typeof window === "undefined") return;
  if (pk) {
    localStorage.setItem(PK_KEY, pk);
  } else {
    localStorage.removeItem(PK_KEY);
  }
}

export function getWalletAddress(): string | null {
  if (typeof window === "undefined") return null;
  return localStorage.getItem(ADDRESS_KEY);
}

export function setWalletAddress(addr: string | null) {
  if (typeof window === "undefined") return;
  if (addr) {
    localStorage.setItem(ADDRESS_KEY, addr);
  } else {
    localStorage.removeItem(ADDRESS_KEY);
  }
}

// ── Re-export SDK types for convenience ──

export type {
  ChatRole,
  ChatMessage,
  ChatCompletionParams,
  ChatCompletionResponse,
  ChatCompletionChunk,
  ChatCompletionUsage,
  Model,
  Provider,
  LeaderboardEntry,
  BalanceResponse,
  GpuListing,
  GpuRental,
  GpuPricingEntry,
  GpuFilters,
  RateGpuParams,
  GpuReputation,
  IncentiveStatus,
  RareModel,
  BridgeChain,
  BridgeInvoice,
  BridgeStatus,
  XergonErrorType,
  XergonErrorBody,
} from '@xergon/sdk';

export { XergonError } from '@xergon/sdk';
