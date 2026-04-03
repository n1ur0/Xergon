/**
 * Centralized API configuration and token helpers.
 *
 * API_BASE and RELAY_BASE live here so that both the ApiClient and the
 * auth store can import them without creating a circular dependency.
 */

export const API_BASE = process.env.NEXT_PUBLIC_API_BASE || 'http://127.0.0.1:8080';
export const RELAY_BASE = API_BASE;

// ── Token helpers ──

const TOKEN_KEY = "xergon_token";

export function getToken(): string | null {
  if (typeof window === "undefined") return null;
  return localStorage.getItem(TOKEN_KEY);
}

export function setToken(token: string | null) {
  if (typeof window === "undefined") return;
  if (token) {
    localStorage.setItem(TOKEN_KEY, token);
  } else {
    localStorage.removeItem(TOKEN_KEY);
  }
}
