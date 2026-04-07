"use client";

import { useState, useEffect, useCallback, useMemo, useRef } from "react";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

type NodeStatus = "online" | "offline" | "degraded";
type NodeType = "relay" | "provider" | "user";

interface TopologyNode {
  id: string;
  label: string;
  type: NodeType;
  status: NodeStatus;
  x: number;
  y: number;
  region: string;
  models?: string[];
  requestCount?: number;
}

interface TopologyEdge {
  from: string;
  to: string;
  requestCount: number;
  latencyMs: number;
  label?: string;
}

interface TopologyData {
  nodes: TopologyNode[];
  edges: TopologyEdge[];
  totalNodes: number;
  totalEdges: number;
  avgLatencyMs: number;
  totalRequests: number;
}

// ---------------------------------------------------------------------------
// Mock data generator
// ---------------------------------------------------------------------------

function generateMockTopology(): TopologyData {
  const regions = ["US", "EU", "Asia"];
  const modelSets = [
    ["llama-3.1-70b", "mistral-7b"],
    ["llama-3.1-8b", "qwen-2.5-72b"],
    ["deepseek-v3", "llama-3.1-70b", "mistral-7b"],
    ["qwen-2.5-7b", "mistral-7b"],
  ];

  const nodes: TopologyNode[] = [];
  const edges: TopologyEdge[] = [];

  // Relays (center cluster)
  const relayCount = 3;
  for (let i = 0; i < relayCount; i++) {
    nodes.push({
      id: `relay-${i}`,
      label: `Relay ${i + 1}`,
      type: "relay",
      status: i === 0 ? "online" : i === 1 ? "online" : "degraded",
      x: 400 + (i - 1) * 140,
      y: 250 + (i === 1 ? -40 : 20),
      region: regions[i],
    });
  }

  // Providers (outer ring)
  const providerCount = 12;
  for (let i = 0; i < providerCount; i++) {
    const angle = (i / providerCount) * Math.PI * 2 - Math.PI / 2;
    const radius = 180 + Math.random() * 60;
    nodes.push({
      id: `provider-${i}`,
      label: `Provider ${i + 1}`,
      type: "provider",
      status: Math.random() > 0.2 ? (Math.random() > 0.15 ? "online" : "degraded") : "offline",
      x: 400 + Math.cos(angle) * radius,
      y: 260 + Math.sin(angle) * radius,
      region: regions[i % regions.length],
      models: modelSets[i % modelSets.length],
      requestCount: Math.floor(Math.random() * 5000) + 200,
    });
  }

  // Users (outer ring)
  const userCount = 8;
  for (let i = 0; i < userCount; i++) {
    const angle = (i / userCount) * Math.PI * 2;
    const radius = 310 + Math.random() * 50;
    nodes.push({
      id: `user-${i}`,
      label: `User ${i + 1}`,
      type: "user",
      status: Math.random() > 0.1 ? "online" : "offline",
      x: 400 + Math.cos(angle) * radius,
      y: 260 + Math.sin(angle) * radius,
      region: regions[i % regions.length],
    });
  }

  // Connect providers to relays
  for (let i = 0; i < providerCount; i++) {
    const relayIdx = i % relayCount;
    edges.push({
      from: `provider-${i}`,
      to: `relay-${relayIdx}`,
      requestCount: Math.floor(Math.random() * 3000) + 100,
      latencyMs: Math.floor(Math.random() * 200) + 20,
    });
    // Some providers connect to multiple relays
    if (i % 3 === 0) {
      edges.push({
        from: `provider-${i}`,
        to: `relay-${(relayIdx + 1) % relayCount}`,
        requestCount: Math.floor(Math.random() * 1000) + 50,
        latencyMs: Math.floor(Math.random() * 300) + 40,
      });
    }
  }

  // Connect users to relays
  for (let i = 0; i < userCount; i++) {
    const relayIdx = i % relayCount;
    edges.push({
      from: `user-${i}`,
      to: `relay-${relayIdx}`,
      requestCount: Math.floor(Math.random() * 500) + 10,
      latencyMs: Math.floor(Math.random() * 150) + 10,
    });
  }

  const allLatencies = edges.map((e) => e.latencyMs);
  const avgLatency = allLatencies.length > 0 ? allLatencies.reduce((a, b) => a + b, 0) / allLatencies.length : 0;

  return {
    nodes,
    edges,
    totalNodes: nodes.length,
    totalEdges: edges.length,
    avgLatencyMs: Math.round(avgLatency),
    totalRequests: edges.reduce((s, e) => s + e.requestCount, 0),
  };
}

