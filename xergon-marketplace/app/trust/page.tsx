"use client";

import { useState, useEffect, useMemo, useCallback } from "react";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface TrustScore {
  tee: number;
  zk: number;
  uptime: number;
  ponw: number;
  reviews: number;
}

interface ScoreHistory {
  date: string;
  total: number;
}

interface Provider {
  id: string;
  name: string;
  teeVerified: boolean;
  zkVerified: boolean;
  scores: TrustScore;
  history: ScoreHistory[];
  attestationHash: string;
  attestationDate: string;
}

type SortKey = "total" | "tee" | "zk" | "uptime" | "ponw";

// ---------------------------------------------------------------------------
// Mock data
// ---------------------------------------------------------------------------

const MOCK_PROVIDERS: Provider[] = [
  {
    id: "p1", name: "NeuralForge Alpha", teeVerified: true, zkVerified: true,
    scores: { tee: 95, zk: 92, uptime: 98, ponw: 88, reviews: 91 },
    history: [
      { date: "2025-10-01", total: 88 }, { date: "2025-10-08", total: 89 },
      { date: "2025-10-15", total: 91 }, { date: "2025-10-22", total: 90 },
      { date: "2025-10-29", total: 92 }, { date: "2025-11-05", total: 93 },
      { date: "2025-11-12", total: 93 },
    ],
    attestationHash: "0x7a3f...e9b2", attestationDate: "2025-11-12T08:30:00Z",
  },
  {
    id: "p2", name: "DeepCompute Sigma", teeVerified: true, zkVerified: true,
    scores: { tee: 88, zk: 85, uptime: 94, ponw: 78, reviews: 82 },
    history: [
      { date: "2025-10-01", total: 80 }, { date: "2025-10-08", total: 82 },
      { date: "2025-10-15", total: 83 }, { date: "2025-10-22", total: 84 },
      { date: "2025-10-29", total: 85 }, { date: "2025-11-05", total: 84 },
      { date: "2025-11-12", total: 85 },
    ],
    attestationHash: "0x2c8d...f1a4", attestationDate: "2025-11-11T14:15:00Z",
  },
  {
    id: "p3", name: "VertexMind Beta", teeVerified: true, zkVerified: false,
    scores: { tee: 72, zk: 45, uptime: 89, ponw: 70, reviews: 65 },
    history: [
      { date: "2025-10-01", total: 62 }, { date: "2025-10-08", total: 63 },
      { date: "2025-10-15", total: 64 }, { date: "2025-10-22", total: 65 },
      { date: "2025-10-29", total: 66 }, { date: "2025-11-05", total: 67 },
      { date: "2025-11-12", total: 68 },
    ],
    attestationHash: "0x9e1b...c3d7", attestationDate: "2025-11-10T10:00:00Z",
  },
  {
    id: "p4", name: "CipherNode Pro", teeVerified: false, zkVerified: true,
    scores: { tee: 30, zk: 91, uptime: 85, ponw: 80, reviews: 74 },
    history: [
      { date: "2025-10-01", total: 68 }, { date: "2025-10-08", total: 69 },
      { date: "2025-10-15", total: 70 }, { date: "2025-10-22", total: 71 },
      { date: "2025-10-29", total: 72 }, { date: "2025-11-05", total: 71 },
      { date: "2025-11-12", total: 72 },
    ],
    attestationHash: "0x5f2a...d8e1", attestationDate: "2025-11-09T22:45:00Z",
  },
  {
    id: "p5", name: "QuantumEdge Delta", teeVerified: true, zkVerified: true,
    scores: { tee: 82, zk: 78, uptime: 91, ponw: 85, reviews: 80 },
    history: [
      { date: "2025-10-01", total: 78 }, { date: "2025-10-08", total: 79 },
      { date: "2025-10-15", total: 80 }, { date: "2025-10-22", total: 81 },
      { date: "2025-10-29", total: 82 }, { date: "2025-11-05", total: 83 },
      { date: "2025-11-12", total: 83 },
    ],
    attestationHash: "0xb4c7...a2f9", attestationDate: "2025-11-12T06:20:00Z",
  },
  {
    id: "p6", name: "TrustGrid Omega", teeVerified: true, zkVerified: false,
    scores: { tee: 68, zk: 35, uptime: 76, ponw: 55, reviews: 48 },
    history: [
      { date: "2025-10-01", total: 52 }, { date: "2025-10-08", total: 53 },
      { date: "2025-10-15", total: 54 }, { date: "2025-10-22", total: 55 },
      { date: "2025-10-29", total: 56 }, { date: "2025-11-05", total: 55 },
      { date: "2025-11-12", total: 56 },
    ],
    attestationHash: "0xd1e8...7b3c", attestationDate: "2025-11-08T16:30:00Z",
  },
  {
    id: "p7", name: "SafeInfer Gamma", teeVerified: true, zkVerified: true,
    scores: { tee: 90, zk: 87, uptime: 96, ponw: 92, reviews: 88 },
    history: [
      { date: "2025-10-01", total: 86 }, { date: "2025-10-08", total: 87 },
      { date: "2025-10-15", total: 88 }, { date: "2025-10-22", total: 89 },
      { date: "2025-10-29", total: 89 }, { date: "2025-11-05", total: 90 },
      { date: "2025-11-12", total: 91 },
    ],
    attestationHash: "0x3a9f...1e5d", attestationDate: "2025-11-12T11:00:00Z",
  },
  {
    id: "p8", name: "RawPower Epsilon", teeVerified: false, zkVerified: false,
    scores: { tee: 15, zk: 20, uptime: 88, ponw: 42, reviews: 35 },
    history: [
      { date: "2025-10-01", total: 36 }, { date: "2025-10-08", total: 37 },
      { date: "2025-10-15", total: 38 }, { date: "2025-10-22", total: 38 },
      { date: "2025-10-29", total: 39 }, { date: "2025-11-05", total: 39 },
      { date: "2025-11-12", total: 40 },
    ],
    attestationHash: "0x6e4c...b9f8", attestationDate: "2025-11-06T09:10:00Z",
  },
  {
    id: "p9", name: "EnclaveAI Zeta", teeVerified: true, zkVerified: true,
    scores: { tee: 97, zk: 94, uptime: 99, ponw: 90, reviews: 95 },
    history: [
      { date: "2025-10-01", total: 90 }, { date: "2025-10-08", total: 91 },
      { date: "2025-10-15", total: 92 }, { date: "2025-10-22", total: 93 },
      { date: "2025-10-29", total: 94 }, { date: "2025-11-05", total: 94 },
      { date: "2025-11-12", total: 95 },
    ],
    attestationHash: "0x8b2d...4c6a", attestationDate: "2025-11-12T07:45:00Z",
  },
  {
    id: "p10", name: "OpenNet Theta", teeVerified: false, zkVerified: true,
    scores: { tee: 40, zk: 82, uptime: 72, ponw: 60, reviews: 55 },
    history: [
      { date: "2025-10-01", total: 58 }, { date: "2025-10-08", total: 59 },
      { date: "2025-10-15", total: 60 }, { date: "2025-10-22", total: 61 },
      { date: "2025-10-29", total: 61 }, { date: "2025-11-05", total: 62 },
      { date: "2025-11-12", total: 62 },
    ],
    attestationHash: "0xc5f1...3a8e", attestationDate: "2025-11-07T13:55:00Z",
  },
];

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function getTotal(s: TrustScore): number {
  return Math.round((s.tee + s.zk + s.uptime + s.ponw + s.reviews) / 5);
}

