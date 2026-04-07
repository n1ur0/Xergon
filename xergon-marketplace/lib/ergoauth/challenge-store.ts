/**
 * In-memory challenge store for ErgoAuth.
 *
 * In production, use Redis or a database. This is fine for single-instance dev.
 */

import type { PendingChallenge } from "./types";
import { CHALLENGE_TTL_MS } from "./challenge";

const pendingChallenges = new Map<string, PendingChallenge>();

/** Prune expired challenges every 60 seconds */
const CLEANUP_INTERVAL_MS = 60_000;

let cleanupTimer: ReturnType<typeof setInterval> | null = null;

function ensureCleanup() {
  if (cleanupTimer) return;
  cleanupTimer = setInterval(() => {
    const now = Date.now();
    for (const [nonce, challenge] of pendingChallenges) {
      if (challenge.expiresAt < now) {
        pendingChallenges.delete(nonce);
      }
    }
  }, CLEANUP_INTERVAL_MS);
  // Allow the Node.js process to exit even if this timer is active
  if (cleanupTimer.unref) cleanupTimer.unref();
}

export function getPendingChallenge(nonce: string): PendingChallenge | undefined {
  return pendingChallenges.get(nonce);
}

export function deletePendingChallenge(nonce: string): void {
  pendingChallenges.delete(nonce);
}

export function setPendingChallenge(challenge: PendingChallenge): void {
  pendingChallenges.set(challenge.nonce, challenge);
  ensureCleanup();
}
