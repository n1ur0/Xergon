"use client";

import { useState, useEffect, useCallback } from "react";
import Link from "next/link";
import {
  User,
  Edit3,
  Save,
  X,
  Award,
  DollarSign,
  Activity,
  Zap,
  Clock,
  Star,
  Settings,
  Globe,
  Bell,
  BarChart3,
} from "lucide-react";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface UserProfile {
  address: string;
  displayName: string;
  avatar?: string;
  bio?: string;
  joinedAt: string;
  stats: {
    totalSpentNanoErg: number;
    totalRequests: number;
    totalTokensConsumed: number;
    rentalCount: number;
    activeRentals: number;
    favoriteModels: string[];
    mostUsedProvider?: string;
  };
  reputation: {
    score: number;
    level: "bronze" | "silver" | "gold" | "platinum";
    disputesOpened: number;
    disputesResolved: number;
  };
  preferences: {
    defaultModel: string;
    preferredRegion: string;
    notificationsEnabled: boolean;
  };
}

const LEVEL_COLORS: Record<string, { bg: string; text: string; border: string; ring: string }> = {
  bronze: { bg: "bg-amber-100 dark:bg-amber-900/30", text: "text-amber-700 dark:text-amber-300", border: "border-amber-300 dark:border-amber-700", ring: "ring-amber-500" },
  silver: { bg: "bg-gray-100 dark:bg-gray-800/50", text: "text-gray-600 dark:text-gray-300", border: "border-gray-300 dark:border-gray-600", ring: "ring-gray-400" },
  gold: { bg: "bg-yellow-100 dark:bg-yellow-900/30", text: "text-yellow-700 dark:text-yellow-300", border: "border-yellow-300 dark:border-yellow-700", ring: "ring-yellow-500" },
  platinum: { bg: "bg-purple-100 dark:bg-purple-900/30", text: "text-purple-700 dark:text-purple-300", border: "border-purple-300 dark:border-purple-700", ring: "ring-purple-500" },
};

const MODELS = ["llama-3.1-70b", "qwen2.5-72b", "mistral-7b", "deepseek-coder-33b", "phi-3-medium"];
const REGIONS = ["North America", "Europe", "Asia", "South America", "Oceania"];

function formatNanoErg(n: number): string {
  return `${(n / 1e9).toFixed(2)} ERG`;
}

function formatNumber(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return n.toString();
}

function getInitials(name: string): string {
  return name
    .replace(/_/g, " ")
    .split(" ")
    .map((w) => w[0])
    .filter(Boolean)
    .slice(0, 2)
    .join("")
    .toUpperCase();
}

