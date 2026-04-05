"use client";

import { useState, useEffect, useRef } from "react";
import { useRateLimit, type RateLimitState } from "@/lib/hooks/use-rate-limit";
import {
  formatRemaining,
  formatTokenRemaining,
  formatResetTime,
  getLimitColor,
  getPercentageRemaining,
} from "@/lib/utils/rate-limit";
import { cn } from "@/lib/utils";

interface RateLimitIndicatorProps {
  /** Use compact mode: icon + percentage only (for narrow screens) */
  compact?: boolean;
  /** Callback to expose rate limit state to parent */
  onStateChange?: (state: RateLimitState) => void;
}

interface RateLimitIndicatorComponent {
  (props: RateLimitIndicatorProps): React.ReactElement | null;
  /** Static reference for feeding API responses */
  _updateRef: ((response: Response) => void) | undefined;
}

/**
 * Gauge icon SVG (clock-like rate icon).
 */
function RateIcon({ className }: { className?: string }) {
  return (
    <svg
      xmlns="http://www.w3.org/2000/svg"
      width="14"
      height="14"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      className={className}
    >
      <path d="M12 2a3 3 0 0 0-3 3v7a3 3 0 0 0 6 0V5a3 3 0 0 0-3-3Z" />
      <path d="M19 10v2a7 7 0 0 1-14 0v-2" />
      <line x1="12" x2="12" y1="19" y2="22" />
    </svg>
  );
}

/**
 * Live countdown display.
 */
function ResetCountdown({ seconds }: { seconds: number }) {
  const [countdown, setCountdown] = useState<string | null>(null);

  useEffect(() => {
    if (seconds <= 0) {
      setCountdown(null);
      return;
    }

    // Compute from the raw seconds (which ticks via the hook)
    const h = Math.floor(seconds / 3600);
    let rem = seconds % 3600;
    const m = Math.floor(rem / 60);
    const s = rem % 60;

    const parts: string[] = [];
    if (h > 0) parts.push(`${h}h`);
    if (m > 0 || h > 0) parts.push(`${m}m`);
    parts.push(`${s}s`);
    setCountdown(parts.join(" "));
  }, [seconds]);

  if (!countdown) return null;

  return (
    <span className="text-[10px] text-surface-800/40">
      Resets in {countdown}
    </span>
  );
}

