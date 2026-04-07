"use client";

import { useState, useEffect, useMemo } from "react";
import { cn } from "@/lib/utils";
import type { ModelInfo } from "@/lib/api/client";

interface ModelCardProps {
  model: ModelInfo;
  isSelected?: boolean;
  onSelect?: (modelId: string) => void;
  onRemove?: (modelId: string) => void;
  compact?: boolean;
  showQuickSelect?: boolean;
}

export function ModelCard({
  model,
  isSelected,
  onSelect,
  onRemove,
  compact = false,
  showQuickSelect = false,
}: ModelCardProps) {
  const [online, setOnline] = useState<boolean | null>(null);
  const [hovered, setHovered] = useState(false);

  // We can't check per-model online status from health endpoint directly,
  // so we infer from model.available flag. Poll periodically if desired.
  useEffect(() => {
    // Use model.available as the initial online indicator
    setOnline(model.available ? true : null);
  }, [model.available]);

  const speedColor = {
    fast: "text-green-600 bg-green-50",
    balanced: "text-amber-600 bg-amber-50",
    slow: "text-red-600 bg-red-50",
  }[model.speed ?? "balanced"];

  const priceDisplay = useMemo(() => {
    if (model.freeTier) return "Free";
    if (model.effectivePriceNanoerg != null && model.effectivePriceNanoerg > 0) {
      const erg = model.effectivePriceNanoerg / 1e9;
      if (erg < 0.000001) return `<${(0.000001).toExponential(1)} ERG/tok`;
      return `${erg.toExponential(2)} ERG/tok`;
    }
    return null;
  }, [model.effectivePriceNanoerg, model.freeTier]);

  return (
    <div
      className={cn(
        "group relative rounded-xl border transition-all duration-150",
        isSelected
          ? "border-brand-300 bg-brand-50/50 shadow-sm ring-1 ring-brand-200"
          : "border-surface-200 bg-surface-0 hover:border-surface-300 hover:shadow-sm",
        compact ? "p-3" : "p-4",
      )}
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
    >
      {/* Online indicator dot */}
      <div className="absolute top-3 right-3">
        <div
          className={cn(
            "h-2 w-2 rounded-full",
            online === true
              ? "bg-green-500 shadow-[0_0_4px_rgba(34,197,94,0.5)]"
              : online === false
                ? "bg-red-400"
                : "bg-surface-300",
          )}
          title={online === true ? "Online" : online === false ? "Offline" : "Unknown"}
        />
      </div>

      {/* Model name */}
      <div className="flex items-start gap-2 pr-4">
        <div className="min-w-0 flex-1">
          <h3
            className={cn(
              "font-medium text-surface-900 truncate",
              compact ? "text-sm" : "text-base",
            )}
          >
            {model.name}
          </h3>
          {!compact && model.description && (
            <p className="mt-1 text-xs text-surface-800/50 line-clamp-2">
              {model.description}
            </p>
          )}
        </div>
      </div>

      {/* Tags and metadata */}
      <div className="mt-3 flex flex-wrap items-center gap-1.5">
        {model.speed && (
          <span
            className={cn(
              "inline-flex items-center rounded-full px-2 py-0.5 text-[10px] font-medium",
              speedColor,
            )}
          >
            {model.speed}
          </span>
        )}
        {model.tags?.map((tag) => (
          <span
            key={tag}
            className="inline-flex items-center rounded-full bg-surface-100 px-2 py-0.5 text-[10px] font-medium text-surface-800/50"
          >
            {tag}
          </span>
        ))}
        {model.freeTier && (
          <span className="inline-flex items-center rounded-full bg-emerald-50 px-2 py-0.5 text-[10px] font-medium text-emerald-600">
            Free
          </span>
        )}
      </div>

      {/* Context window & pricing */}
      {!compact && (
        <div className="mt-3 flex items-center gap-3 text-[11px] text-surface-800/40">
          {model.contextWindow && (
            <span className="flex items-center gap-1">
              <ContextIcon />
              {model.contextWindow >= 1000
                ? `${(model.contextWindow / 1000).toFixed(0)}k`
                : model.contextWindow}{" "}
              ctx
            </span>
          )}
          {model.providerCount != null && model.providerCount > 0 && (
            <span className="flex items-center gap-1">
              <ProviderIcon />
              {model.providerCount} provider{model.providerCount !== 1 ? "s" : ""}
            </span>
          )}
          {priceDisplay && (
            <span className="flex items-center gap-1">
              <PriceIcon />
              {priceDisplay}
            </span>
          )}
        </div>
      )}

      {/* Actions */}
      <div className="mt-3 flex items-center gap-2">
        {showQuickSelect && onSelect && (
          <button
            onClick={() => onSelect(model.id)}
            className={cn(
              "flex-1 rounded-lg px-3 py-1.5 text-xs font-medium transition-colors",
              isSelected
                ? "bg-brand-100 text-brand-700 hover:bg-brand-200"
                : "bg-surface-50 text-surface-800/60 hover:bg-surface-100 hover:text-surface-800/80",
            )}
          >
            {isSelected ? "Selected" : "Select"}
          </button>
        )}
        {onRemove && isSelected && (
          <button
            onClick={() => onRemove(model.id)}
            className="rounded-lg px-2 py-1.5 text-xs text-surface-800/40 hover:bg-red-50 hover:text-red-500 transition-colors"
            title="Remove from comparison"
          >
            <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <path d="M18 6 6 18" />
              <path d="m6 6 12 12" />
            </svg>
          </button>
        )}
      </div>
    </div>
  );
}

// ── Icons ──

function ContextIcon() {
  return (
    <svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
      <rect x="2" y="3" width="20" height="14" rx="2" ry="2" />
      <line x1="8" x2="16" y1="21" y2="21" />
      <line x1="12" x2="12" y1="17" y2="21" />
    </svg>
  );
}

function ProviderIcon() {
  return (
    <svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
      <path d="M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2" />
      <circle cx="9" cy="7" r="4" />
      <path d="M22 21v-2a4 4 0 0 0-3-3.87" />
      <path d="M16 3.13a4 4 0 0 1 0 7.75" />
    </svg>
  );
}

function PriceIcon() {
  return (
    <svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
      <line x1="12" x2="12" y1="2" y2="22" />
      <path d="M17 5H9.5a3.5 3.5 0 0 0 0 7h5a3.5 3.5 0 0 1 0 7H6" />
    </svg>
  );
}