// ---------------------------------------------------------------------------
// Color helpers
// ---------------------------------------------------------------------------

const NODE_COLORS: Record<NodeStatus, string> = {
  online: "#10b981",
  degraded: "#f59e0b",
  offline: "#ef4444",
};

const NODE_COLORS_DARK: Record<NodeStatus, string> = {
  online: "#34d399",
  degraded: "#fbbf24",
  offline: "#f87171",
};

const NODE_FILLS: Record<NodeStatus, string> = {
  online: "fill-emerald-500",
  degraded: "fill-amber-500",
  offline: "fill-red-400",
};

const NODE_GLOWS: Record<NodeStatus, string> = {
  online: "fill-emerald-500/20",
  degraded: "fill-amber-500/20",
  offline: "fill-red-400/20",
};

const NODE_SIZES: Record<NodeType, number> = {
  relay: 18,
  provider: 14,
  user: 10,
};

const NODE_LABEL_OFFSETS: Record<NodeType, number> = {
  relay: 24,
  provider: 20,
  user: 16,
};

// ---------------------------------------------------------------------------
// SVG icon shapes
// ---------------------------------------------------------------------------

function NodeShape({ type, x, y, size, status }: { type: NodeType; x: number; y: number; size: number; status: NodeStatus }) {
  if (type === "relay") {
    return (
      <g>
        <circle cx={x} cy={y} r={size + 6} className={NODE_GLOWS[status]} />
        <rect x={x - size} y={y - size} width={size * 2} height={size * 2} rx={4} className={NODE_FILLS[status]} />
        <rect x={x - size + 2} y={y - size + 2} width={size * 2 - 4} height={size * 2 - 4} rx={3} fill="none" className="stroke-white/30" strokeWidth="1" />
      </g>
    );
  }
  if (type === "provider") {
    return (
      <g>
        <circle cx={x} cy={y} r={size + 5} className={NODE_GLOWS[status]} />
        <polygon
          points={`${x},${y - size} ${x + size},${y + size * 0.7} ${x - size},${y + size * 0.7}`}
          className={NODE_FILLS[status]}
        />
      </g>
    );
  }
  // User - circle
  return (
    <g>
      <circle cx={x} cy={y} r={size + 4} className={NODE_GLOWS[status]} />
      <circle cx={x} cy={y} r={size} className={NODE_FILLS[status]} />
    </g>
  );
}

// ---------------------------------------------------------------------------
// Legend
// ---------------------------------------------------------------------------

function Legend({ filters, onFilterChange }: {
  filters: { region: string; modelType: string };
  onFilterChange: (key: string, value: string) => void;
}) {
  return (
    <div className="flex flex-wrap items-center gap-4 text-xs">
      {/* Node types */}
      <div className="flex items-center gap-3">
        <LegendItem icon="rect" label="Relay" color="text-emerald-500" />
        <LegendItem icon="tri" label="Provider" color="text-blue-500" />
        <LegendItem icon="circle" label="User" color="text-violet-500" />
      </div>

      {/* Status */}
      <div className="flex items-center gap-3">
        <LegendItem icon="dot" label="Online" color="text-emerald-500" />
        <LegendItem icon="dot" label="Degraded" color="text-amber-500" />
        <LegendItem icon="dot" label="Offline" color="text-red-400" />
      </div>

      {/* Filters */}
      <select
        value={filters.region}
        onChange={(e) => onFilterChange("region", e.target.value)}
        className="px-2 py-1 text-xs rounded border border-surface-200 bg-surface-0 text-surface-800 focus:outline-none focus:ring-1 focus:ring-brand-500/30"
      >
        <option value="all">All Regions</option>
        <option value="US">US</option>
        <option value="EU">EU</option>
        <option value="Asia">Asia</option>
      </select>
    </div>
  );
}

