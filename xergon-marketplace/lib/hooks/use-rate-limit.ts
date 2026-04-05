"use client";

import { useState, useEffect, useCallback, useRef } from "react";
import {
  type RateLimitInfo,
  parseRateLimitHeaders,
  getPercentageRemaining,
  getSecondsUntilReset,
} from "@/lib/utils/rate-limit";
import { API_BASE, getWalletPk } from "@/lib/api/config";

const STORAGE_KEY = "xergon_ratelimit_info";
const POLL_INTERVAL = 60_000; // 60s

export interface RateLimitState extends RateLimitInfo {
  /** Percentage remaining (0-100), undefined if unknown */
  percentage: number | undefined;
  /** True when < 20% remaining */
  isNearLimit: boolean;
  /** True when remaining === 0 */
  isLimited: boolean;
  /** Seconds until reset, 0 if unknown or already reset */
  secondsUntilReset: number;
}

const INITIAL_STATE: RateLimitState = {
  requestLimit: undefined,
  requestRemaining: undefined,
  resetTimestamp: undefined,
  tokenLimit: undefined,
  tokenRemaining: undefined,
  hasData: false,
  percentage: undefined,
  isNearLimit: false,
  isLimited: false,
  secondsUntilReset: 0,
};

function computeDerived(info: RateLimitInfo): RateLimitState {
  const percentage = getPercentageRemaining(info.requestRemaining, info.requestLimit);
  const isNearLimit = percentage !== undefined && percentage < 20;
  const isLimited = info.requestRemaining === 0;
  const secondsUntilReset = getSecondsUntilReset(info.resetTimestamp);

  return {
    ...info,
    percentage,
    isNearLimit,
    isLimited,
    secondsUntilReset,
  };
}

function loadFromStorage(): RateLimitState {
  if (typeof window === "undefined") return INITIAL_STATE;
  try {
    const raw = sessionStorage.getItem(STORAGE_KEY);
    if (!raw) return INITIAL_STATE;
    const parsed = JSON.parse(raw) as RateLimitInfo;
    return computeDerived(parsed);
  } catch {
    return INITIAL_STATE;
  }
}

function saveToStorage(info: RateLimitInfo) {
  if (typeof window === "undefined") return;
  try {
    sessionStorage.setItem(STORAGE_KEY, JSON.stringify(info));
  } catch {
    // sessionStorage full or unavailable
  }
}

/**
 * React hook that tracks rate limit state.
 *
 * - Reads X-RateLimit-* headers from API responses
 * - Persists to sessionStorage (survives refresh, not tab close)
 * - Provides derived state: percentage, isNearLimit, isLimited, secondsUntilReset
 * - Auto-polls HEAD /v1/models every 60s when no API calls happen
 */
export function useRateLimit() {
  const [state, setState] = useState<RateLimitState>(INITIAL_STATE);
  const lastActivityRef = useRef<number>(Date.now());
  const mountedRef = useRef(true);

  // Load persisted state on mount
  useEffect(() => {
    mountedRef.current = true;
    const stored = loadFromStorage();
    // Only use stored data if it's not expired (reset time hasn't passed)
    if (stored.hasData) {
      if (stored.resetTimestamp !== undefined) {
        const now = Math.floor(Date.now() / 1000);
        if (stored.resetTimestamp > now) {
          setState(computeDerived(stored));
        }
        // If reset timestamp is in the past, data is stale, start fresh
      } else {
        setState(computeDerived(stored));
      }
    }

    return () => {
      mountedRef.current = false;
    };
  }, []);

  /**
   * Update rate limit info from a fetch Response.
   * Call this after every API request to keep state fresh.
   */
  const updateFromResponse = useCallback((response: Response) => {
    const info = parseRateLimitHeaders(response.headers);
    if (info.hasData) {
      lastActivityRef.current = Date.now();
      saveToStorage(info);
      if (mountedRef.current) {
        setState(computeDerived(info));
      }
    }
  }, []);

  /**
   * Poll HEAD /v1/models to get fresh rate limit headers.
   */
  const pollHeaders = useCallback(async () => {
    try {
      const walletPk = getWalletPk();
      const res = await fetch(`${API_BASE}/models`, {
        method: "HEAD",
        headers: {
          ...(walletPk ? { "X-Wallet-PK": walletPk } : {}),
        },
      });
      const info = parseRateLimitHeaders(res.headers);
      if (info.hasData) {
        saveToStorage(info);
        if (mountedRef.current) {
          setState(computeDerived(info));
        }
      }
    } catch {
      // Silently fail -- polling is best-effort
    }
  }, []);

  // Auto-poll every 60s when no recent API activity
  useEffect(() => {
    const interval = setInterval(() => {
      const elapsed = Date.now() - lastActivityRef.current;
      if (elapsed >= POLL_INTERVAL) {
        pollHeaders();
      }
    }, POLL_INTERVAL);

    return () => clearInterval(interval);
  }, [pollHeaders]);

  // Tick countdown every second for secondsUntilReset
  useEffect(() => {
    const interval = setInterval(() => {
      setState((prev) => {
        if (prev.resetTimestamp === undefined) return prev;
        const seconds = getSecondsUntilReset(prev.resetTimestamp);
        if (seconds === prev.secondsUntilReset) return prev;

        // If timer reached 0, the window has reset -- clear stale data
        if (seconds === 0 && prev.hasData) {
          return INITIAL_STATE;
        }

        return { ...prev, secondsUntilReset: seconds };
      });
    }, 1000);

    return () => clearInterval(interval);
  }, []);

  return {
    ...state,
    updateFromResponse,
  };
}
