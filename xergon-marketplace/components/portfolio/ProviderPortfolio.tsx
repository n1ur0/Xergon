"use client";

import { useState, useEffect, useMemo } from "react";
import Link from "next/link";
import { cn } from "@/lib/utils";
import type {
  ProviderPortfolio,
  PortfolioReview,
  ActivityEvent,
  Certification,
  PortfolioModel,
  PerformanceDataPoint,
  SkillTag,
} from "@/types/portfolio";

// ── Helpers ──

function formatNanoErg(nano: number): string {
  if (nano >= 1_000_000_000) return `${(nano / 1_000_000_000).toFixed(2)} ERG`;
  if (nano >= 1_000_000) return `${(nano / 1_000_000).toFixed(1)}mERG`;
  if (nano >= 1_000) return `${(nano / 1_000).toFixed(1)}\u00B5ERG`;
  return `${nano} nERG`;
}

function formatNumber(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return String(n);
}

function regionFlag(region: string): string {
  const flags: Record<string, string> = {
    US: "\u{1F1FA}\u{1F1F8}",
    EU: "\u{1F1EA}\u{1F1FA}",
    Asia: "\u{1F30F}",
    Other: "\u{1F30D}",
  };
  return flags[region] ?? "\u{1F30D}";
}

const CATEGORY_COLORS: Record<string, string> = {
  nlp: "bg-blue-100 text-blue-700",
  vision: "bg-purple-100 text-purple-700",
  code: "bg-green-100 text-green-700",
  audio: "bg-orange-100 text-orange-700",
  multimodal: "bg-pink-100 text-pink-700",
  embeddings: "bg-cyan-100 text-cyan-700",
};

const ACTIVITY_ICONS: Record<ActivityEvent["type"], string> = {
  model_added: "\u{1F680}",
  model_updated: "\u{1F504}",
  price_change: "\u{1F4B0}",
  status_change: "\u{1F7E2}",
  milestone: "\u{1F3C6}",
  certification: "\uD83C\uDFC5",
  achievement: "\u{2B50}",
};

// ── Star Rating ──

function StarRating({ rating, size = 14 }: { rating: number; size?: number }) {
  return (
    <div className="flex items-center gap-0.5">
      {[1, 2, 3, 4, 5].map((star) => (
        <svg
          key={star}
          xmlns="http://www.w3.org/2000/svg"
          width={size}
          height={size}
          viewBox="0 0 24 24"
          fill={star <= rating ? "currentColor" : "none"}
          stroke="currentColor"
          strokeWidth="2"
          className={star <= rating ? "text-amber-400" : "text-surface-300"}
        >
          <polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2" />
        </svg>
      ))}
    </div>
  );
}

// ── Mini Sparkline Chart ──

function SparklineChart({
  data,
  color = "text-brand-500",
  height = 40,
  label,
}: {
  data: PerformanceDataPoint[];
  color?: string;
  height?: number;
  label: string;
}) {
  if (data.length < 2) return <div className="text-xs text-surface-800/30">No data</div>;

  const values = data.map(d => d.value);
  const min = Math.min(...values);
  const max = Math.max(...values);
  const range = max - min || 1;

  const width = 200;
  const padding = 2;

  const points = values.map((v, i) => {
    const x = padding + (i / (values.length - 1)) * (width - padding * 2);
    const y = padding + (1 - (v - min) / range) * (height - padding * 2);
    return `${x},${y}`;
  }).join(" ");

  return (
    <div>
      <div className="text-xs text-surface-800/50 mb-1">{label}</div>
      <svg viewBox={`0 0 ${width} ${height}`} className="w-full" style={{ height }}>
        <polyline
          points={points}
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
          strokeLinecap="round"
          strokeLinejoin="round"
          className={color}
        />
        {/* Gradient fill */}
        <polyline
          points={`${padding},${height} ${points} ${width - padding},${height}`}
          fill="currentColor"
          className={cn(color, "opacity-10")}
        />
      </svg>
      <div className="flex justify-between text-[10px] text-surface-800/30 mt-0.5">
        <span>{data[0].date.slice(5)}</span>
        <span>{data[data.length - 1].date.slice(5)}</span>
      </div>
    </div>
  );
}

// ── Stat Card ──

