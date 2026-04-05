"use client";

import { useMemo } from "react";

interface TokenCounterProps {
  promptTokens?: number;
  completionTokens?: number;
  totalTokens?: number;
  costNanoerg?: number;
  /** Optional model pricing string (nanoerg per token) for cost estimate */
  pricePerTokenNanoerg?: number;
  className?: string;
}

export function TokenCounter({
  promptTokens,
  completionTokens,
  totalTokens,
  costNanoerg,
  pricePerTokenNanoerg,
  className,
}: TokenCounterProps) {
  const tokens = totalTokens ?? (promptTokens ?? 0) + (completionTokens ?? 0);

  const costDisplay = useMemo(() => {
    // Use actual cost from API if available
    if (costNanoerg != null && costNanoerg > 0) {
      return `${(costNanoerg / 1e9).toFixed(6).replace(/0+$/, "").replace(/\.$/, "")} ERG`;
    }
    // Estimate from pricing
    if (pricePerTokenNanoerg && pricePerTokenNanoerg > 0 && tokens > 0) {
      const est = (tokens * pricePerTokenNanoerg) / 1e9;
      return `~${est.toFixed(6).replace(/0+$/, "").replace(/\.$/, "")} ERG`;
    }
    return null;
  }, [costNanoerg, pricePerTokenNanoerg, tokens]);

  if (tokens === 0 && !costDisplay) return null;

  const parts: string[] = [];
  if (promptTokens != null && completionTokens != null) {
    parts.push(`${promptTokens}+${completionTokens}`);
  } else {
    parts.push(`${tokens}`);
  }
  parts.push("tokens");
  if (costDisplay) {
    parts.push(`(${costDisplay})`);
  }

  return (
    <span className={`text-[11px] font-mono text-surface-800/40 ${className ?? ""}`}>
      {parts.join(" ")}
    </span>
  );
}
