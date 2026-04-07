"use client";

import { useState, useEffect } from "react";
import { useAuthStore } from "@/lib/stores/auth";
import { toast } from "sonner";
import Link from "next/link";

export default function SettingsPage() {
  const user = useAuthStore((s) => s.user);
  const refreshBalance = useAuthStore((s) => s.refreshBalance);

  const [displayName, setDisplayName] = useState("");
  const [bio, setBio] = useState("");
  const [publicProfile, setPublicProfile] = useState(true);
  const [region, setRegion] = useState("us-east");
  const [timezone, setTimezone] = useState("UTC");
  const [saving, setSaving] = useState(false);

  // Load saved profile data
  useEffect(() => {
    try {
      const saved = localStorage.getItem("xergon-profile");
      if (saved) {
        const data = JSON.parse(saved);
        setDisplayName(data.displayName || "");
        setBio(data.bio || "");
        setPublicProfile(data.publicProfile ?? true);
        setRegion(data.region || "us-east");
        setTimezone(data.timezone || "UTC");
      }
    } catch {
      // use defaults
    }
  }, []);

  const handleSave = async () => {
    setSaving(true);
    // Simulate API call
    await new Promise((r) => setTimeout(r, 600));

    localStorage.setItem("xergon-profile", JSON.stringify({
      displayName,
      bio,
      publicProfile,
      region,
      timezone,
    }));

    setSaving(false);
    toast.success("Profile saved");
  };

  function truncateAddress(addr: string) {
    if (addr.length <= 16) return addr;
    return `${addr.slice(0, 10)}...${addr.slice(-4)}`;
  }

  if (!user) return null;

  return (
    <div className="space-y-6">
      {/* Profile Settings */}
      <section className="rounded-xl border border-surface-200 bg-surface-0 p-6">
        <h2 className="font-semibold mb-4">Profile</h2>
        <div className="space-y-4">
          {/* Avatar */}
          <div className="flex items-center gap-4">
            <div className="h-16 w-16 rounded-full bg-brand-100 flex items-center justify-center text-brand-600 font-bold text-2xl dark:bg-brand-900/30">
              {(displayName || user.publicKey.slice(0, 2)).charAt(0).toUpperCase()}
            </div>
            <div>
              <button
                className="px-3 py-1.5 text-sm font-medium rounded-lg border border-surface-200 text-surface-800 hover:bg-surface-50 transition-colors"
              >
                Upload Avatar
              </button>
              <p className="text-xs text-surface-800/40 mt-1">JPG, PNG or GIF. Max 2MB.</p>
            </div>
          </div>

          {/* Display name */}
          <div>
            <label className="block text-sm font-medium text-surface-900 mb-1">Display Name</label>
            <input
              type="text"
              value={displayName}
              onChange={(e) => setDisplayName(e.target.value)}
              placeholder="Enter a display name"
              className="w-full px-3 py-2 text-sm rounded-lg border border-surface-200 bg-surface-0 focus:outline-none focus:ring-2 focus:ring-brand-500/30"
            />
          </div>

          {/* Bio */}
          <div>
            <label className="block text-sm font-medium text-surface-900 mb-1">Bio</label>
            <textarea
              value={bio}
              onChange={(e) => setBio(e.target.value)}
              placeholder="Tell us about yourself..."
              rows={3}
              className="w-full px-3 py-2 text-sm rounded-lg border border-surface-200 bg-surface-0 focus:outline-none focus:ring-2 focus:ring-brand-500/30 resize-none"
            />
          </div>

          {/* Public profile toggle */}
          <div className="flex items-center justify-between">
            <div>
              <p className="text-sm font-medium text-surface-900">Public Profile</p>
              <p className="text-xs text-surface-800/40">Allow others to see your profile on the marketplace</p>
            </div>
            <button
              onClick={() => setPublicProfile(!publicProfile)}
              className={`relative inline-flex h-6 w-11 shrink-0 rounded-full border-2 border-transparent transition-colors cursor-pointer ${
                publicProfile ? "bg-brand-600" : "bg-surface-300 dark:bg-surface-600"
              }`}
              role="switch"
              aria-checked={publicProfile}
            >
              <span className={`pointer-events-none inline-block h-5 w-5 rounded-full bg-white shadow-lg ring-0 transition-transform ${
                publicProfile ? "translate-x-5" : "translate-x-0"
              }`} />
            </button>
          </div>

          {/* Region */}
          <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
            <div>
              <label className="block text-sm font-medium text-surface-900 mb-1">Region</label>
              <select
                value={region}
                onChange={(e) => setRegion(e.target.value)}
                className="w-full px-3 py-2 text-sm rounded-lg border border-surface-200 bg-surface-0 focus:outline-none focus:ring-2 focus:ring-brand-500/30"
              >
                <option value="us-east">US East</option>
                <option value="us-west">US West</option>
                <option value="eu-west">EU West</option>
                <option value="eu-central">EU Central</option>
                <option value="ap-east">Asia Pacific</option>
                <option value="ap-southeast">Southeast Asia</option>
              </select>
            </div>
            <div>
              <label className="block text-sm font-medium text-surface-900 mb-1">Timezone</label>
              <select
                value={timezone}
                onChange={(e) => setTimezone(e.target.value)}
                className="w-full px-3 py-2 text-sm rounded-lg border border-surface-200 bg-surface-0 focus:outline-none focus:ring-2 focus:ring-brand-500/30"
              >
                <option value="UTC">UTC</option>
                <option value="US/Eastern">US/Eastern</option>
                <option value="US/Pacific">US/Pacific</option>
                <option value="Europe/London">Europe/London</option>
                <option value="Europe/Berlin">Europe/Berlin</option>
                <option value="Asia/Tokyo">Asia/Tokyo</option>
                <option value="Asia/Shanghai">Asia/Shanghai</option>
              </select>
            </div>
          </div>

          <button
            onClick={handleSave}
            disabled={saving}
            className="inline-flex items-center px-4 py-2 rounded-lg text-sm font-medium bg-brand-600 text-white hover:bg-brand-700 transition-colors disabled:opacity-50"
          >
            {saving ? "Saving..." : "Save Profile"}
          </button>
        </div>
      </section>

      {/* Wallet Info */}
      <section className="rounded-xl border border-surface-200 bg-surface-0 p-6">
        <h2 className="font-semibold mb-4">Wallet</h2>
        <div className="space-y-4">
          <div>
            <span className="block text-sm text-surface-800/50 mb-1">Ergo Address</span>
            <p className="font-mono text-sm bg-surface-100 rounded-lg px-3 py-2 break-all dark:bg-surface-800">
              {user.ergoAddress}
            </p>
          </div>
          <div>
            <span className="block text-sm text-surface-800/50 mb-1">Public Key</span>
            <p className="font-mono text-sm bg-surface-100 rounded-lg px-3 py-2 break-all dark:bg-surface-800">
              {user.publicKey}
            </p>
          </div>
          <div className="grid grid-cols-2 gap-4 text-sm">
            <div>
              <span className="text-surface-800/50">Balance</span>
              <p className="font-medium">{user.balance.toFixed(4)} ERG</p>
            </div>
            <div>
              <span className="text-surface-800/50">Short Address</span>
              <p className="font-mono font-medium">{truncateAddress(user.ergoAddress)}</p>
            </div>
          </div>
          <button
            onClick={() => {
              refreshBalance();
              toast.success("Balance refreshed");
            }}
            className="rounded-lg bg-brand-600 px-4 py-2 text-sm font-medium text-white transition-colors hover:bg-brand-700"
          >
            Refresh Balance
          </button>
        </div>
      </section>

      {/* Quick links */}
      <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
        <Link
          href="/settings/security"
          className="rounded-xl border border-surface-200 bg-surface-0 p-4 hover:shadow-md transition-shadow"
        >
          <h3 className="text-sm font-semibold text-surface-900 mb-1">Security</h3>
          <p className="text-xs text-surface-800/50">2FA, active sessions, login history</p>
        </Link>
        <Link
          href="/settings/notifications"
          className="rounded-xl border border-surface-200 bg-surface-0 p-4 hover:shadow-md transition-shadow"
        >
          <h3 className="text-sm font-semibold text-surface-900 mb-1">Notifications</h3>
          <p className="text-xs text-surface-800/50">Email digest, notification preferences</p>
        </Link>
        <Link
          href="/settings/preferences"
          className="rounded-xl border border-surface-200 bg-surface-0 p-4 hover:shadow-md transition-shadow"
        >
          <h3 className="text-sm font-semibold text-surface-900 mb-1">Preferences</h3>
          <p className="text-xs text-surface-800/50">Theme, language, default model</p>
        </Link>
        <Link
          href="/settings/api-keys"
          className="rounded-xl border border-surface-200 bg-surface-0 p-4 hover:shadow-md transition-shadow"
        >
          <h3 className="text-sm font-semibold text-surface-900 mb-1">API Keys</h3>
          <p className="text-xs text-surface-800/50">Manage API keys for programmatic access</p>
        </Link>
      </div>
    </div>
  );
}