function StatCard({
  label,
  value,
  icon,
  valueColor,
}: {
  label: string;
  value: string;
  icon?: string;
  valueColor?: string;
}) {
  return (
    <div className="rounded-lg border border-surface-200 bg-surface-0 p-4">
      <div className="flex items-center gap-1.5 text-xs text-surface-800/40 mb-1.5">
        {icon && <span>{icon}</span>}
        {label}
      </div>
      <div className={cn("text-lg font-semibold", valueColor ?? "text-surface-900")}>
        {value}
      </div>
    </div>
  );
}

// ── Model Showcase Card ──

function ModelShowcaseCard({ model }: { model: PortfolioModel }) {
  return (
    <div className="rounded-lg border border-surface-200 bg-surface-0 p-4 hover:border-brand-300 hover:shadow-sm transition-all">
      <div className="flex items-start justify-between mb-2">
        <div>
          <h3 className="font-medium text-surface-900 text-sm">{model.name}</h3>
          <span className={cn(
            "inline-block mt-0.5 rounded-full px-2 py-0.5 text-[10px] font-medium uppercase",
            model.tier === "free" ? "bg-emerald-100 text-emerald-700" : "bg-brand-100 text-brand-700",
          )}>
            {model.tier}
          </span>
        </div>
        {model.available && (
          <span className="flex items-center gap-1 text-[10px] text-green-600">
            <span className="h-1.5 w-1.5 rounded-full bg-green-500" />
            Online
          </span>
        )}
      </div>

      {model.description && (
        <p className="text-xs text-surface-800/50 mb-2 line-clamp-2">{model.description}</p>
      )}

      <div className="flex items-center gap-3 text-xs text-surface-800/50 mb-3">
        {model.contextWindow && (
          <span>Context: {model.contextWindow >= 1000 ? `${(model.contextWindow / 1000).toFixed(0)}K` : model.contextWindow}</span>
        )}
        {model.avgLatencyMs != null && <span>Latency: {model.avgLatencyMs}ms</span>}
        <span>Requests: {formatNumber(model.requestCount)}</span>
      </div>

      {/* Benchmarks */}
      {model.benchmarks && Object.keys(model.benchmarks).length > 0 && (
        <div className="flex flex-wrap gap-1.5 mb-3">
          {Object.entries(model.benchmarks).map(([name, score]) => (
            <span key={name} className="rounded bg-surface-50 px-1.5 py-0.5 text-[10px] text-surface-800/60">
              {name}: <span className="font-medium">{score}</span>
            </span>
          ))}
        </div>
      )}

      {/* Tags */}
      {model.tags && model.tags.length > 0 && (
        <div className="flex flex-wrap gap-1">
          {model.tags.map(tag => (
            <span key={tag} className="rounded-full bg-surface-100 px-2 py-0.5 text-[10px] text-surface-800/60">
              {tag}
            </span>
          ))}
        </div>
      )}
    </div>
  );
}

// ── Review Card ──

function ReviewCard({ review }: { review: PortfolioReview }) {
  return (
    <div className="rounded-lg border border-surface-200 bg-surface-0 p-4">
      <div className="flex items-center justify-between mb-2">
        <div className="flex items-center gap-2">
          <span className="text-xs font-mono text-surface-800/60">{review.author}</span>
          <StarRating rating={review.rating} size={12} />
        </div>
        <span className="text-xs text-surface-800/30">{review.date}</span>
      </div>
      {review.model && (
        <span className="inline-block mb-1.5 rounded bg-brand-50 px-1.5 py-0.5 text-[10px] text-brand-600 font-medium">
          {review.model}
        </span>
      )}
      <p className="text-sm text-surface-800/70">{review.content}</p>
    </div>
  );
}

// ── Activity Timeline ──