function scoreColor(v: number): string {
  if (v >= 80) return "bg-green-500";
  if (v >= 50) return "bg-yellow-500";
  return "bg-red-500";
}

function scoreTextColor(v: number): string {
  if (v >= 80) return "text-green-600 dark:text-green-400";
  if (v >= 50) return "text-yellow-600 dark:text-yellow-400";
  return "text-red-600 dark:text-red-400";
}

function scoreBadgeColor(v: number): string {
  if (v >= 80) return "bg-green-100 text-green-700 dark:bg-green-900/30 dark:text-green-400";
  if (v >= 50) return "bg-yellow-100 text-yellow-700 dark:bg-yellow-900/30 dark:text-yellow-400";
  return "bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-400";
}

const COMPONENT_LABELS: { key: keyof TrustScore; label: string }[] = [
  { key: "tee", label: "TEE" },
  { key: "zk", label: "ZK Proof" },
  { key: "uptime", label: "Uptime" },
  { key: "ponw", label: "PoNW" },
  { key: "reviews", label: "Reviews" },
];

// ---------------------------------------------------------------------------
// Skeleton
// ---------------------------------------------------------------------------

function TrustCardSkeleton() {
  return (
    <div className="rounded-xl border border-surface-200 bg-surface-0 p-5 space-y-4 animate-pulse">
      <div className="flex items-center justify-between">
        <div className="h-5 w-36 rounded bg-surface-200" />
        <div className="h-8 w-16 rounded-lg bg-surface-200" />
      </div>
      <div className="space-y-2">
        {Array.from({ length: 5 }, (_, i) => (
          <div key={i} className="flex items-center gap-3">
            <div className="h-3 w-14 rounded bg-surface-200" />
            <div className="flex-1 h-2 rounded-full bg-surface-200" />
            <div className="h-3 w-6 rounded bg-surface-200" />
          </div>
        ))}
      </div>
    </div>
  );
}

