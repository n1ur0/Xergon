"use client";

import { cn } from "@/lib/utils";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export type OperationType = "base" | "fine_tune" | "merge" | "prune" | "quantize";

export interface LineageNodeData {
  id: string;
  name: string;
  version: string;
  operation: OperationType;
  createdAt: string;
  parentId?: string;
  parentIds?: string[];
  description?: string;
  parameters?: Record<string, string | number | boolean>;
}

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

export const OPERATION_CONFIG: Record<OperationType, { label: string; color: string; icon: string }> = {
  base: { label: "Base", color: "bg-blue-100 text-blue-700 dark:bg-blue-900/30 dark:text-blue-400", icon: "◆" },
  fine_tune: { label: "Fine-tuned", color: "bg-purple-100 text-purple-700 dark:bg-purple-900/30 dark:text-purple-400", icon: "⟳" },
  merge: { label: "Merged", color: "bg-amber-100 text-amber-700 dark:bg-amber-900/30 dark:text-amber-400", icon: "⊕" },
  prune: { label: "Pruned", color: "bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-400", icon: "✂" },
  quantize: { label: "Quantized", color: "bg-emerald-100 text-emerald-700 dark:bg-emerald-900/30 dark:text-emerald-400", icon: "⊞" },
};

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

interface LineageNodeProps {
  node: LineageNodeData;
  isSelected?: boolean;
  onClick?: (nodeId: string) => void;
  compact?: boolean;
}

export function LineageNode({ node, isSelected, onClick, compact }: LineageNodeProps) {
  const config = OPERATION_CONFIG[node.operation];

  return (
    <div
      className={cn(
        "group rounded-xl border bg-surface-0 transition-all cursor-pointer",
        isSelected
          ? "border-brand-400 shadow-md ring-2 ring-brand-100 dark:ring-brand-900/30"
          : "border-surface-200 hover:shadow-sm hover:border-surface-300",
        compact ? "p-3" : "p-4",
      )}
      onClick={() => onClick?.(node.id)}
      role="button"
      tabIndex={0}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          onClick?.(node.id);
        }
      }}
    >
      {/* Operation badge + icon */}
      <div className="flex items-center gap-2 mb-2">
        <span
          className={cn(
            "inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-[10px] font-medium",
            config.color,
          )}
        >
          <span className="text-xs">{config.icon}</span>
          {config.label}
        </span>
      </div>

      {/* Model name */}
      <div className="text-sm font-semibold text-surface-900 group-hover:text-brand-600 transition-colors leading-tight mb-1">
        {node.name}
      </div>

      {/* Version */}
      <div className="text-xs font-mono text-surface-800/40 mb-1">v{node.version}</div>

      {/* Description */}
      {!compact && node.description && (
        <p className="text-xs text-surface-800/40 line-clamp-2 mb-2">{node.description}</p>
      )}

      {/* Date */}
      <div className="text-[10px] text-surface-800/30">
        {new Date(node.createdAt).toLocaleDateString("en-US", {
          month: "short",
          day: "numeric",
          year: "numeric",
        })}
      </div>

      {/* Parent info */}
      {!compact && node.parentIds && node.parentIds.length > 0 && (
        <div className="mt-2 pt-2 border-t border-surface-100">
          <div className="text-[10px] text-surface-800/30">
            Derived from {node.parentIds.length} model{node.parentIds.length > 1 ? "s" : ""}
          </div>
        </div>
      )}
    </div>
  );
}
