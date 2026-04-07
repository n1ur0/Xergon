"use client";

import { useState, useEffect, useCallback, useRef } from "react";
import { cn } from "@/lib/utils";
import { LineageNode, type LineageNodeData, type OperationType } from "@/components/lineage/LineageNode";
import { LineageDetail } from "@/components/lineage/LineageDetail";
import { ErrorBoundary } from "@/components/ui/ErrorBoundary";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface LineageEdge {
  from: string;
  to: string;
  type: "fine_tune" | "merge" | "prune" | "quantize" | "base";
}

interface LineageTree {
  nodes: LineageNodeData[];
  edges: LineageEdge[];
}

// ---------------------------------------------------------------------------
// Skeleton
// ---------------------------------------------------------------------------

function SkeletonPulse({ className }: { className?: string }) {
  return <div className={`skeleton-shimmer rounded-lg ${className ?? ""}`} />;
}

// ---------------------------------------------------------------------------
// SVG Lineage Graph
// ---------------------------------------------------------------------------

function LineageGraphSVG({
  nodes,
  edges,
  selectedId,
  onNodeClick,
  scale,
  offset,
}: {
  nodes: LineageNodeData[];
  edges: LineageEdge[];
  selectedId: string | null;
  onNodeClick: (id: string) => void;
  scale: number;
  offset: { x: number; y: number };
}) {
  // Layout nodes in a tree-like structure
  const NODE_W = 200;
  const NODE_H = 80;
  const H_GAP = 40;
  const V_GAP = 60;

  // Build adjacency: find root nodes (no incoming edges)
  const childIds = new Set(edges.map((e) => e.to));
  const rootNodes = nodes.filter((n) => !childIds.has(n.id));

  // Build children map
  const childrenMap = new Map<string, string[]>();
  for (const edge of edges) {
    const existing = childrenMap.get(edge.from) ?? [];
    existing.push(edge.to);
    childrenMap.set(edge.from, existing);
  }

  // Simple tree layout
  const positions = new Map<string, { x: number; y: number }>();

  function layoutTree(nodeIds: string[], depth: number, startX: number): number {
    let currentX = startX;
    for (const nodeId of nodeIds) {
      const children = childrenMap.get(nodeId) ?? [];
      if (children.length > 0) {
        currentX = layoutTree(children, depth + 1, currentX);
      }
      positions.set(nodeId, { x: currentX, y: depth * (NODE_H + V_GAP) });
      currentX += NODE_W + H_GAP;
    }
    return currentX;
  }

  // If no roots, just lay out all nodes
  if (rootNodes.length > 0) {
    layoutTree(rootNodes.map((n) => n.id), 0, 0);
  } else {
    nodes.forEach((n, i) => {
      positions.set(n.id, { x: i * (NODE_W + H_GAP), y: 0 });
    });
  }

  // Ensure all nodes have positions (handles orphan nodes from merges)
  let maxX = 0;
  positions.forEach((pos) => {
    if (pos.x + NODE_W > maxX) maxX = pos.x + NODE_W;
  });
  for (const node of nodes) {
    if (!positions.has(node.id)) {
      maxX += H_GAP;
      positions.set(node.id, { x: maxX, y: 0 });
    }
  }

  // Compute viewBox bounds
  let minX = Infinity, minY = Infinity, maxXX = -Infinity, maxY = -Infinity;
  positions.forEach((pos) => {
    if (pos.x < minX) minX = pos.x;
    if (pos.y < minY) minY = pos.y;
    if (pos.x + NODE_W > maxXX) maxXX = pos.x + NODE_W;
    if (pos.y + NODE_H > maxY) maxY = pos.y + NODE_H;
  });
  const padding = 60;
  minX -= padding;
  minY -= padding;
  maxXX += padding;
  maxY += padding;

  const edgeColors: Record<string, string> = {
    fine_tune: "#a855f7",
    merge: "#f59e0b",
    prune: "#ef4444",
    quantize: "#10b981",
    base: "#3b82f6",
  };

  return (
    <svg
      className="w-full h-full"
      viewBox={`${minX} ${minY} ${maxXX - minX} ${maxY - minY}`}
      style={{
        transform: `scale(${scale}) translate(${offset.x}px, ${offset.y}px)`,
      }}
    >
      {/* Edges */}
      {edges.map((edge, i) => {
        const from = positions.get(edge.from);
        const to = positions.get(edge.to);
        if (!from || !to) return null;

        const fromX = from.x + NODE_W / 2;
        const fromY = from.y + NODE_H;
        const toX = to.x + NODE_W / 2;
        const toY = to.y;
        const midY = (fromY + toY) / 2;

        return (
          <path
            key={i}
            d={`M ${fromX} ${fromY} C ${fromX} ${midY}, ${toX} ${midY}, ${toX} ${toY}`}
            fill="none"
            stroke={edgeColors[edge.type] ?? "#94a3b8"}
            strokeWidth={2}
            strokeOpacity={0.5}
          />
        );
      })}

      {/* Nodes */}
      {nodes.map((node) => {
        const pos = positions.get(node.id);
        if (!pos) return null;
        const isSelected = node.id === selectedId;

        const opColors: Record<OperationType, string> = {
          base: "#3b82f6",
          fine_tune: "#a855f7",
          merge: "#f59e0b",
          prune: "#ef4444",
          quantize: "#10b981",
        };

        return (
          <g
            key={node.id}
            onClick={() => onNodeClick(node.id)}
            className="cursor-pointer"
            role="button"
          >
            {/* Node background */}
            <rect
              x={pos.x}
              y={pos.y}
              width={NODE_W}
              height={NODE_H}
              rx={12}
              fill="white"
              stroke={isSelected ? "#6366f1" : "#e2e8f0"}
              strokeWidth={isSelected ? 2 : 1}
              className="transition-all"
            />
            {/* Top color bar */}
            <rect
              x={pos.x}
              y={pos.y}
              width={NODE_W}
              height={4}
              rx={2}
              fill={opColors[node.operation]}
            />
            {/* Model name */}
            <text
              x={pos.x + 12}
              y={pos.y + 28}
              className="text-sm font-semibold fill-gray-900"
              fontSize={13}
              fontWeight={600}
            >
              {node.name.length > 20 ? node.name.slice(0, 20) + "..." : node.name}
            </text>
            {/* Version + operation */}
            <text
              x={pos.x + 12}
              y={pos.y + 44}
              className="text-xs fill-gray-400"
              fontSize={10}
            >
              v{node.version} &middot; {node.operation.replace("_", " ")}
            </text>
            {/* Date */}
            <text
              x={pos.x + 12}
              y={pos.y + 60}
              className="text-[10px] fill-gray-300"
              fontSize={9}
            >
              {new Date(node.createdAt).toLocaleDateString("en-US", { month: "short", day: "numeric" })}
            </text>
          </g>
        );
      })}
    </svg>
  );
}