export const RateLimitIndicator: RateLimitIndicatorComponent = function RateLimitIndicator({ compact = false, onStateChange }) {
  const { updateFromResponse, ...state } = useRateLimit();
  const prevPctRef = useRef<number | undefined>(undefined);
  const [tooltipVisible, setTooltipVisible] = useState(false);

  // Notify parent of state changes
  useEffect(() => {
    onStateChange?.(state);
  }, [state, onStateChange]);

  // Expose updateFromResponse so parent can feed API responses
  useEffect(() => {
    RateLimitIndicator._updateRef = updateFromResponse;
    return () => {
      RateLimitIndicator._updateRef = undefined;
    };
  }, [updateFromResponse]);

  const pct = getPercentageRemaining(state.requestRemaining, state.requestLimit);
  const colors = getLimitColor(pct);
  const isCritical = pct !== undefined && pct < 5;
  const isUnknown = !state.hasData;

  // Trigger animation class when percentage changes
  const [animating, setAnimating] = useState(false);
  useEffect(() => {
    if (prevPctRef.current !== undefined && pct !== undefined && prevPctRef.current !== pct) {
      setAnimating(true);
      const t = setTimeout(() => setAnimating(false), 600);
      return () => clearTimeout(t);
    }
    prevPctRef.current = pct;
  }, [pct]);

  // ── Compact mode: just icon + percentage ──
  if (compact) {
    return (
      <div className="relative group">
        <div
          className={cn(
            "flex items-center gap-1 rounded-md px-1.5 py-0.5 transition-colors",
            isUnknown ? "text-surface-800/30" : colors.text,
          )}
        >
          <RateIcon />
          {isUnknown ? (
            <span className="text-[10px]">--</span>
          ) : pct !== undefined ? (
            <span className="text-[10px] font-medium">{Math.round(pct)}%</span>
          ) : (
            <span className="text-[10px]">Unlimited</span>
          )}
        </div>

        {/* Tooltip on hover */}
        <div
          className={cn(
            "absolute bottom-full left-1/2 -translate-x-1/2 mb-2 px-3 py-2 rounded-lg bg-surface-900 text-surface-0 text-xs whitespace-nowrap shadow-lg",
            "opacity-0 pointer-events-none group-hover:opacity-100 transition-opacity z-50",
          )}
        >
          <RateLimitTooltipContent state={state} pct={pct} />
        </div>
      </div>
    );
  }

  // ── Full mode ──
  return (
    <div className="relative group">
      <div className={cn("flex items-center gap-2 rounded-lg px-2.5 py-1.5 transition-colors", isUnknown ? "bg-surface-50" : colors.bg)}>
        {/* Progress bar */}
        <div className="flex items-center gap-1.5 min-w-0 flex-1">
          <RateIcon className={cn("shrink-0", isUnknown ? "text-surface-800/25" : colors.text)} />

          {isUnknown ? (
            <span className="text-[11px] text-surface-800/40">Checking...</span>
          ) : (
            <>
              {/* Request info */}
              {state.requestRemaining !== undefined && state.requestLimit !== undefined ? (
                <span className={cn("text-[11px] font-medium truncate", colors.text)}>
                  {formatRemaining(state.requestRemaining, state.requestLimit)} requests
                </span>
              ) : (
                <span className="text-[11px] text-surface-800/40">Unlimited</span>
              )}

              {/* Mini progress bar */}
              {pct !== undefined && (
                <div className="flex-1 min-w-[40px] max-w-[80px] h-1.5 rounded-full bg-surface-200/60 overflow-hidden">
                  <div
                    className={cn(
                      "h-full rounded-full transition-all duration-500 ease-out",
                      colors.bar,
                      animating && "animate-pulse",
                      isCritical && "animate-pulse shadow-[0_0_6px_rgba(239,68,68,0.5)]",
                    )}
                    style={{ width: `${pct}%` }}
                  />
                </div>
              )}
            </>
          )}
        </div>

        {/* Token info (if available) */}
        {state.tokenRemaining !== undefined && state.tokenLimit !== undefined && (
          <span className="text-[10px] text-surface-800/35 hidden sm:inline">
            {formatTokenRemaining(state.tokenRemaining, state.tokenLimit)}
          </span>
        )}

        {/* Reset countdown */}
        {state.secondsUntilReset > 0 && (
          <ResetCountdown seconds={state.secondsUntilReset} />
        )}
      </div>

      {/* Critical glow */}
      {isCritical && (
        <div className="absolute inset-0 rounded-lg bg-red-500/10 animate-pulse pointer-events-none" />
      )}

      {/* Tooltip on hover */}
      <div
        className={cn(
          "absolute bottom-full left-1/2 -translate-x-1/2 mb-2 px-3 py-2 rounded-lg bg-surface-900 text-surface-0 text-xs whitespace-nowrap shadow-lg",
          "opacity-0 pointer-events-none group-hover:opacity-100 transition-opacity z-50",
        )}
        onMouseEnter={() => setTooltipVisible(true)}
        onMouseLeave={() => setTooltipVisible(false)}
      >
        <RateLimitTooltipContent state={state} pct={pct} />
      </div>
    </div>
  );
}

function RateLimitTooltipContent({
  state,
  pct,
}: {
  state: RateLimitState;
  pct: number | undefined;
}) {
  if (!state.hasData) {
    return <span className="text-surface-400">No rate limit data available</span>;
  }

  return (
    <div className="space-y-1">
      <div className="font-medium text-surface-100">Rate Limit Status</div>
      {state.requestRemaining !== undefined && state.requestLimit !== undefined && (
        <div className="flex justify-between gap-4">
          <span className="text-surface-400">Requests</span>
          <span className="text-surface-200">
            {state.requestRemaining}/{state.requestLimit}
            {pct !== undefined && ` (${Math.round(pct)}%)`}
          </span>
        </div>
      )}
      {state.tokenRemaining !== undefined && state.tokenLimit !== undefined && (
        <div className="flex justify-between gap-4">
          <span className="text-surface-400">Tokens</span>
          <span className="text-surface-200">
            {state.tokenRemaining.toLocaleString()}/{state.tokenLimit.toLocaleString()}
          </span>
        </div>
      )}
      {state.resetTimestamp !== undefined && (
        <div className="flex justify-between gap-4">
          <span className="text-surface-400">Resets</span>
          <span className="text-surface-200">
            {formatResetTime(state.resetTimestamp) ?? "Now"}
          </span>
        </div>
      )}
      {state.isLimited && (
        <div className="text-red-400 font-medium pt-1">Rate limit reached</div>
      )}
    </div>
  );
}

/**
 * Static reference for parents to call updateFromResponse without ref forwarding.
 */
RateLimitIndicator._updateRef = undefined;

export default RateLimitIndicator;