function truncateAddr(addr: string): string {
  if (addr.length <= 16) return addr;
  return `${addr.slice(0, 10)}...${addr.slice(-4)}`;
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export default function ProfilePage() {
  const [profile, setProfile] = useState<UserProfile | null>(null);
  const [loading, setLoading] = useState(true);
  const [editing, setEditing] = useState(false);
  const [editForm, setEditForm] = useState({ displayName: "", bio: "" });
  const [saving, setSaving] = useState(false);
  const [prefs, setPrefs] = useState({
    defaultModel: "",
    preferredRegion: "",
    notificationsEnabled: true,
  });
  const [activityData, setActivityData] = useState<number[]>([]);

  // Fetch profile
  const fetchProfile = useCallback(async () => {
    try {
      const res = await fetch("/api/user/profile");
      if (res.ok) {
        const data = await res.json();
        setProfile(data);
        setEditForm({ displayName: data.displayName, bio: data.bio || "" });
        setPrefs(data.preferences);
      }
    } catch {
      // Silently fail
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchProfile();
  }, [fetchProfile]);

  // Generate activity data (14 days)
  useEffect(() => {
    const data = Array.from({ length: 14 }, () => Math.floor(Math.random() * 200 + 20));
    setActivityData(data);
  }, []);

  // Save profile
  const handleSave = async () => {
    setSaving(true);
    try {
      await fetch("/api/user/profile", {
        method: "PATCH",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          address: profile?.address,
          displayName: editForm.displayName,
          bio: editForm.bio,
        }),
      });
      setProfile((prev) =>
        prev ? { ...prev, displayName: editForm.displayName, bio: editForm.bio } : prev,
      );
      setEditing(false);
    } catch {
      // Silently fail
    } finally {
      setSaving(false);
    }
  };

  // Save preferences
  const handlePrefChange = async (key: string, value: string | boolean) => {
    const newPrefs = { ...prefs, [key]: value };
    setPrefs(newPrefs);
    try {
      await fetch("/api/user/profile", {
        method: "PATCH",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          address: profile?.address,
          preferences: newPrefs,
        }),
      });
    } catch {
      // Silently fail
    }
  };

  if (loading) {
    return (
      <div className="mx-auto max-w-4xl px-4 py-12">
        <div className="animate-pulse space-y-6">
          <div className="h-40 rounded-2xl bg-surface-200 dark:bg-surface-800" />
          <div className="grid grid-cols-2 gap-4 md:grid-cols-4">
            {Array.from({ length: 4 }).map((_, i) => (
              <div key={i} className="h-24 rounded-xl bg-surface-200 dark:bg-surface-800" />
            ))}
          </div>
        </div>
      </div>
    );
  }

  if (!profile) {
    return (
      <div className="mx-auto max-w-4xl px-4 py-12 text-center">
        <p className="text-surface-800/50">Failed to load profile.</p>
      </div>
    );
  }

  const levelColors = LEVEL_COLORS[profile.reputation.level] || LEVEL_COLORS.bronze;
  const maxActivity = Math.max(...activityData, 1);

  return (
    <div className="mx-auto max-w-4xl px-4 py-8 space-y-8">
      {/* Profile Header */}
      <div className="rounded-2xl border border-surface-200 bg-white p-6 dark:border-surface-700 dark:bg-surface-900">
        <div className="flex flex-col gap-4 sm:flex-row sm:items-start sm:justify-between">
          <div className="flex items-start gap-4">
            {/* Avatar */}
            <div className="flex h-16 w-16 shrink-0 items-center justify-center rounded-full bg-brand-600 text-xl font-bold text-white">
              {profile.avatar ? (
                <img src={profile.avatar} alt="" className="h-16 w-16 rounded-full object-cover" />
              ) : (
                getInitials(profile.displayName)
              )}
            </div>
            <div>
              {editing ? (
                <div className="space-y-2">
                  <input
                    type="text"
                    value={editForm.displayName}
                    onChange={(e) =>
                      setEditForm((f) => ({ ...f, displayName: e.target.value }))
                    }
                    className="rounded-lg border border-surface-300 bg-surface-0 px-3 py-1.5 text-lg font-semibold dark:border-surface-600 dark:bg-surface-800"
                  />
                  <textarea
                    value={editForm.bio}
                    onChange={(e) =>
                      setEditForm((f) => ({ ...f, bio: e.target.value }))
                    }
                    placeholder="Tell us about yourself..."
                    rows={2}
                    className="w-full rounded-lg border border-surface-300 bg-surface-0 px-3 py-1.5 text-sm dark:border-surface-600 dark:bg-surface-800"
                  />
                </div>
              ) : (
                <>
                  <h1 className="text-xl font-bold text-surface-900 dark:text-surface-0">
                    {profile.displayName}
                  </h1>
                  <p className="font-mono text-sm text-surface-800/50">
                    {truncateAddr(profile.address)}
                  </p>
                  {profile.bio && (
                    <p className="mt-1 text-sm text-surface-800/70 dark:text-surface-300/70">
                      {profile.bio}
                    </p>
                  )}
                </>
              )}
              <p className="mt-1 text-xs text-surface-800/40">
                Joined {new Date(profile.joinedAt).toLocaleDateString()}
              </p>
            </div>
          </div>

          <div className="flex items-center gap-3">
            {/* Reputation Badge */}
            <div
              className={`inline-flex items-center gap-1.5 rounded-full border px-3 py-1 text-xs font-medium ${levelColors.bg} ${levelColors.text} ${levelColors.border}`}
            >
              <Award className="h-3.5 w-3.5" />
              {profile.reputation.level.charAt(0).toUpperCase() + profile.reputation.level.slice(1)}
              <span className="ml-1 opacity-70">({profile.reputation.score})</span>
            </div>

            {/* Edit / Save / Cancel */}
            {editing ? (
              <div className="flex gap-2">
                <button
                  onClick={handleSave}
                  disabled={saving}
                  className="inline-flex items-center gap-1.5 rounded-lg bg-brand-600 px-3 py-1.5 text-sm font-medium text-white transition-colors hover:bg-brand-700 disabled:opacity-50"
                >
                  <Save className="h-3.5 w-3.5" />
                  {saving ? "Saving..." : "Save"}
                </button>
                <button
                  onClick={() => {
                    setEditing(false);
                    setEditForm({ displayName: profile.displayName, bio: profile.bio || "" });
                  }}
                  className="inline-flex items-center gap-1.5 rounded-lg border border-surface-300 px-3 py-1.5 text-sm transition-colors hover:bg-surface-100 dark:border-surface-600 dark:hover:bg-surface-800"
                >
                  <X className="h-3.5 w-3.5" />
                  Cancel
                </button>
              </div>
            ) : (
              <button
                onClick={() => setEditing(true)}
                className="inline-flex items-center gap-1.5 rounded-lg border border-surface-300 px-3 py-1.5 text-sm transition-colors hover:bg-surface-100 dark:border-surface-600 dark:hover:bg-surface-800"
              >
                <Edit3 className="h-3.5 w-3.5" />
                Edit Profile
              </button>
            )}
          </div>
        </div>
      </div>

      {/* Stats Grid */}
      <div className="grid grid-cols-2 gap-4 md:grid-cols-5">
        <StatCard
          icon={<DollarSign className="h-5 w-5 text-emerald-500" />}
          label="Total Spent"
          value={formatNanoErg(profile.stats.totalSpentNanoErg)}
        />
        <StatCard
          icon={<Activity className="h-5 w-5 text-blue-500" />}
          label="Total Requests"
          value={formatNumber(profile.stats.totalRequests)}
        />
        <StatCard
          icon={<Zap className="h-5 w-5 text-amber-500" />}
          label="Tokens Consumed"
          value={formatNumber(profile.stats.totalTokensConsumed)}
        />
        <StatCard
          icon={<Clock className="h-5 w-5 text-purple-500" />}
          label="Rentals"
          value={profile.stats.rentalCount.toString()}
        />
        <StatCard
          icon={<Star className="h-5 w-5 text-green-500" />}
          label="Active"
          value={profile.stats.activeRentals.toString()}
        />
      </div>

      {/* Favorite Models */}
      {profile.stats.favoriteModels.length > 0 && (
        <div className="rounded-2xl border border-surface-200 bg-white p-6 dark:border-surface-700 dark:bg-surface-900">
          <h2 className="mb-3 text-sm font-semibold text-surface-800/60 uppercase tracking-wider">
            Favorite Models
          </h2>
          <div className="flex flex-wrap gap-2">
            {profile.stats.favoriteModels.map((model) => (
              <span
                key={model}
                className="inline-flex items-center rounded-full bg-brand-50 px-3 py-1 text-sm font-medium text-brand-700 dark:bg-brand-900/30 dark:text-brand-300"
              >
                {model}
              </span>
            ))}
          </div>
        </div>
      )}

      {/* Quick Links */}
      <div className="grid grid-cols-1 gap-4 sm:grid-cols-3">
        <Link
          href="/profile/rentals"
          className="flex items-center gap-3 rounded-2xl border border-surface-200 bg-white p-4 transition-colors hover:bg-surface-50 dark:border-surface-700 dark:bg-surface-900 dark:hover:bg-surface-800"
        >
          <Clock className="h-5 w-5 text-brand-500" />
          <div>
            <p className="font-medium text-surface-900 dark:text-surface-0">Rental History</p>
            <p className="text-xs text-surface-800/50">View all past rentals</p>
          </div>
        </Link>
        <Link
          href="/profile/notifications"
          className="flex items-center gap-3 rounded-2xl border border-surface-200 bg-white p-4 transition-colors hover:bg-surface-50 dark:border-surface-700 dark:bg-surface-900 dark:hover:bg-surface-800"
        >
          <Bell className="h-5 w-5 text-brand-500" />
          <div>
            <p className="font-medium text-surface-900 dark:text-surface-0">Notifications</p>
            <p className="text-xs text-surface-800/50">Manage alerts</p>
          </div>
        </Link>
        <Link
          href="/settings"
          className="flex items-center gap-3 rounded-2xl border border-surface-200 bg-white p-4 transition-colors hover:bg-surface-50 dark:border-surface-700 dark:bg-surface-900 dark:hover:bg-surface-800"
        >
          <Settings className="h-5 w-5 text-brand-500" />
          <div>
            <p className="font-medium text-surface-900 dark:text-surface-0">Settings</p>
            <p className="text-xs text-surface-800/50">Account settings</p>
          </div>
        </Link>
      </div>

      {/* Preferences */}
      <div className="rounded-2xl border border-surface-200 bg-white p-6 dark:border-surface-700 dark:bg-surface-900">
        <h2 className="mb-4 flex items-center gap-2 text-sm font-semibold text-surface-800/60 uppercase tracking-wider">
          <Settings className="h-4 w-4" />
          Preferences
        </h2>
        <div className="space-y-4">
          <div>
            <label className="mb-1 block text-sm font-medium text-surface-800/70 dark:text-surface-300/70">
              Default Model
            </label>
            <select
              value={prefs.defaultModel}
              onChange={(e) => handlePrefChange("defaultModel", e.target.value)}
              className="w-full rounded-lg border border-surface-300 bg-surface-0 px-3 py-2 text-sm dark:border-surface-600 dark:bg-surface-800"
            >
              {MODELS.map((m) => (
                <option key={m} value={m}>
                  {m}
                </option>
              ))}
            </select>
          </div>
          <div>
            <label className="mb-1 block text-sm font-medium text-surface-800/70 dark:text-surface-300/70">
              <Globe className="mr-1 inline h-3.5 w-3.5" />
              Preferred Region
            </label>
            <select
              value={prefs.preferredRegion}
              onChange={(e) => handlePrefChange("preferredRegion", e.target.value)}
              className="w-full rounded-lg border border-surface-300 bg-surface-0 px-3 py-2 text-sm dark:border-surface-600 dark:bg-surface-800"
            >
              {REGIONS.map((r) => (
                <option key={r} value={r}>
                  {r}
                </option>
              ))}
            </select>
          </div>
          <div className="flex items-center justify-between">
            <label className="text-sm font-medium text-surface-800/70 dark:text-surface-300/70">
              <Bell className="mr-1 inline h-3.5 w-3.5" />
              Enable Notifications
            </label>
            <button
              onClick={() =>
                handlePrefChange("notificationsEnabled", !prefs.notificationsEnabled)
              }
              className={`relative inline-flex h-6 w-11 items-center rounded-full transition-colors ${
                prefs.notificationsEnabled
                  ? "bg-brand-600"
                  : "bg-surface-300 dark:bg-surface-600"
              }`}
            >
              <span
                className={`inline-block h-4 w-4 transform rounded-full bg-white transition-transform ${
                  prefs.notificationsEnabled ? "translate-x-6" : "translate-x-1"
                }`}
              />
            </button>
          </div>
        </div>
      </div>

      {/* Activity Chart (14 days) */}
      <div className="rounded-2xl border border-surface-200 bg-white p-6 dark:border-surface-700 dark:bg-surface-900">
        <h2 className="mb-4 flex items-center gap-2 text-sm font-semibold text-surface-800/60 uppercase tracking-wider">
          <BarChart3 className="h-4 w-4" />
          Activity (Last 14 Days)
        </h2>
        <div className="flex items-end gap-1.5 h-32">
          {activityData.map((val, i) => {
            const pct = (val / maxActivity) * 100;
            return (
              <div
                key={i}
                className="flex-1 flex flex-col items-center gap-1"
              >
                <div
                  className="w-full rounded-t bg-brand-500/80 transition-all hover:bg-brand-600"
                  style={{ height: `${Math.max(pct, 4)}%` }}
                  title={`${val} requests`}
                />
                <span className="text-[10px] text-surface-800/40">
                  {new Date(Date.now() - (13 - i) * 86400000).toLocaleDateString("en", { weekday: "short" }).slice(0, 2)}
                </span>
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// StatCard sub-component
// ---------------------------------------------------------------------------

function StatCard({
  icon,
  label,
  value,
}: {
  icon: React.ReactNode;
  label: string;
  value: string;
}) {
  return (
    <div className="rounded-xl border border-surface-200 bg-white p-4 dark:border-surface-700 dark:bg-surface-900">
      <div className="mb-2">{icon}</div>
      <p className="text-lg font-bold text-surface-900 dark:text-surface-0">{value}</p>
      <p className="text-xs text-surface-800/50">{label}</p>
    </div>
  );
}