// ---------------------------------------------------------------------------
// Main Component
// ---------------------------------------------------------------------------

interface ModelLineageGraphProps {
  modelId: string;
}

export function ModelLineageGraph({ modelId }: ModelLineageGraphProps) {
  const [tree, setTree] = useState<LineageTree | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(modelId);
  const [ancestors, setAncestors] = useState<LineageNodeData[]>([]);
  const [descendants, setDescendants] = useState<LineageNodeData[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [scale, setScale] = useState(0.85);
  const [filterOp, setFilterOp] = useState<OperationType | "all">("all");
  const [showDetail, setShowDetail] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);

  const loadLineage = useCallback(async (id: string) => {
    try {
      setError(null);
      setIsLoading(true);
      const res = await fetch(`/api/lineage?modelId=${encodeURIComponent(id)}`);
      if (!res.ok) throw new Error("Failed to load lineage data");
      const json = await res.json();
      setTree(json);

      // Also load ancestors and descendants
      const [ancRes, descRes] = await Promise.all([
        fetch(`/api/lineage?modelId=${encodeURIComponent(id)}&section=ancestors`),
        fetch(`/api/lineage?modelId=${encodeURIComponent(id)}&section=descendants`),
      ]);
      if (ancRes.ok) setAncestors(await ancRes.json());
      if (descRes.ok) setDescendants(await descRes.json());
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to load lineage");
    } finally {
      setIsLoading(false);
    }
  }, []);

  useEffect(() => {
    loadLineage(modelId);
  }, [modelId, loadLineage]);

  const handleNodeClick = (nodeId: string) => {
    setSelectedId(nodeId);
    setShowDetail(true);
    // Reload ancestors/descendants for new node
    Promise.all([
      fetch(`/api/lineage?modelId=${encodeURIComponent(nodeId)}&section=ancestors`).then((r) => r.ok ? r.json() : []),
      fetch(`/api/lineage?modelId=${encodeURIComponent(nodeId)}&section=descendants`).then((r) => r.ok ? r.json() : []),
    ]).then(([anc, desc]) => {
      setAncestors(anc);
      setDescendants(desc);
    });
  };

  const filteredNodes = tree?.nodes.filter((n) =>
    filterOp === "all" || n.operation === filterOp,
  ) ?? [];

  const filteredEdges = tree?.edges.filter((e) => {
    if (filterOp === "all") return true;
    return e.type === filterOp;
  }) ?? [];

  const selectedNode = tree?.nodes.find((n) => n.id === selectedId);

  return (
    <div className="space-y-6">
      {/* Toolbar */}
      <div className="flex flex-wrap items-center gap-3">
        {/* Operation filter */}
        <div className="flex items-center gap-1 rounded-lg border border-surface-200 bg-surface-50 p-1">
          {(["all", "base", "fine_tune", "merge", "prune", "quantize"] as const).map((op) => (
            <button
              key={op}
              onClick={() => setFilterOp(op)}
              className={cn(
                "rounded-md px-2 py-1 text-[10px] font-medium transition-colors capitalize",
                filterOp === op
                  ? "bg-surface-0 text-surface-900 shadow-sm"
                  : "text-surface-800/50 hover:text-surface-900",
              )}
            >
              {op === "fine_tune" ? "Fine-tune" : op}
            </button>
          ))}
        </div>

        {/* Zoom controls */}
        <div className="flex items-center gap-1">
          <button
            onClick={() => setScale((s) => Math.min(s + 0.1, 2))}
            className="rounded-lg border border-surface-200 bg-surface-50 p-1.5 text-surface-800/50 hover:text-surface-900 transition-colors"
          >
            <svg className="h-4 w-4" fill="none" viewBox="0 0 24 24" strokeWidth={2} stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" d="M12 4.5v15m7.5-7.5h-15" />
            </svg>
          </button>
          <button
            onClick={() => setScale((s) => Math.max(s - 0.1, 0.3))}
            className="rounded-lg border border-surface-200 bg-surface-50 p-1.5 text-surface-800/50 hover:text-surface-900 transition-colors"
          >
            <svg className="h-4 w-4" fill="none" viewBox="0 0 24 24" strokeWidth={2} stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" d="M19.5 12h-15" />
            </svg>
          </button>
          <button
            onClick={() => setScale(0.85)}
            className="rounded-lg border border-surface-200 bg-surface-50 px-2 py-1.5 text-[10px] font-medium text-surface-800/50 hover:text-surface-900 transition-colors"
          >
            Reset
          </button>
        </div>
      </div>

      {/* Error */}
      {error && (
        <div className="rounded-lg border border-red-200 bg-red-50 dark:border-red-800/40 dark:bg-red-950/20 px-4 py-3 text-sm text-red-600 dark:text-red-400">
          {error}
        </div>
      )}

      {/* Graph */}
      <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
        <div className="lg:col-span-2">
          {isLoading ? (
            <div className="rounded-xl border border-surface-200 bg-surface-0 p-5">
              <SkeletonPulse className="h-[500px] w-full" />
            </div>
          ) : tree ? (
            <ErrorBoundary context="Lineage Graph">
              <div
                ref={containerRef}
                className="rounded-xl border border-surface-200 bg-surface-0 shadow-sm overflow-auto"
                style={{ minHeight: 500 }}
              >
                <div className="p-4">
                  <LineageGraphSVG
                    nodes={filteredNodes}
                    edges={filteredEdges}
                    selectedId={selectedId}
                    onNodeClick={handleNodeClick}
                    scale={scale}
                    offset={{ x: 0, y: 0 }}
                  />
                </div>
              </div>
            </ErrorBoundary>
          ) : null}
        </div>

        {/* Detail panel */}
        <div>
          {selectedNode && showDetail ? (
            <ErrorBoundary context="Lineage Detail">
              <LineageDetail
                node={selectedNode}
                ancestors={ancestors}
                descendants={descendants}
                onNodeClick={handleNodeClick}
                onClose={() => {
                  setShowDetail(false);
                  setSelectedId(modelId);
                }}
              />
            </ErrorBoundary>
          ) : isLoading ? (
            <div className="rounded-xl border border-surface-200 bg-surface-0 p-5">
              <SkeletonPulse className="h-[300px] w-full" />
            </div>
          ) : (
            <div className="rounded-xl border border-surface-200 bg-surface-0 p-8 text-center">
              <svg className="h-10 w-10 mx-auto text-surface-800/10 mb-3" fill="none" viewBox="0 0 24 24" strokeWidth={1.5} stroke="currentColor">
                <path strokeLinecap="round" strokeLinejoin="round" d="M13.5 16.875h3.375m0 0h3.375m-3.375 0V13.5m0 3.375v3.375M6 10.5h2.25a2.25 2.25 0 002.25-2.25V6a2.25 2.25 0 00-2.25-2.25H6A2.25 2.25 0 003.75 6v2.25A2.25 2.25 0 006 10.5zm0 9.75h2.25A2.25 2.25 0 0010.5 18v-2.25a2.25 2.25 0 00-2.25-2.25H6a2.25 2.25 0 00-2.25 2.25V18A2.25 2.25 0 006 20.25zm9.75-9.75H18a2.25 2.25 0 002.25-2.25V6A2.25 2.25 0 0018 3.75h-2.25A2.25 2.25 0 0013.5 6v2.25a2.25 2.25 0 002.25 2.25z" />
              </svg>
              <p className="text-sm text-surface-800/40">
                Click a node in the graph to see details
              </p>
            </div>
          )}
        </div>
      </div>

      {/* Node list (alternative view) */}
      {tree && (
        <div className="rounded-xl border border-surface-200 bg-surface-0 overflow-hidden shadow-sm">
          <div className="px-5 py-4 border-b border-surface-100">
            <h2 className="text-base font-semibold text-surface-900">All Models in Lineage</h2>
            <p className="text-xs text-surface-800/40 mt-0.5">{tree.nodes.length} models, {tree.edges.length} relationships</p>
          </div>
          <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-3 p-5">
            {tree.nodes.map((node) => (
              <LineageNode
                key={node.id}
                node={node}
                isSelected={node.id === selectedId}
                onClick={handleNodeClick}
                compact
              />
            ))}
          </div>
        </div>
      )}
    </div>
  );
}
