/**
 * Rate limit utility functions for parsing headers and formatting display values.
 */

export interface RateLimitInfo {
  /** Max requests per window (undefined = unknown/unlimited) */
  requestLimit: number | undefined;
  /** Requests remaining in current window */
  requestRemaining: number | undefined;
  /** Unix timestamp (seconds) when the window resets */
  resetTimestamp: number | undefined;
  /** Max tokens per window (undefined = not reported) */
  tokenLimit: number | undefined;
  /** Tokens remaining in current window */
  tokenRemaining: number | undefined;
  /** Whether we have any rate limit data at all */
  hasData: boolean;
}

const RATE_LIMIT_HEADERS = {
  limit: "x-ratelimit-limit",
  remaining: "x-ratelimit-remaining",
  reset: "x-ratelimit-reset",
  tokenLimit: "x-ratelimit-token-limit",
  tokenRemaining: "x-ratelimit-token-remaining",
} as const;

/**
 * Parse rate limit headers from a fetch Response or plain Headers object.
 * Gracefully handles missing headers -- fields will be undefined.
 */
export function parseRateLimitHeaders(headers: Headers): RateLimitInfo {
  const getNum = (key: string): number | undefined => {
    const val = headers.get(key);
    if (val == null || val === "") return undefined;
    const n = Number(val);
    return Number.isFinite(n) ? n : undefined;
  };

  const requestLimit = getNum(RATE_LIMIT_HEADERS.limit);
  const requestRemaining = getNum(RATE_LIMIT_HEADERS.remaining);
  const resetTimestamp = getNum(RATE_LIMIT_HEADERS.reset);
  const tokenLimit = getNum(RATE_LIMIT_HEADERS.tokenLimit);
  const tokenRemaining = getNum(RATE_LIMIT_HEADERS.tokenRemaining);

  const hasData =
    requestLimit !== undefined ||
    requestRemaining !== undefined ||
    resetTimestamp !== undefined ||
    tokenLimit !== undefined ||
    tokenRemaining !== undefined;

  return {
    requestLimit,
    requestRemaining,
    resetTimestamp,
    tokenLimit,
    tokenRemaining,
    hasData,
  };
}

/**
 * Format remaining count, e.g. "142/200"
 */
export function formatRemaining(current: number, max: number): string {
  return `${current}/${max}`;
}

/**
 * Format large numbers compactly: 1200 -> "1.2k", 1200000 -> "1.2M"
 */
function formatCompact(n: number): string {
  if (n >= 1_000_000) return (n / 1_000_000).toFixed(1).replace(/\.0$/, "") + "M";
  if (n >= 1_000) return (n / 1_000).toFixed(1).replace(/\.0$/, "") + "k";
  return String(n);
}

/**
 * Format token remaining display, e.g. "12.4k/50k tokens remaining"
 */
export function formatTokenRemaining(remaining: number, limit: number): string {
  return `${formatCompact(remaining)}/${formatCompact(limit)} tokens remaining`;
}

/**
 * Format reset timestamp as countdown, e.g. "4m 32s"
 * Returns null if timestamp is in the past.
 */
export function formatResetTime(resetTimestamp: number): string | null {
  const now = Math.floor(Date.now() / 1000);
  let diff = resetTimestamp - now;
  if (diff <= 0) return null;

  const h = Math.floor(diff / 3600);
  diff %= 3600;
  const m = Math.floor(diff / 60);
  const s = diff % 60;

  const parts: string[] = [];
  if (h > 0) parts.push(`${h}h`);
  if (m > 0 || h > 0) parts.push(`${m}m`);
  parts.push(`${s}s`);
  return parts.join(" ");
}

/**
 * Get seconds until reset. Returns 0 if already reset or unknown.
 */
export function getSecondsUntilReset(resetTimestamp: number | undefined): number {
  if (resetTimestamp === undefined) return 0;
  const now = Math.floor(Date.now() / 1000);
  const diff = resetTimestamp - now;
  return Math.max(0, diff);
}

/**
 * Calculate percentage remaining (0-100).
 * Returns undefined if limit is unknown.
 */
export function getPercentageRemaining(remaining: number | undefined, limit: number | undefined): number | undefined {
  if (remaining === undefined || limit === undefined || limit === 0) return undefined;
  return Math.min(100, Math.max(0, (remaining / limit) * 100));
}

/**
 * Get Tailwind color classes based on percentage remaining.
 * green > 50%, yellow 20-50%, red < 20%, gray when unknown
 */
export function getLimitColor(percentage: number | undefined): {
  bar: string;
  text: string;
  bg: string;
} {
  if (percentage === undefined) {
    return {
      bar: "bg-surface-300",
      text: "text-surface-800/40",
      bg: "bg-surface-100",
    };
  }
  if (percentage > 50) {
    return {
      bar: "bg-emerald-500",
      text: "text-emerald-600",
      bg: "bg-emerald-50",
    };
  }
  if (percentage > 20) {
    return {
      bar: "bg-amber-500",
      text: "text-amber-600",
      bg: "bg-amber-50",
    };
  }
  return {
    bar: "bg-red-500",
    text: "text-red-600",
    bg: "bg-red-50",
  };
}
