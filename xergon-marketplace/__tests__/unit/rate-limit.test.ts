import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import {
  parseRateLimitHeaders,
  formatRemaining,
  formatTokenRemaining,
  formatResetTime,
  getSecondsUntilReset,
  getPercentageRemaining,
  getLimitColor,
} from "@/lib/utils/rate-limit";

describe("parseRateLimitHeaders", () => {
  it("parses all headers correctly", () => {
    const headers = new Headers({
      "x-ratelimit-limit": "100",
      "x-ratelimit-remaining": "42",
      "x-ratelimit-reset": "1700000000",
      "x-ratelimit-token-limit": "50000",
      "x-ratelimit-token-remaining": "12000",
    });

    const result = parseRateLimitHeaders(headers);
    expect(result.requestLimit).toBe(100);
    expect(result.requestRemaining).toBe(42);
    expect(result.resetTimestamp).toBe(1700000000);
    expect(result.tokenLimit).toBe(50000);
    expect(result.tokenRemaining).toBe(12000);
    expect(result.hasData).toBe(true);
  });

  it("returns undefined for missing headers", () => {
    const headers = new Headers();
    const result = parseRateLimitHeaders(headers);
    expect(result.requestLimit).toBeUndefined();
    expect(result.requestRemaining).toBeUndefined();
    expect(result.hasData).toBe(false);
  });

  it("ignores non-numeric header values", () => {
    const headers = new Headers({
      "x-ratelimit-limit": "abc",
    });
    const result = parseRateLimitHeaders(headers);
    expect(result.requestLimit).toBeUndefined();
    expect(result.hasData).toBe(false);
  });

  it("sets hasData=true when any header is present", () => {
    const headers = new Headers({
      "x-ratelimit-remaining": "50",
    });
    const result = parseRateLimitHeaders(headers);
    expect(result.hasData).toBe(true);
  });
});

describe("formatRemaining", () => {
  it("formats current/max", () => {
    expect(formatRemaining(42, 100)).toBe("42/100");
  });
});

describe("formatTokenRemaining", () => {
  it("formats small numbers without suffix", () => {
    expect(formatTokenRemaining(500, 1000)).toBe("500/1k tokens remaining");
  });

  it("formats thousands with k suffix", () => {
    expect(formatTokenRemaining(1200, 5000)).toBe("1.2k/5k tokens remaining");
  });

  it("formats millions with M suffix", () => {
    expect(formatTokenRemaining(1500000, 5000000)).toBe(
      "1.5M/5M tokens remaining"
    );
  });
});

describe("formatResetTime", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2024-01-01T00:00:00Z"));
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("returns null for past timestamps", () => {
    expect(formatResetTime(1700000000 - 1)).toBeNull();
  });

  it("formats seconds-only countdown", () => {
    const resetAt = Math.floor(Date.now() / 1000) + 45;
    expect(formatResetTime(resetAt)).toBe("45s");
  });

  it("formats minutes and seconds", () => {
    const resetAt = Math.floor(Date.now() / 1000) + 125;
    expect(formatResetTime(resetAt)).toBe("2m 5s");
  });

  it("formats hours, minutes, seconds", () => {
    const resetAt = Math.floor(Date.now() / 1000) + 3665;
    expect(formatResetTime(resetAt)).toBe("1h 1m 5s");
  });
});

describe("getSecondsUntilReset", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2024-01-01T00:00:00Z"));
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("returns 0 for undefined timestamp", () => {
    expect(getSecondsUntilReset(undefined)).toBe(0);
  });

  it("returns 0 for past timestamps", () => {
    expect(getSecondsUntilReset(1000)).toBe(0);
  });

  it("returns positive seconds for future timestamps", () => {
    const resetAt = Math.floor(Date.now() / 1000) + 300;
    expect(getSecondsUntilReset(resetAt)).toBe(300);
  });
});

describe("getPercentageRemaining", () => {
  it("returns percentage", () => {
    expect(getPercentageRemaining(50, 100)).toBe(50);
  });

  it("returns undefined when limit is undefined", () => {
    expect(getPercentageRemaining(50, undefined)).toBeUndefined();
  });

  it("returns undefined when remaining is undefined", () => {
    expect(getPercentageRemaining(undefined, 100)).toBeUndefined();
  });

  it("returns undefined when limit is 0", () => {
    expect(getPercentageRemaining(50, 0)).toBeUndefined();
  });

  it("clamps to 100", () => {
    expect(getPercentageRemaining(150, 100)).toBe(100);
  });

  it("clamps to 0", () => {
    expect(getPercentageRemaining(-10, 100)).toBe(0);
  });
});

describe("getLimitColor", () => {
  it("returns gray for undefined percentage", () => {
    const result = getLimitColor(undefined);
    expect(result.bar).toBe("bg-surface-300");
  });

  it("returns green for >50%", () => {
    const result = getLimitColor(75);
    expect(result.bar).toBe("bg-emerald-500");
    expect(result.text).toBe("text-emerald-600");
  });

  it("returns yellow for 20-50%", () => {
    const result = getLimitColor(35);
    expect(result.bar).toBe("bg-amber-500");
    expect(result.text).toBe("text-amber-600");
  });

  it("returns red for <20%", () => {
    const result = getLimitColor(10);
    expect(result.bar).toBe("bg-red-500");
    expect(result.text).toBe("text-red-600");
  });
});
