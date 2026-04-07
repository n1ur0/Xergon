"use client";

import { useState } from "react";
import { cn } from "@/lib/utils";
import { type LineageNodeData, type OperationType } from "@/components/lineage/LineageNode";
import { OPERATION_CONFIG } from "@/components/lineage/LineageNode";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface LineageDetailProps {
  node: LineageNodeData;
  ancestors: LineageNodeData[];
  descendants: LineageNodeData[];
  onNodeClick?: (nodeId: string) => void;
  onClose?: () => void;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function formatDate(iso: string): string {
  return new Date(iso).toLocaleDateString("en-US", {
    month: "short",
    day: "numeric",
    year: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function LineageDetail({ node, ancestors, descendants, onNodeClick, onClose }: LineageDetailProps) {
  const [showDiff, setShowDiff] = useState(false);
  const config = OPERATION_CONFIG[node.operation];

  return (
    <div className="rounded-xl border border-surface-200 bg-surface-0 overflow-hidden shadow-sm">
      {/* Header */}
      <div className="flex items-center justify-between px-5 py-4 border-b border-surface-100">
        <div>
          <div className="flex items-center gap-2 mb-1">
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
          <h2 className="text-lg font-bold text-surface-900">{node.name}</h2>
          <div className="text-xs font-mono text-surface-800/40 mt-0.5">v{node.version} &middot; {node.id}</div>
        </div>
        {onClose && (
          <button
            onClick={onClose}
            className="text-surface-800/30 hover:text-surface-800/60 transition-colors p-1"
          >
            <svg className="h-5 w-5" fill="none" viewBox="0 0 24 24" strokeWidth={2} stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        )}
      </div>

      <div className="p-5 space-y-5">
        {/* Description */}
        {node.description && (
          <div>
            <h3 className="text-xs font-semibold text-surface-800/50 uppercase tracking-wider mb-2">Description</h3>
            <p className="text-sm text-surface-800/60 leading-relaxed">{node.description}</p>
          </div>
        )}

        {/* Metadata */}
        <div>
          <h3 className="text-xs font-semibold text-surface-800/50 uppercase tracking-wider mb-2">Metadata</h3>
          <div className="grid grid-cols-2 gap-3">
            <div className="rounded-lg bg-surface-50 px-3 py-2">
              <div className="text-[10px] text-surface-800/30">Operation</div>
              <div className="text-sm font-medium text-surface-900 capitalize">{node.operation.replace("_", " ")}</div>
            </div>
            <div className="rounded-lg bg-surface-50 px-3 py-2">
              <div className="text-[10px] text-surface-800/30">Created</div>
              <div className="text-sm font-medium text-surface-900">{formatDate(node.createdAt)}</div>
            </div>
          </div>
        </div>

        {/* Parameters / Config */}
        {node.parameters && Object.keys(node.parameters).length > 0 && (
          <div>
            <div className="flex items-center justify-between mb-2">
              <h3 className="text-xs font-semibold text-surface-800/50 uppercase tracking-wider">Parameters Changed</h3>
              <button
                onClick={() => setShowDiff(!showDiff)}
                className="text-[10px] text-brand-600 hover:text-brand-700 font-medium"
              >
                {showDiff ? "Hide" : "Show"} diff
              </button>
            </div>
            {showDiff ? (
              <div className="rounded-lg bg-slate-950 p-4 text-xs font-mono overflow-x-auto">
                {Object.entries(node.parameters).map(([key, value]) => (
                  <div key={key} className="flex gap-2">
                    <span className="text-red-400">- {key}: old_value</span>
                    <span className="text-emerald-400">+ {key}: {String(value)}</span>
                  </div>
                ))}
              </div>
            ) : (
              <div className="space-y-1.5">
                {Object.entries(node.parameters).map(([key, value]) => (
                  <div key={key} className="flex items-center justify-between rounded-lg bg-surface-50 px-3 py-1.5">
                    <span className="text-xs font-medium text-surface-800/60">{key}</span>
                    <span className="text-xs font-mono text-surface-900">{String(value)}</span>
                  </div>
                ))}
              </div>
            )}
          </div>
        )}

        {/* Parent models */}
        {ancestors.length > 0 && (
          <div>
            <h3 className="text-xs font-semibold text-surface-800/50 uppercase tracking-wider mb-2">
              Parent Models ({ancestors.length})
            </h3>
            <div className="space-y-2">
              {ancestors.map((ancestor) => (
                <button
                  key={ancestor.id}
                  onClick={() => onNodeClick?.(ancestor.id)}
                  className="w-full flex items-center gap-3 rounded-lg border border-surface-200 bg-surface-50 p-3 text-left transition-colors hover:bg-surface-100"
                >
                  <span
                    className={cn(
                      "inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-[10px] font-medium",
                      OPERATION_CONFIG[ancestor.operation].color,
                    )}
                  >
                    {OPERATION_CONFIG[ancestor.operation].icon} {OPERATION_CONFIG[ancestor.operation].label}
                  </span>
                  <div className="flex-1 min-w-0">
                    <div className="text-sm font-medium text-surface-900 truncate">{ancestor.name}</div>
                    <div className="text-[10px] text-surface-800/30 font-mono">v{ancestor.version}</div>
                  </div>
                  <svg className="h-4 w-4 text-surface-800/20 shrink-0" fill="none" viewBox="0 0 24 24" strokeWidth={2} stroke="currentColor">
                    <path strokeLinecap="round" strokeLinejoin="round" d="M8.25 4.5l7.5 7.5-7.5 7.5" />
                  </svg>
                </button>
              ))}
            </div>
          </div>
        )}

        {/* Child models */}
        {descendants.length > 0 && (
          <div>
            <h3 className="text-xs font-semibold text-surface-800/50 uppercase tracking-wider mb-2">
              Child Models ({descendants.length})
            </h3>
            <div className="space-y-2">
              {descendants.map((desc) => (
                <button
                  key={desc.id}
                  onClick={() => onNodeClick?.(desc.id)}
                  className="w-full flex items-center gap-3 rounded-lg border border-surface-200 bg-surface-50 p-3 text-left transition-colors hover:bg-surface-100"
                >
                  <span
                    className={cn(
                      "inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-[10px] font-medium",
                      OPERATION_CONFIG[desc.operation].color,
                    )}
                  >
                    {OPERATION_CONFIG[desc.operation].icon} {OPERATION_CONFIG[desc.operation].label}
                  </span>
                  <div className="flex-1 min-w-0">
                    <div className="text-sm font-medium text-surface-900 truncate">{desc.name}</div>
                    <div className="text-[10px] text-surface-800/30 font-mono">v{desc.version}</div>
                  </div>
                  <svg className="h-4 w-4 text-surface-800/20 shrink-0" fill="none" viewBox="0 0 24 24" strokeWidth={2} stroke="currentColor">
                    <path strokeLinecap="round" strokeLinejoin="round" d="M8.25 4.5l7.5 7.5-7.5 7.5" />
                  </svg>
                </button>
              ))}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