function StatsSkeleton() {
  return (
    <div className="grid grid-cols-1 sm:grid-cols-3 gap-4 animate-pulse">
      {Array.from({ length: 3 }, (_, i) => (
        <div key={i} className="rounded-xl border border-surface-200 bg-surface-0 p-4">
          <div className="h-3 w-24 rounded bg-surface-200 mb-2" />
          <div className="h-7 w-12 rounded bg-surface-200" />
        </div>
      ))}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Shield icon
// ---------------------------------------------------------------------------

function ShieldIcon({ className }: { className?: string }) {
  return (
    <svg className={className} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z" />
    </svg>
  );
}

// ---------------------------------------------------------------------------
// Main component
// ---------------------------------------------------------------------------

export default function TrustPage() {
  const [loading, setLoading] = useState(true);
  const [search, setSearch] = useState("");
  const [sortBy, setSortBy] = useState<SortKey>("total");
  const [minScore, setMinScore] = useState(0);
  const [teeOnly, setTeeOnly] = useState(false);
  const [expandedId, setExpandedId] = useState<string | null>(null);

  useEffect(() => {
    const t = setTimeout(() => setLoading(false), 800);
    return () => clearTimeout(t);
  }, []);

  const toggleExpand = useCallback((id: string) => {
    setExpandedId((prev) => (prev === id ? null : id));
  }, []);

  const filteredProviders = useMemo(() => {
    let list = [...MOCK_PROVIDERS];
    if (search) {
      const q = search.toLowerCase();
      list = list.filter((p) => p.name.toLowerCase().includes(q));
    }
    if (minScore > 0) {
      list = list.filter((p) => getTotal(p.scores) >= minScore);
    }
    if (teeOnly) {
      list = list.filter((p) => p.teeVerified);
    }
    list.sort((a, b) => {
      const getVal = (p: Provider): number => {
        if (sortBy === "total") return getTotal(p.scores);
        return p.scores[sortBy];
      };
      return getVal(b) - getVal(a);
    });
    return list;
  }, [search, sortBy, minScore, teeOnly]);

  const stats = useMemo(() => {
    const all = MOCK_PROVIDERS;
    const avg = all.reduce((s, p) => s + getTotal(p.scores), 0) / all.length;
    const teeCount = all.filter((p) => p.teeVerified).length;
    const zkCount = all.filter((p) => p.zkVerified).length;
    return { avg: Math.round(avg * 10) / 10, teeCount, zkCount };
  }, []);

  const sortOptions: { key: SortKey; label: string }[] = [
    { key: "total", label: "Total Score" },
    { key: "tee", label: "TEE" },
    { key: "zk", label: "ZK Proof" },
    { key: "uptime", label: "Uptime" },
    { key: "ponw", label: "PoNW" },
  ];

  return (
    <main className="mx-auto max-w-6xl px-4 py-6 space-y-6">
      {/* Header */}
      <div className="space-y-1">
        <div className="flex items-center gap-3">
          <ShieldIcon className="w-7 h-7 text-brand-600" />
          <h1 className="text-2xl font-bold text-surface-900">Trust Score Explorer</h1>
        </div>
        <p className="text-sm text-surface-800/60">
          Explore and compare provider trust scores across TEE, ZK proofs, uptime, PoNW, and reviews
        </p>
      </div>

      {/* Stats */}
      {loading ? (
        <StatsSkeleton />
      ) : (
        <div className="grid grid-cols-1 sm:grid-cols-3 gap-4">
          <div className="rounded-xl border border-surface-200 bg-surface-0 p-4">
            <div className="text-xs text-surface-800/50 mb-1">Average Trust Score</div>
            <div className={`text-2xl font-bold ${scoreTextColor(stats.avg)}`}>{stats.avg}</div>
          </div>
          <div className="rounded-xl border border-surface-200 bg-surface-0 p-4">
            <div className="text-xs text-surface-800/50 mb-1">TEE Verified</div>
            <div className="text-2xl font-bold text-surface-900">{stats.teeCount}<span className="text-sm font-normal text-surface-800/40"> / {MOCK_PROVIDERS.length}</span></div>
          </div>
          <div className="rounded-xl border border-surface-200 bg-surface-0 p-4">
            <div className="text-xs text-surface-800/50 mb-1">ZK Verified</div>
            <div className="text-2xl font-bold text-surface-900">{stats.zkCount}<span className="text-sm font-normal text-surface-800/40"> / {MOCK_PROVIDERS.length}</span></div>
          </div>
        </div>
      )}

      {/* Filters */}
      <div className="flex flex-col sm:flex-row gap-3 items-start sm:items-center">
        {/* Search */}
        <div className="relative flex-1 w-full sm:max-w-xs">
          <svg className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-surface-800/40" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
            <circle cx="11" cy="11" r="8" /><line x1="21" y1="21" x2="16.65" y2="16.65" />
          </svg>
          <input
            type="text"
            placeholder="Search providers..."
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            className="w-full pl-9 pr-3 py-2 text-sm rounded-lg border border-surface-200 bg-surface-0 text-surface-800 focus:outline-none focus:ring-2 focus:ring-brand-500/30"
          />
        </div>

        {/* Sort */}
        <select
          value={sortBy}
          onChange={(e) => setSortBy(e.target.value as SortKey)}
          className="px-3 py-2 text-sm rounded-lg border border-surface-200 bg-surface-0 text-surface-800 focus:outline-none focus:ring-2 focus:ring-brand-500/30"
        >
          {sortOptions.map((o) => (
            <option key={o.key} value={o.key}>Sort: {o.label}</option>
          ))}
        </select>

        {/* Min Score Slider */}
        <div className="flex items-center gap-2">
          <label className="text-xs text-surface-800/50 whitespace-nowrap">Min: {minScore}</label>
          <input
            type="range"
            min={0}
            max={100}
            step={5}
            value={minScore}
            onChange={(e) => setMinScore(Number(e.target.value))}
            className="w-24 accent-brand-600"
          />
        </div>

        {/* TEE Only Toggle */}
        <label className="inline-flex items-center gap-2 cursor-pointer select-none">
          <span className="text-sm text-surface-800/70">TEE only</span>
          <button
            type="button"
            role="switch"
            aria-checked={teeOnly}
            onClick={() => setTeeOnly(!teeOnly)}
            className={`relative inline-flex h-5 w-9 items-center rounded-full transition-colors ${teeOnly ? "bg-brand-600" : "bg-surface-300"}`}
          >
            <span className={`inline-block h-3.5 w-3.5 rounded-full bg-white transition-transform ${teeOnly ? "translate-x-4.5" : "translate-x-0.5"}`} />
          </button>
        </label>
      </div>

      {/* Status bar */}
      <div className="text-xs text-surface-800/50">
        {loading ? "Loading..." : `${filteredProviders.length} of ${MOCK_PROVIDERS.length} providers shown`}
      </div>

      {/* Loading skeletons */}
      {loading && (
        <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
          {Array.from({ length: 6 }, (_, i) => (
            <TrustCardSkeleton key={i} />
          ))}
        </div>
      )}

      {/* Empty state */}
      {!loading && filteredProviders.length === 0 && (
        <div className="flex flex-col items-center justify-center py-16 text-center space-y-3">
          <ShieldIcon className="w-12 h-12 text-surface-300" />
          <div>
            <p className="font-medium text-surface-800/70">No providers match your filters</p>
            <p className="text-sm text-surface-800/50">Try lowering the minimum score or removing filters</p>
          </div>
        </div>
      )}

      {/* Provider grid */}
      {!loading && filteredProviders.length > 0 && (
        <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
          {filteredProviders.map((provider) => {
            const total = getTotal(provider.scores);
            const isExpanded = expandedId === provider.id;
            return (
              <div key={provider.id} className="rounded-xl border border-surface-200 bg-surface-0 transition-all hover:shadow-sm">
                {/* Card header */}
                <div className="p-5 space-y-4">
                  <div className="flex items-center justify-between">
                    <div className="flex items-center gap-2 min-w-0">
                      <ShieldIcon className="w-5 h-5 text-surface-800/40 flex-shrink-0" />
                      <span className="font-medium text-surface-900 truncate">{provider.name}</span>
                    </div>
                    <span className={`text-xl font-bold ${scoreTextColor(total)}`}>{total}</span>
                  </div>

                  {/* Verification badges */}
                  <div className="flex gap-2">
                    {provider.teeVerified && (
                      <span className="px-2 py-0.5 text-xs rounded-md bg-blue-100 text-blue-700 dark:bg-blue-900/30 dark:text-blue-400 font-medium">TEE Verified</span>
                    )}
                    {provider.zkVerified && (
                      <span className="px-2 py-0.5 text-xs rounded-md bg-purple-100 text-purple-700 dark:bg-purple-900/30 dark:text-purple-400 font-medium">ZK Verified</span>
                    )}
                  </div>

                  {/* Component bars */}
                  <div className="space-y-2">
                    {COMPONENT_LABELS.map(({ key, label }) => {
                      const val = provider.scores[key];
                      return (
                        <div key={key} className="flex items-center gap-3">
                          <span className="text-xs text-surface-800/60 w-14 flex-shrink-0">{label}</span>
                          <div className="flex-1 h-2 rounded-full bg-surface-100 dark:bg-surface-800 overflow-hidden">
                            <div
                              className={`h-full rounded-full transition-all ${scoreColor(val)}`}
                              style={{ width: `${val}%` }}
                            />
                          </div>
                          <span className={`text-xs font-medium w-7 text-right ${scoreTextColor(val)}`}>{val}</span>
                        </div>
                      );
                    })}
                  </div>

                  {/* Expand toggle */}
                  <button
                    type="button"
                    onClick={() => toggleExpand(provider.id)}
                    className="flex items-center gap-1 text-xs text-brand-600 hover:text-brand-700 font-medium transition-colors"
                  >
                    {isExpanded ? "Hide details" : "View details"}
                    <svg className={`w-3.5 h-3.5 transition-transform ${isExpanded ? "rotate-180" : ""}`} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                      <polyline points="6 9 12 15 18 9" />
                    </svg>
                  </button>
                </div>

                {/* Expanded detail */}
                {isExpanded && (
                  <div className="border-t border-surface-100 px-5 py-4 space-y-4">
                    {/* Score history mini chart */}
                    <div>
                      <h4 className="text-xs font-semibold text-surface-800/50 uppercase tracking-wide mb-3">Score History (7 weeks)</h4>
                      <div className="flex items-end gap-1.5 h-20">
                        {provider.history.map((h, i) => {
                          const barH = Math.max(8, (h.total / 100) * 80);
                          return (
                            <div key={i} className="flex-1 flex flex-col items-center gap-1">
                              <span className="text-[10px] text-surface-800/40">{h.total}</span>
                              <div
                                className={`w-full rounded-t ${scoreColor(h.total)}`}
                                style={{ height: `${barH}px` }}
                              />
                              <span className="text-[9px] text-surface-800/30">{h.date.slice(5)}</span>
                            </div>
                          );
                        })}
                      </div>
                    </div>

                    {/* Attestation info */}
                    <div>
                      <h4 className="text-xs font-semibold text-surface-800/50 uppercase tracking-wide mb-2">Attestation</h4>
                      <div className="grid grid-cols-2 gap-2 text-xs">
                        <div className="rounded-lg bg-surface-50 dark:bg-surface-800/50 p-2">
                          <div className="text-surface-800/40 mb-0.5">Hash</div>
                          <div className="font-mono text-surface-800/70">{provider.attestationHash}</div>
                        </div>
                        <div className="rounded-lg bg-surface-50 dark:bg-surface-800/50 p-2">
                          <div className="text-surface-800/40 mb-0.5">Last Attested</div>
                          <div className="text-surface-800/70">{new Date(provider.attestationDate).toLocaleDateString()}</div>
                        </div>
                      </div>
                    </div>
                  </div>
                )}
              </div>
            );
          })}
        </div>
      )}
    </main>
  );
}