function LegendItem({ icon, label, color }: { icon: "rect" | "tri" | "circle" | "dot"; label: string; color: string }) {
  return (
    <span className="flex items-center gap-1">
      {icon === "rect" && <span className={`w-3 h-3 rounded-sm ${color} bg-current`} />}
      {icon === "tri" && (
        <svg className="w-3 h-3" viewBox="0 0 12 12"><polygon points="6,1 11,10 1,10" className={color} fill="currentColor" /></svg>
      )}
      {icon === "circle" && <span className={`w-3 h-3 rounded-full ${color} bg-current`} />}
      {icon === "dot" && <span className={`w-2 h-2 rounded-full ${color} bg-current`} />}
      <span className="text-surface-800/50">{label}</span>
    </span>
  );
}

// ---------------------------------------------------------------------------
// Stat card
// ---------------------------------------------------------------------------

function StatCard({ label, value, icon }: { label: string; value: string; icon: React.ReactNode }) {
  return (
    <div className="rounded-xl border border-surface-200 bg-surface-0 p-4">
      <div className="flex items-center gap-2 mb-1">
        <div className="text-brand-600">{icon}</div>
        <span className="text-xs text-surface-800/50">{label}</span>
      </div>
      <div className="text-lg font-bold text-surface-900">{value}</div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Loading skeleton
// ---------------------------------------------------------------------------

function LoadingSkeleton() {
  return (
    <div className="max-w-6xl mx-auto px-4 py-8 space-y-6 animate-pulse">
      <div className="h-8 w-48 rounded-lg bg-surface-200" />
      <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
        {Array.from({ length: 4 }).map((_, i) => (
          <div key={i} className="h-20 rounded-xl border border-surface-200 bg-surface-0" />
        ))}
      </div>
      <div className="h-96 rounded-xl border border-surface-200 bg-surface-0" />
    </div>
  );
}

// ---------------------------------------------------------------------------
// Main page
// ---------------------------------------------------------------------------

export default function TopologyPage() {
  const [data, setData] = useState<TopologyData | null>(null);
  const [loading, setLoading] = useState(true);
  const [hoveredNode, setHoveredNode] = useState<string | null>(null);
  const [filters, setFilters] = useState({ region: "all", modelType: "all" });
  const [isDark, setIsDark] = useState(false);
  const svgRef = useRef<SVGSVGElement>(null);

  // Detect dark mode
  useEffect(() => {
    const check = () => setIsDark(document.documentElement.classList.contains("dark"));
    check();
    const observer = new MutationObserver(check);
    observer.observe(document.documentElement, { attributes: true, attributeFilter: ["class"] });
    return () => observer.disconnect();
  }, []);

  // Load topology data
  const loadData = useCallback(async () => {
    try {
      // Try fetching from API first
      const res = await fetch("/api/topology").catch(() => null);
      if (res?.ok) {
        const json = await res.json();
        setData(json as TopologyData);
      } else {
        // Fallback to mock data
        setData(generateMockTopology());
      }
    } catch {
      setData(generateMockTopology());
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    loadData();
    // Refresh every 15 seconds for simulation
    const interval = setInterval(() => {
      setData(generateMockTopology());
    }, 15_000);
    return () => clearInterval(interval);
  }, [loadData]);

  const handleFilterChange = useCallback((key: string, value: string) => {
    setFilters((prev) => ({ ...prev, [key]: value }));
  }, []);

  // Filtered data
  const filteredData = useMemo(() => {
    if (!data) return null;
    const filteredNodes = data.nodes.filter((n) => {
      if (filters.region !== "all" && n.region !== filters.region) return false;
      return true;
    });
    const nodeIds = new Set(filteredNodes.map((n) => n.id));
    const filteredEdges = data.edges.filter((e) => nodeIds.has(e.from) && nodeIds.has(e.to));
    return { ...data, nodes: filteredNodes, edges: filteredEdges };
  }, [data, filters]);

  // SVG viewBox dimensions
  const viewBoxWidth = 800;
  const viewBoxHeight = 520;

  if (loading) return <LoadingSkeleton />;
  if (!filteredData) return null;

  const nodeColorFn = (status: NodeStatus) => isDark ? NODE_COLORS_DARK[status] : NODE_COLORS[status];

  return (
    <div className="max-w-6xl mx-auto px-4 py-8">
      {/* Header */}
      <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-3 mb-6">
        <div>
          <h1 className="text-2xl font-bold text-surface-900">Network Topology</h1>
          <p className="text-sm text-surface-800/50 mt-0.5">
            Live visualization of the Xergon relay, provider, and user network
          </p>
        </div>
        <button
          type="button"
          onClick={loadData}
          className="inline-flex items-center gap-2 px-3 py-1.5 rounded-lg border border-surface-200 bg-surface-0 text-sm font-medium text-surface-800/70 hover:bg-surface-50 transition-colors"
        >
          <svg className="w-3.5 h-3.5" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <polyline points="23 4 23 10 17 10" />
            <path d="M20.49 15a9 9 0 11-2.12-9.36L23 10" />
          </svg>
          Refresh
        </button>
      </div>

      {/* Stats summary */}
      <div className="grid grid-cols-2 md:grid-cols-4 gap-4 mb-6">
        <StatCard
          label="Total Nodes"
          value={String(filteredData.totalNodes)}
          icon={<svg className="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5"><circle cx="12" cy="12" r="10" /><path d="M12 2a15.3 15.3 0 014 10 15.3 15.3 0 01-4 10 15.3 15.3 0 01-4-10 15.3 15.3 0 014-10z" /></svg>}
        />
        <StatCard
          label="Connections"
          value={String(filteredData.totalEdges)}
          icon={<svg className="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5"><path d="M10 13a5 5 0 007.54.54l3-3a5 5 0 00-7.07-7.07l-1.72 1.71" /><path d="M14 11a5 5 0 00-7.54-.54l-3 3a5 5 0 007.07 7.07l1.71-1.71" /></svg>}
        />
        <StatCard
          label="Avg Latency"
          value={`${filteredData.avgLatencyMs}ms`}
          icon={<svg className="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5"><circle cx="12" cy="12" r="10" /><polyline points="12 6 12 12 16 14" /></svg>}
        />
        <StatCard
          label="Total Requests"
          value={formatReqCount(filteredData.totalRequests)}
          icon={<svg className="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5"><polyline points="22 12 18 12 15 21 9 3 6 12 2 12" /></svg>}
        />
      </div>

      {/* Legend + filters */}
      <div className="rounded-xl border border-surface-200 bg-surface-0 p-3 mb-4">
        <Legend filters={filters} onFilterChange={handleFilterChange} />
      </div>

      {/* SVG Topology */}
      <div className="rounded-xl border border-surface-200 bg-surface-0 overflow-hidden">
        <svg
          ref={svgRef}
          viewBox={`0 0 ${viewBoxWidth} ${viewBoxHeight}`}
          className="w-full h-auto min-h-[400px] md:min-h-[520px]"
          style={{ background: isDark ? "#0f172a" : "#fafafa" }}
        >
          <defs>
            {/* Grid pattern */}
            <pattern id="grid" width="40" height="40" patternUnits="userSpaceOnUse">
              <path d="M 40 0 L 0 0 0 40" fill="none" stroke={isDark ? "#1e293b" : "#e2e8f0"} strokeWidth="0.5" />
            </pattern>
          </defs>

          {/* Background grid */}
          <rect width="100%" height="100%" fill="url(#grid)" />

          {/* Edges */}
          {filteredData.edges.map((edge, i) => {
            const fromNode = filteredData.nodes.find((n) => n.id === edge.from);
            const toNode = filteredData.nodes.find((n) => n.id === edge.to);
            if (!fromNode || !toNode) return null;
            const isHighlighted = hoveredNode === edge.from || hoveredNode === edge.to;
            const opacity = hoveredNode ? (isHighlighted ? 0.8 : 0.1) : 0.3;
            const edgeColor = edge.latencyMs > 200 ? (isDark ? "#fbbf24" : "#f59e0b") : (isDark ? "#94a3b8" : "#cbd5e1");

            return (
              <g key={`edge-${i}`}>
                <line
                  x1={fromNode.x} y1={fromNode.y}
                  x2={toNode.x} y2={toNode.y}
                  stroke={edgeColor}
                  strokeWidth={isHighlighted ? 2 : 1}
                  opacity={opacity}
                />
                {/* Edge label for highlighted edges */}
                {isHighlighted && (
                  <>
                    <text
                      x={(fromNode.x + toNode.x) / 2}
                      y={(fromNode.y + toNode.y) / 2 - 6}
                      textAnchor="middle"
                      className="fill-surface-800/60"
                      fontSize="8"
                    >
                      {edge.latencyMs}ms
                    </text>
                    <text
                      x={(fromNode.x + toNode.x) / 2}
                      y={(fromNode.y + toNode.y) / 2 + 4}
                      textAnchor="middle"
                      className="fill-surface-800/40"
                      fontSize="7"
                    >
                      {edge.requestCount} req
                    </text>
                  </>
                )}
              </g>
            );
          })}

          {/* Nodes */}
          {filteredData.nodes.map((node) => {
            const size = NODE_SIZES[node.type];
            const labelOffset = NODE_LABEL_OFFSETS[node.type];
            const isHovered = hoveredNode === node.id;
            const opacity = hoveredNode && !isHovered ? 0.3 : 1;

            return (
              <g
                key={node.id}
                opacity={opacity}
                onMouseEnter={() => setHoveredNode(node.id)}
                onMouseLeave={() => setHoveredNode(null)}
                className="cursor-pointer"
              >
                <NodeShape type={node.type} x={node.x} y={node.y} size={size} status={node.status} />
                <text
                  x={node.x}
                  y={node.y + labelOffset + 4}
                  textAnchor="middle"
                  className="fill-surface-800/60"
                  fontSize="9"
                  fontWeight="500"
                >
                  {node.label}
                </text>
                {/* Tooltip on hover */}
                {isHovered && (
                  <g>
                    <rect
                      x={node.x - 55}
                      y={node.y - size - 42}
                      width={110}
                      height={32}
                      rx={6}
                      className="fill-surface-900 dark:fill-surface-100"
                      opacity={0.95}
                    />
                    <text
                      x={node.x}
                      y={node.y - size - 28}
                      textAnchor="middle"
                      className="fill-surface-0 dark:fill-surface-900"
                      fontSize="8"
                      fontWeight="600"
                    >
                      {node.label} ({node.region})
                    </text>
                    <text
                      x={node.x}
                      y={node.y - size - 16}
                      textAnchor="middle"
                      className="fill-surface-800/50 dark:fill-surface-800/70"
                      fontSize="7"
                    >
                      {node.status} {node.requestCount ? `| ${node.requestCount} req` : ""}
                    </text>
                  </g>
                )}
              </g>
            );
          })}
        </svg>
      </div>

      {/* Node list summary */}
      <div className="grid grid-cols-1 md:grid-cols-3 gap-4 mt-4">
        <NodeCountSection title="Relays" nodes={filteredData.nodes.filter((n) => n.type === "relay")} color="text-emerald-500" />
        <NodeCountSection title="Providers" nodes={filteredData.nodes.filter((n) => n.type === "provider")} color="text-blue-500" />
        <NodeCountSection title="Users" nodes={filteredData.nodes.filter((n) => n.type === "user")} color="text-violet-500" />
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function formatReqCount(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return String(n);
}

function NodeCountSection({ title, nodes, color }: { title: string; nodes: TopologyNode[]; color: string }) {
  const online = nodes.filter((n) => n.status === "online").length;
  const degraded = nodes.filter((n) => n.status === "degraded").length;
  const offline = nodes.filter((n) => n.status === "offline").length;
  return (
    <div className="rounded-xl border border-surface-200 bg-surface-0 p-4">
      <h3 className={`text-sm font-semibold ${color} mb-2`}>{title}</h3>
      <div className="flex items-center gap-4 text-xs text-surface-800/50">
        <span>{nodes.length} total</span>
        <span className="text-emerald-600">{online} online</span>
        {degraded > 0 && <span className="text-amber-600">{degraded} degraded</span>}
        {offline > 0 && <span className="text-red-500">{offline} offline</span>}
      </div>
    </div>
  );
}