function ActivityTimeline({ events }: { events: ActivityEvent[] }) {
  return (
    <div className="relative pl-6">
      <div className="absolute left-2 top-2 bottom-2 w-px bg-surface-200" />
      <div className="space-y-3">
        {events.map(event => (
          <div key={event.id} className="relative">
            <div className="absolute -left-4 top-1.5 h-3 w-3 rounded-full bg-surface-200 border-2 border-surface-0" />
            <div className="rounded-lg border border-surface-200 bg-surface-0 p-3">
              <div className="flex items-center gap-2 mb-1">
                <span>{ACTIVITY_ICONS[event.type]}</span>
                <span className="text-xs text-surface-800/40">{event.date}</span>
              </div>
              <p className="text-sm text-surface-800/70">{event.description}</p>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}

// ── Certification Badge ──

function CertBadge({ cert }: { cert: Certification }) {
  return (
    <div className="flex items-center gap-2.5 rounded-lg border border-surface-200 bg-surface-0 p-3">
      <span className="text-xl">{cert.icon}</span>
      <div>
        <div className="text-sm font-medium text-surface-900">{cert.label}</div>
        <div className="text-xs text-surface-800/40">{cert.description}</div>
      </div>
    </div>
  );
}

// ── Edit Modal ──

function EditPortfolioModal({
  portfolio,
  onSave,
  onClose,
}: {
  portfolio: ProviderPortfolio;
  onSave: (data: { displayName?: string; bio?: string; website?: string }) => void;
  onClose: () => void;
}) {
  const [displayName, setDisplayName] = useState(portfolio.displayName ?? "");
  const [bio, setBio] = useState(portfolio.bio ?? "");
  const [website, setWebsite] = useState(portfolio.website ?? "");

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40" onClick={onClose}>
      <div className="w-full max-w-md rounded-xl border border-surface-200 bg-surface-0 p-6 shadow-xl" onClick={e => e.stopPropagation()}>
        <h2 className="text-lg font-semibold text-surface-900 mb-4">Edit Portfolio</h2>
        <div className="space-y-4">
          <div>
            <label className="block text-sm text-surface-800/60 mb-1">Display Name</label>
            <input
              type="text"
              value={displayName}
              onChange={e => setDisplayName(e.target.value)}
              className="w-full rounded-lg border border-surface-200 bg-surface-0 px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-brand-500/40"
            />
          </div>
          <div>
            <label className="block text-sm text-surface-800/60 mb-1">Bio</label>
            <textarea
              value={bio}
              onChange={e => setBio(e.target.value)}
              rows={3}
              className="w-full rounded-lg border border-surface-200 bg-surface-0 px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-brand-500/40 resize-none"
            />
          </div>
          <div>
            <label className="block text-sm text-surface-800/60 mb-1">Website</label>
            <input
              type="url"
              value={website}
              onChange={e => setWebsite(e.target.value)}
              className="w-full rounded-lg border border-surface-200 bg-surface-0 px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-brand-500/40"
            />
          </div>
        </div>
        <div className="flex justify-end gap-2 mt-6">
          <button onClick={onClose} className="rounded-lg border border-surface-200 px-4 py-2 text-sm text-surface-800/60 hover:bg-surface-50 transition-colors">
            Cancel
          </button>
          <button
            onClick={() => onSave({ displayName, bio, website })}
            className="rounded-lg bg-brand-600 px-4 py-2 text-sm font-medium text-white hover:bg-brand-700 transition-colors"
          >
            Save Changes
          </button>
        </div>
      </div>
    </div>
  );
}

// ── Main Component ──

interface ProviderPortfolioProps {
  providerId: string;
  isOwner?: boolean;
}

export function ProviderPortfolioComponent({ providerId, isOwner = false }: ProviderPortfolioProps) {
  const [portfolio, setPortfolio] = useState<ProviderPortfolio | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [showEditModal, setShowEditModal] = useState(false);
  const [activeTab, setActiveTab] = useState<"overview" | "models" | "performance" | "reviews">("overview");

  useEffect(() => {
    fetch(`/api/providers/${providerId}/portfolio`)
      .then(res => {
        if (!res.ok) throw new Error("Failed to load portfolio");
        return res.json();
      })
      .then(data => {
        setPortfolio(data);
        setLoading(false);
      })
      .catch(err => {
        setError(err.message);
        setLoading(false);
      });
  }, [providerId]);

  const handleSave = async (data: { displayName?: string; bio?: string; website?: string }) => {
    try {
      const res = await fetch(`/api/providers/${providerId}/portfolio`, {
        method: "PATCH",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(data),
      });
      if (res.ok) {
        const updated = await res.json();
        setPortfolio(prev => prev ? { ...prev, ...updated } : prev);
        setShowEditModal(false);
      }
    } catch {
      // Handle error silently for now
    }
  };

  if (loading) {
    return (
      <div className="max-w-6xl mx-auto px-4 py-8">
        <div className="animate-pulse space-y-6">
          <div className="h-32 rounded-xl bg-surface-100" />
          <div className="grid grid-cols-2 sm:grid-cols-4 gap-4">
            {[1, 2, 3, 4].map(i => (
              <div key={i} className="h-20 rounded-lg bg-surface-100" />
            ))}
          </div>
          <div className="h-64 rounded-xl bg-surface-100" />
        </div>
      </div>
    );
  }

  if (error || !portfolio) {
    return (
      <div className="max-w-6xl mx-auto px-4 py-8 text-center">
        <p className="text-surface-800/50">Failed to load portfolio: {error ?? "Unknown error"}</p>
      </div>
    );
  }

  const displayName = portfolio.displayName ?? (providerId.length > 16 ? `${providerId.slice(0, 12)}...${providerId.slice(-6)}` : providerId);

  return (
    <div className="max-w-6xl mx-auto px-4 py-8">
      {/* Back link */}
      <Link
        href="/providers"
        className="inline-flex items-center gap-1.5 text-sm text-surface-800/50 hover:text-surface-800/80 mb-6 transition-colors"
      >
        <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
          <path d="m12 19-7-7 7-7" />
          <path d="M19 12H5" />
        </svg>
        Back to Providers
      </Link>

      {/* Profile Header */}
      <div className="rounded-xl border border-surface-200 bg-surface-0 p-6 mb-6">
        <div className="flex flex-col sm:flex-row sm:items-start gap-4">
          <div className="flex items-center justify-center h-20 w-20 rounded-full bg-brand-100 text-brand-700 text-3xl font-bold shrink-0">
            {displayName.slice(0, 2).toUpperCase()}
          </div>
          <div className="flex-1 min-w-0">
            <div className="flex items-center gap-2 flex-wrap mb-1">
              <h1 className="text-2xl font-bold text-surface-900">{displayName}</h1>
              {portfolio.certifications.find(c => c.label === "Verified Provider") && (
                <span className="inline-flex items-center gap-1 rounded-full bg-blue-100 px-2 py-0.5 text-xs font-medium text-blue-700">
                  \u2705 Verified
                </span>
              )}
            </div>
            {portfolio.bio && (
              <p className="text-sm text-surface-800/60 mb-3">{portfolio.bio}</p>
            )}
            <div className="flex items-center gap-4 text-sm text-surface-800/60 flex-wrap">
              <span className="flex items-center gap-1">
                <StarRating rating={Math.round(portfolio.stats.avgRating)} />
                <span className="font-medium">{portfolio.stats.avgRating.toFixed(1)}</span>
              </span>
              <span className="text-surface-800/30">|</span>
              <span>Joined {portfolio.joinedDate}</span>
              {portfolio.website && (
                <>
                  <span className="text-surface-800/30">|</span>
                  <a href={portfolio.website} target="_blank" rel="noopener noreferrer" className="text-brand-600 hover:text-brand-700 transition-colors">
                    Website
                  </a>
                </>
              )}
              {portfolio.socialLinks?.length ? (
                portfolio.socialLinks.map(link => (
                  <span key={link.platform}>
                    <span className="text-surface-800/30">|</span>
                    <a href={link.url} target="_blank" rel="noopener noreferrer" className="text-brand-600 hover:text-brand-700 transition-colors capitalize">
                      {link.platform}
                    </a>
                  </span>
                ))
              ) : null}
            </div>
          </div>

          {/* Action buttons */}
          <div className="flex gap-2 shrink-0">
            {isOwner && (
              <button
                onClick={() => setShowEditModal(true)}
                className="rounded-lg border border-surface-200 px-3 py-2 text-xs font-medium text-surface-800/60 hover:bg-surface-50 transition-colors"
              >
                Edit Portfolio
              </button>
            )}
            <button className="rounded-lg bg-brand-600 px-3 py-2 text-xs font-medium text-white hover:bg-brand-700 transition-colors">
              Contact
            </button>
          </div>
        </div>
      </div>

      {/* Stats Grid */}
      <div className="grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-5 gap-4 mb-6">
        <StatCard label="Total Models" value={String(portfolio.stats.totalModels)} icon="\uD83D\uDCE6" />
        <StatCard label="Total Requests" value={formatNumber(portfolio.stats.totalRequests)} icon="\uD83D\uDCCA" />
        <StatCard label="Uptime" value={`${portfolio.stats.uptimePct}%`} icon="\u2B50" valueColor="text-green-600" />
        <StatCard label="Rating" value={`${portfolio.stats.avgRating}/5`} icon="\u2B50" valueColor="text-amber-600" />
        <StatCard label="Revenue" value={formatNanoErg(portfolio.stats.totalRevenue)} icon="\uD83D\uDCB0" />
      </div>

      {/* Social Proof */}
      <div className="rounded-lg border border-surface-200 bg-surface-0 p-4 mb-6">
        <div className="flex items-center gap-6 text-sm text-surface-800/60">
          <span className="flex items-center gap-1.5">
            <span className="text-base">\uD83D\uDC65</span>
            <span><strong className="text-surface-900">{formatNumber(portfolio.stats.totalUsersServed)}</strong> users served</span>
          </span>
          <span className="flex items-center gap-1.5">
            <span className="text-base">\uD83D\uDD04</span>
            <span><strong className="text-surface-900">{formatNumber(portfolio.stats.repeatCustomers)}</strong> repeat customers</span>
          </span>
        </div>
      </div>

      {/* Skills / Expertise Tags */}
      {portfolio.skills.length > 0 && (
        <div className="mb-6">
          <h2 className="text-sm font-semibold text-surface-900 mb-3">Skills & Expertise</h2>
          <div className="flex flex-wrap gap-2">
            {portfolio.skills.map(skill => (
              <span
                key={skill.id}
                className={cn(
                  "inline-flex items-center rounded-full px-3 py-1 text-xs font-medium",
                  CATEGORY_COLORS[skill.category] ?? "bg-surface-100 text-surface-800/60",
                )}
              >
                {skill.label}
              </span>
            ))}
          </div>
        </div>
      )}

      {/* Tab Navigation */}
      <div className="border-b border-surface-200 mb-6">
        <nav className="flex gap-6 -mb-px">
          {(["overview", "models", "performance", "reviews"] as const).map(tab => (
            <button
              key={tab}
              onClick={() => setActiveTab(tab)}
              className={cn(
                "pb-3 text-sm font-medium capitalize transition-colors border-b-2",
                activeTab === tab
                  ? "border-brand-600 text-brand-600"
                  : "border-transparent text-surface-800/50 hover:text-surface-800/70",
              )}
            >
              {tab}
              {tab === "models" && <span className="ml-1.5 text-xs text-surface-800/30">({portfolio.models.length})</span>}
              {tab === "reviews" && <span className="ml-1.5 text-xs text-surface-800/30">({portfolio.reviews.length})</span>}
            </button>
          ))}
        </nav>
      </div>

      {/* Tab Content */}
      {activeTab === "overview" && (
        <div className="grid gap-6 lg:grid-cols-2">
          {/* Certifications & Badges */}
          <section>
            <h2 className="text-lg font-semibold text-surface-900 mb-4">Certifications & Badges</h2>
            <div className="grid gap-3 sm:grid-cols-2">
              {portfolio.certifications.map(cert => (
                <CertBadge key={cert.id} cert={cert} />
              ))}
            </div>
          </section>

          {/* Activity Feed */}
          <section>
            <h2 className="text-lg font-semibold text-surface-900 mb-4">Recent Activity</h2>
            <ActivityTimeline events={portfolio.activity.slice(0, 6)} />
          </section>
        </div>
      )}

      {activeTab === "models" && (
        <section>
          <h2 className="text-lg font-semibold text-surface-900 mb-4">Model Showcase</h2>
          {portfolio.models.length === 0 ? (
            <div className="text-sm text-surface-800/40 py-8 text-center">
              No models currently listed.
            </div>
          ) : (
            <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
              {portfolio.models.map(model => (
                <ModelShowcaseCard key={model.id} model={model} />
              ))}
            </div>
          )}
        </section>
      )}

      {activeTab === "performance" && (
        <section>
          <h2 className="text-lg font-semibold text-surface-900 mb-4">Performance</h2>
          <div className="grid gap-6 sm:grid-cols-3">
            <SparklineChart data={portfolio.performanceHistory.requests} color="text-brand-500" label="Requests Over Time" height={60} />
            <SparklineChart data={portfolio.performanceHistory.latency} color="text-amber-500" label="Latency Trends (ms)" height={60} />
            <SparklineChart data={portfolio.performanceHistory.availability} color="text-green-500" label="Availability %" height={60} />
          </div>
        </section>
      )}

      {activeTab === "reviews" && (
        <section>
          <h2 className="text-lg font-semibold text-surface-900 mb-4">Reviews</h2>
          {portfolio.reviews.length === 0 ? (
            <div className="text-sm text-surface-800/40 py-8 text-center">
              No reviews yet.
            </div>
          ) : (
            <div className="space-y-4">
              {portfolio.reviews.map(review => (
                <ReviewCard key={review.id} review={review} />
              ))}
            </div>
          )}
        </section>
      )}

      {/* Edit Modal */}
      {showEditModal && (
        <EditPortfolioModal
          portfolio={portfolio}
          onSave={handleSave}
          onClose={() => setShowEditModal(false)}
        />
      )}
    </div>
  );
}
