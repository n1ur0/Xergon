/**
 * Shared in-memory store for ErgoPay requests.
 * Used across API route handlers (Next.js module boundary workaround).
 *
 * In production, replace with Redis or a database.
 */

import type { StoredErgoPayRequest } from "@/lib/ergopay/types";

const REQUEST_TTL_MS = 10 * 60 * 1000; // 10 minutes

const requests = new Map<string, StoredErgoPayRequest>();

let cleanupTimer: ReturnType<typeof setInterval> | null = null;

function ensureCleanup() {
  if (cleanupTimer) return;
  cleanupTimer = setInterval(() => {
    const now = Date.now();
    for (const [id, req] of requests) {
      if (req.expiresAt < now) {
        req.status = "expired";
      }
    }
  }, 60_000);
  if (cleanupTimer.unref) cleanupTimer.unref();
}

export function getStoredRequest(id: string): StoredErgoPayRequest | undefined {
  const req = requests.get(id);
  if (req && req.expiresAt < Date.now()) {
    req.status = "expired";
  }
  return req;
}

export function setStoredRequest(req: StoredErgoPayRequest): void {
  requests.set(req.id, req);
  ensureCleanup();
}

export function updateStoredRequest(id: string, update: Partial<StoredErgoPayRequest>): boolean {
  const req = requests.get(id);
  if (!req) return false;
  Object.assign(req, update);
  return true;
}
