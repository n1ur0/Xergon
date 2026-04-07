"use client";

import { useState, useMemo } from "react";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

type UserStatus = "active" | "suspended" | "banned";
type UserTier = "free" | "basic" | "pro" | "enterprise";

interface UserRecord {
  id: string;
  displayName: string;
  email: string;
  publicKey: string;
  status: UserStatus;
  tier: UserTier;
  joinedAt: string;
  lastActiveAt: string;
  rentalCount: number;
  totalSpentNanoErg: number;
  flagsReceived: number;
}

interface ActivityEntry {
  id: string;
  type: "rental" | "review" | "payment" | "dispute" | "login";
  description: string;
  timestamp: string;
}

// ---------------------------------------------------------------------------
// Mock data
// ---------------------------------------------------------------------------

const MOCK_USERS: UserRecord[] = [
  {
    id: "u1",
    displayName: "Alice Berg",
    email: "alice@example.com",
    publicKey: "9f2a1b3c4d5e6f7a8b9c0d1e2f3a4b5c6d7e8f9a0b1c2d3e4f5a6b7c8d9e0f1a2",
    status: "active",
    tier: "pro",
    joinedAt: "2025-12-01T00:00:00Z",
    lastActiveAt: "2026-04-05T09:00:00Z",
    rentalCount: 42,
    totalSpentNanoErg: 15_000_000_000,
    flagsReceived: 0,
  },
  {
    id: "u2",
    displayName: "Bob Node",
    email: "bob@provider.io",
    publicKey: "1a2b3c4d5e6f7a8b9c0d1e2f3a4b5c6d7e8f9a0b1c2d3e4f5a6b7c8d9e0f1a2",
    status: "active",
    tier: "enterprise",
    joinedAt: "2025-10-15T00:00:00Z",
    lastActiveAt: "2026-04-05T08:30:00Z",
    rentalCount: 0,
    totalSpentNanoErg: 0,
    flagsReceived: 1,
  },
  {
    id: "u3",
    displayName: "Carol Miner",
    email: "carol@ergo.org",
    publicKey: "a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1b2",
    status: "suspended",
    tier: "basic",
    joinedAt: "2026-01-10T00:00:00Z",
    lastActiveAt: "2026-03-20T14:00:00Z",
    rentalCount: 5,
    totalSpentNanoErg: 2_500_000_000,
    flagsReceived: 3,
  },
  {
    id: "u4",
    displayName: "Dave Suspicious",
    email: "dave@spam.net",
    publicKey: "f1e2d3c4b5a6f7e8d9c0b1a2f3e4d5c6b7a8f9e0d1c2b3a4f5e6d7c8b9a0f1e2",
    status: "banned",
    tier: "free",
    joinedAt: "2026-03-01T00:00:00Z",
    lastActiveAt: "2026-03-15T10:00:00Z",
    rentalCount: 1,
    totalSpentNanoErg: 100_000_000,
    flagsReceived: 8,
  },
  {
    id: "u5",
    displayName: "Eve Validator",
    email: "eve@xergon.network",
    publicKey: "c1d2e3f4a5b6c7d8e9f0a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2",
    status: "active",
    tier: "pro",
    joinedAt: "2025-11-20T00:00:00Z",
    lastActiveAt: "2026-04-04T22:00:00Z",
    rentalCount: 18,
    totalSpentNanoErg: 8_000_000_000,
    flagsReceived: 0,
  },
];

const MOCK_ACTIVITY: ActivityEntry[] = [
  { id: "a1", type: "rental", description: "Rented llama-3.1-70b from ProviderX", timestamp: "2026-04-05T09:00:00Z" },
  { id: "a2", type: "payment", description: "Paid 0.5 ERG for rental #1042", timestamp: "2026-04-05T08:55:00Z" },
  { id: "a3", type: "login", description: "Logged in from 192.168.1.1", timestamp: "2026-04-05T08:50:00Z" },
  { id: "a4", type: "review", description: "Left a 5-star review for ProviderX", timestamp: "2026-04-04T20:00:00Z" },
  { id: "a5", type: "dispute", description: "Opened dispute #D78 for rental #1039", timestamp: "2026-04-03T15:00:00Z" },
];

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function nanoergToErg(n: number): string {
  return `${(n / 1e9).toFixed(2)} ERG`;
}

