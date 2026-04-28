/**
 * Centralized API configuration and wallet auth helpers.
 *
 * This module contains client-side helpers and re-exports the SDK
 * for use in server components.
 */

// This is a client module - wallet helpers use localStorage
"use client";

// Import relay URL from central config (re-exported for client use)
export { API_BASE, RELAY_BASE } from "./server-sdk";

// ── Wallet auth helpers ───────────────────────────────────────────────

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