function truncateKey(key: string): string {
  if (key.length <= 16) return key;
  return `${key.slice(0, 8)}...${key.slice(-6)}`;
}

const STATUS_STYLES: Record<UserStatus, string> = {
  active: "bg-green-100 text-green-800 dark:bg-green-900/30 dark:text-green-300",
  suspended: "bg-amber-100 text-amber-800 dark:bg-amber-900/30 dark:text-amber-300",
  banned: "bg-red-100 text-red-800 dark:bg-red-900/30 dark:text-red-300",
};

const TIER_STYLES: Record<UserTier, string> = {
  free: "bg-surface-100 text-surface-800/60 dark:bg-surface-800 dark:text-surface-400",
  basic: "bg-blue-100 text-blue-800 dark:bg-blue-900/30 dark:text-blue-300",
  pro: "bg-purple-100 text-purple-800 dark:bg-purple-900/30 dark:text-purple-300",
  enterprise: "bg-amber-100 text-amber-800 dark:bg-amber-900/30 dark:text-amber-300",
};

const ACTIVITY_ICONS: Record<ActivityEntry["type"], string> = {
  rental: "📦",
  payment: "💳",
  login: "🔑",
  review: "⭐",
  dispute: "⚠️",
};

const TIERS: UserTier[] = ["free", "basic", "pro", "enterprise"];

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function UserManagement() {
  const [users, setUsers] = useState<UserRecord[]>(MOCK_USERS);
  const [search, setSearch] = useState("");
  const [drawerUser, setDrawerUser] = useState<UserRecord | null>(null);

  const filtered = useMemo(() => {
    const q = search.toLowerCase();
    if (!q) return users;
    return users.filter(
      (u) =>
        u.displayName.toLowerCase().includes(q) ||
        u.email.toLowerCase().includes(q) ||
        u.publicKey.toLowerCase().includes(q),
    );
  }, [users, search]);

  const updateUser = (id: string, updates: Partial<UserRecord>) => {
    setUsers((prev) => prev.map((u) => (u.id === id ? { ...u, ...updates } : u)));
    if (drawerUser?.id === id) {
      setDrawerUser((prev) => (prev ? { ...prev, ...updates } : null));
    }
  };

  const handleBan = (id: string) => updateUser(id, { status: "banned" });
  const handleSuspend = (id: string) => updateUser(id, { status: "suspended" });
  const handleReactivate = (id: string) => updateUser(id, { status: "active" });
  const handleResetRateLimits = (id: string) => {
    // In production this would call /api/admin/users/[id]/reset-rate-limits
    alert(`Rate limits reset for user ${id}`);
  };
  const handleChangeTier = (id: string, tier: UserTier) => updateUser(id, { tier });

  return (
    <div className="space-y-4">
      {/* Header */}
      <div className="flex items-center justify-between flex-wrap gap-3">
        <div>
          <h2 className="text-lg font-semibold text-surface-900">User Management</h2>
          <p className="text-sm text-surface-800/50">{users.length} total users</p>
        </div>
        <input
          type="text"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          placeholder="Search by name, email, or public key..."
          className="px-3 py-1.5 text-sm rounded-lg border border-surface-200 bg-surface-0 focus:outline-none focus:ring-2 focus:ring-brand-500/30 w-72"
        />
      </div>

      {/* User table */}
      <div className="rounded-xl border border-surface-200 bg-surface-0 overflow-hidden">
        <div className="overflow-x-auto">
          <table className="w-full text-sm">
            <thead>
              <tr className="border-b border-surface-200 bg-surface-50 dark:bg-surface-800/50">
                <th className="text-left px-4 py-2.5 font-medium text-surface-800/60">User</th>
                <th className="text-left px-4 py-2.5 font-medium text-surface-800/60">Status</th>
                <th className="text-left px-4 py-2.5 font-medium text-surface-800/60">Tier</th>
                <th className="text-left px-4 py-2.5 font-medium text-surface-800/60">Rentals</th>
                <th className="text-left px-4 py-2.5 font-medium text-surface-800/60">Spent</th>
                <th className="text-left px-4 py-2.5 font-medium text-surface-800/60">Flags</th>
                <th className="text-right px-4 py-2.5 font-medium text-surface-800/60">Actions</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-surface-100 dark:divide-surface-800">
              {filtered.map((user) => (
                <tr key={user.id} className="hover:bg-surface-50/50 dark:hover:bg-surface-800/30 transition-colors">
                  <td className="px-4 py-3">
                    <div className="font-medium text-surface-900">{user.displayName}</div>
                    <div className="text-xs text-surface-800/50">{user.email}</div>
                    <div className="text-xs font-mono text-surface-800/40">{truncateKey(user.publicKey)}</div>
                  </td>
                  <td className="px-4 py-3">
                    <span className={`inline-flex items-center px-2 py-0.5 rounded-md text-xs font-medium ${STATUS_STYLES[user.status]}`}>
                      {user.status}
                    </span>
                  </td>
                  <td className="px-4 py-3">
                    <span className={`inline-flex items-center px-2 py-0.5 rounded-md text-xs font-medium ${TIER_STYLES[user.tier]}`}>
                      {user.tier}
                    </span>
                  </td>
                  <td className="px-4 py-3 text-surface-800/70">{user.rentalCount}</td>
                  <td className="px-4 py-3 text-surface-800/70">{nanoergToErg(user.totalSpentNanoErg)}</td>
                  <td className="px-4 py-3">
                    <span className={user.flagsReceived > 0 ? "text-red-600 font-medium" : "text-surface-800/40"}>
                      {user.flagsReceived}
                    </span>
                  </td>
                  <td className="px-4 py-3 text-right">
                    <div className="flex items-center justify-end gap-1.5">
                      <button
                        onClick={() => setDrawerUser(user)}
                        className="px-2.5 py-1 text-xs font-medium rounded-md text-brand-600 hover:bg-brand-50 transition-colors dark:hover:bg-brand-900/10"
                      >
                        Profile
                      </button>
                      {user.status === "active" ? (
                        <button
                          onClick={() => handleSuspend(user.id)}
                          className="px-2.5 py-1 text-xs font-medium rounded-md bg-amber-100 text-amber-800 hover:bg-amber-200 transition-colors dark:bg-amber-900/30 dark:text-amber-300 dark:hover:bg-amber-900/50"
                        >
                          Suspend
                        </button>
                      ) : (
                        <button
                          onClick={() => handleReactivate(user.id)}
                          className="px-2.5 py-1 text-xs font-medium rounded-md bg-green-100 text-green-800 hover:bg-green-200 transition-colors dark:bg-green-900/30 dark:text-green-300 dark:hover:bg-green-900/50"
                        >
                          Reactivate
                        </button>
                      )}
                      <button
                        onClick={() => handleBan(user.id)}
                        className="px-2.5 py-1 text-xs font-medium rounded-md bg-red-100 text-red-800 hover:bg-red-200 transition-colors dark:bg-red-900/30 dark:text-red-300 dark:hover:bg-red-900/50"
                      >
                        Ban
                      </button>
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>

        {filtered.length === 0 && (
          <div className="text-center py-12 text-surface-800/40">
            No users match your search.
          </div>
        )}
      </div>

      {/* User detail drawer */}
      {drawerUser && (
        <div className="fixed inset-0 z-50 flex justify-end">
          <div
            className="absolute inset-0 bg-black/30"
            onClick={() => setDrawerUser(null)}
          />
          <div className="relative w-full max-w-md bg-surface-0 shadow-xl overflow-y-auto">
            {/* Drawer header */}
            <div className="sticky top-0 z-10 flex items-center justify-between border-b border-surface-200 bg-surface-0 px-6 py-4">
              <h3 className="text-lg font-semibold text-surface-900">User Profile</h3>
              <button
                onClick={() => setDrawerUser(null)}
                className="rounded-lg p-1.5 text-surface-800/50 hover:bg-surface-100 transition-colors"
              >
                <svg className="w-5 h-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                  <line x1="18" y1="6" x2="6" y2="18" />
                  <line x1="6" y1="6" x2="18" y2="18" />
                </svg>
              </button>
            </div>

            <div className="p-6 space-y-6">
              {/* User info */}
              <div className="space-y-3">
                <div className="flex items-center gap-3">
                  <div className="h-12 w-12 rounded-full bg-brand-100 flex items-center justify-center text-brand-600 font-bold text-lg">
                    {drawerUser.displayName.charAt(0)}
                  </div>
                  <div>
                    <p className="font-semibold text-surface-900">{drawerUser.displayName}</p>
                    <p className="text-sm text-surface-800/50">{drawerUser.email}</p>
                  </div>
                </div>
                <div className="flex items-center gap-2">
                  <span className={`inline-flex items-center px-2 py-0.5 rounded-md text-xs font-medium ${STATUS_STYLES[drawerUser.status]}`}>
                    {drawerUser.status}
                  </span>
                  <span className={`inline-flex items-center px-2 py-0.5 rounded-md text-xs font-medium ${TIER_STYLES[drawerUser.tier]}`}>
                    {drawerUser.tier}
                  </span>
                </div>
              </div>

              {/* Details */}
              <div className="space-y-2">
                <h4 className="text-sm font-semibold text-surface-900">Details</h4>
                <div className="grid grid-cols-2 gap-3 text-sm">
                  <div>
                    <span className="text-surface-800/50">Public Key</span>
                    <p className="font-mono text-xs break-all mt-0.5">{drawerUser.publicKey}</p>
                  </div>
                  <div>
                    <span className="text-surface-800/50">Joined</span>
                    <p className="mt-0.5">{new Date(drawerUser.joinedAt).toLocaleDateString()}</p>
                  </div>
                  <div>
                    <span className="text-surface-800/50">Last Active</span>
                    <p className="mt-0.5">{new Date(drawerUser.lastActiveAt).toLocaleDateString()}</p>
                  </div>
                  <div>
                    <span className="text-surface-800/50">Total Rentals</span>
                    <p className="mt-0.5 font-medium">{drawerUser.rentalCount}</p>
                  </div>
                  <div>
                    <span className="text-surface-800/50">Total Spent</span>
                    <p className="mt-0.5 font-medium">{nanoergToErg(drawerUser.totalSpentNanoErg)}</p>
                  </div>
                  <div>
                    <span className="text-surface-800/50">Flags Received</span>
                    <p className={`mt-0.5 font-medium ${drawerUser.flagsReceived > 0 ? "text-red-600" : ""}`}>
                      {drawerUser.flagsReceived}
                    </p>
                  </div>
                </div>
              </div>

              {/* Admin actions */}
              <div className="space-y-3">
                <h4 className="text-sm font-semibold text-surface-900">Admin Actions</h4>
                <div className="flex items-center gap-2">
                  <button
                    onClick={() => handleResetRateLimits(drawerUser.id)}
                    className="px-3 py-1.5 text-xs font-medium rounded-md border border-surface-200 text-surface-800 hover:bg-surface-50 transition-colors"
                  >
                    Reset Rate Limits
                  </button>
                  <button
                    onClick={() => handleBan(drawerUser.id)}
                    className="px-3 py-1.5 text-xs font-medium rounded-md bg-red-600 text-white hover:bg-red-700 transition-colors"
                  >
                    Ban User
                  </button>
                </div>
                <div className="flex items-center gap-2">
                  <label className="text-xs text-surface-800/50">Change Tier:</label>
                  <select
                    value={drawerUser.tier}
                    onChange={(e) => handleChangeTier(drawerUser.id, e.target.value as UserTier)}
                    className="px-2 py-1 text-xs rounded-md border border-surface-200 bg-surface-0 focus:outline-none focus:ring-2 focus:ring-brand-500/30"
                  >
                    {TIERS.map((t) => (
                      <option key={t} value={t}>{t}</option>
                    ))}
                  </select>
                </div>
              </div>

              {/* Activity history */}
              <div className="space-y-3">
                <h4 className="text-sm font-semibold text-surface-900">Recent Activity</h4>
                <div className="space-y-2">
                  {MOCK_ACTIVITY.map((entry) => (
                    <div key={entry.id} className="flex items-start gap-2.5 text-sm">
                      <span className="mt-0.5 text-base">{ACTIVITY_ICONS[entry.type]}</span>
                      <div className="flex-1 min-w-0">
                        <p className="text-surface-800/80">{entry.description}</p>
                        <p className="text-xs text-surface-800/40">
                          {new Date(entry.timestamp).toLocaleString()}
                        </p>
                      </div>
                    </div>
                  ))}
                </div>
              </div>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
