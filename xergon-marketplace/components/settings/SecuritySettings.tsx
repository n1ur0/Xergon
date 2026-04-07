"use client";

import { useState, useEffect, useCallback } from "react";
import { toast } from "sonner";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface Session {
  id: string;
  device: string;
  browser: string;
  ip: string;
  location: string;
  current: boolean;
  lastActive: string;
  createdAt: string;
}

interface LoginEntry {
  id: string;
  ip: string;
  location: string;
  device: string;
  timestamp: string;
  success: boolean;
}

// ---------------------------------------------------------------------------
// Mock data
// ---------------------------------------------------------------------------

const MOCK_SESSIONS: Session[] = [
  {
    id: "s1",
    device: "Desktop",
    browser: "Chrome 123",
    ip: "192.168.1.100",
    location: "San Francisco, US",
    current: true,
    lastActive: "2026-04-05T09:00:00Z",
    createdAt: "2026-03-15T10:00:00Z",
  },
  {
    id: "s2",
    device: "Mobile",
    browser: "Safari Mobile",
    ip: "10.0.0.50",
    location: "San Francisco, US",
    current: false,
    lastActive: "2026-04-04T18:00:00Z",
    createdAt: "2026-04-01T08:00:00Z",
  },
  {
    id: "s3",
    device: "Desktop",
    browser: "Firefox 124",
    ip: "172.16.0.10",
    location: "New York, US",
    current: false,
    lastActive: "2026-04-03T12:00:00Z",
    createdAt: "2026-03-20T14:00:00Z",
  },
];

const MOCK_LOGIN_HISTORY: LoginEntry[] = [
  { id: "l1", ip: "192.168.1.100", location: "San Francisco, US", device: "Chrome 123 / Desktop", timestamp: "2026-04-05T09:00:00Z", success: true },
  { id: "l2", ip: "10.0.0.50", location: "San Francisco, US", device: "Safari Mobile / iPhone", timestamp: "2026-04-04T18:00:00Z", success: true },
  { id: "l3", ip: "203.0.113.42", location: "Unknown", device: "Chrome 123 / Desktop", timestamp: "2026-04-04T03:00:00Z", success: false },
  { id: "l4", ip: "172.16.0.10", location: "New York, US", device: "Firefox 124 / Desktop", timestamp: "2026-04-03T12:00:00Z", success: true },
  { id: "l5", ip: "192.168.1.100", location: "San Francisco, US", device: "Chrome 123 / Desktop", timestamp: "2026-04-02T10:00:00Z", success: true },
];

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function SecuritySettings() {
  const [twoFactorEnabled, setTwoFactorEnabled] = useState(false);
  const [show2faSetup, setShow2faSetup] = useState(false);
  const [sessions, setSessions] = useState<Session[]>(MOCK_SESSIONS);
  const [loginHistory] = useState<LoginEntry[]>(MOCK_LOGIN_HISTORY);
  const [currentPassword, setCurrentPassword] = useState("");
  const [newPassword, setNewPassword] = useState("");
  const [confirmPassword, setConfirmPassword] = useState("");
  const [changingPassword, setChangingPassword] = useState(false);

  useEffect(() => {
    try {
      const saved = localStorage.getItem("xergon-security");
      if (saved) {
        const data = JSON.parse(saved);
        setTwoFactorEnabled(data.twoFactorEnabled ?? false);
      }
    } catch {
      // use defaults
    }
  }, []);

  const save2fa = useCallback((enabled: boolean) => {
    setTwoFactorEnabled(enabled);
    localStorage.setItem("xergon-security", JSON.stringify({ twoFactorEnabled: enabled }));
    toast.success(enabled ? "Two-factor authentication enabled" : "Two-factor authentication disabled");
    setShow2faSetup(false);
  }, []);

  const revokeSession = (id: string) => {
    setSessions((prev) => prev.filter((s) => s.id !== id));
    toast.success("Session revoked");
  };

  const handleChangePassword = async () => {
    if (!currentPassword || !newPassword || !confirmPassword) {
      toast.error("All fields are required");
      return;
    }
    if (newPassword !== confirmPassword) {
      toast.error("New passwords do not match");
      return;
    }
    if (newPassword.length < 8) {
      toast.error("Password must be at least 8 characters");
      return;
    }

    setChangingPassword(true);
    await new Promise((r) => setTimeout(r, 800));
    setChangingPassword(false);
    setCurrentPassword("");
    setNewPassword("");
    setConfirmPassword("");
    toast.success("Password updated successfully");
  };

  return (
    <div className="space-y-6">
      {/* Two-factor authentication */}
      <section className="rounded-xl border border-surface-200 bg-surface-0 p-6">
        <div className="flex items-center justify-between mb-4">
          <div>
            <h2 className="font-semibold">Two-Factor Authentication</h2>
            <p className="text-sm text-surface-800/50 mt-0.5">
              Add an extra layer of security to your account
            </p>
          </div>
          <span className={`inline-flex items-center px-2.5 py-0.5 rounded-full text-xs font-medium ${
            twoFactorEnabled
              ? "bg-green-100 text-green-800 dark:bg-green-900/30 dark:text-green-300"
              : "bg-surface-100 text-surface-800/60 dark:bg-surface-800 dark:text-surface-400"
          }`}>
            {twoFactorEnabled ? "Enabled" : "Disabled"}
          </span>
        </div>

        {!twoFactorEnabled && !show2faSetup && (
          <button
            onClick={() => setShow2faSetup(true)}
            className="inline-flex items-center px-4 py-2 rounded-lg text-sm font-medium bg-brand-600 text-white hover:bg-brand-700 transition-colors"
          >
            Enable 2FA
          </button>
        )}

        {show2faSetup && !twoFactorEnabled && (
          <div className="rounded-lg border border-surface-200 p-4 space-y-3">
            <p className="text-sm text-surface-800/70">
              Scan the QR code below with your authenticator app (Google Authenticator, Authy, etc.).
            </p>
            <div className="flex items-center gap-4">
              {/* Placeholder QR code */}
              <div className="h-32 w-32 rounded-lg bg-surface-100 flex items-center justify-center dark:bg-surface-800">
                <svg className="w-20 h-20 text-surface-800/30 dark:text-surface-500" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1">
                  <rect x="3" y="3" width="7" height="7" />
                  <rect x="14" y="3" width="7" height="7" />
                  <rect x="3" y="14" width="7" height="7" />
                  <rect x="14" y="14" width="3" height="3" />
                  <rect x="18" y="14" width="3" height="3" />
                  <rect x="14" y="18" width="3" height="3" />
                  <rect x="18" y="18" width="3" height="3" />
                </svg>
              </div>
              <div className="space-y-2">
                <p className="text-xs text-surface-800/50 font-mono bg-surface-100 px-2 py-1 rounded dark:bg-surface-800">
                  XERGON-TOTP-SEED-12345
                </p>
                <p className="text-xs text-surface-800/40">
                  Or enter this code manually in your app
                </p>
              </div>
            </div>
            <div>
              <label className="block text-sm font-medium text-surface-900 mb-1">Verification Code</label>
              <div className="flex items-center gap-2">
                <input
                  type="text"
                  placeholder="000000"
                  maxLength={6}
                  className="w-32 px-3 py-2 text-sm rounded-lg border border-surface-200 bg-surface-0 focus:outline-none focus:ring-2 focus:ring-brand-500/30 font-mono text-center tracking-widest"
                />
                <button
                  onClick={() => save2fa(true)}
                  className="px-4 py-2 text-sm font-medium rounded-lg bg-green-600 text-white hover:bg-green-700 transition-colors"
                >
                  Verify & Enable
                </button>
                <button
                  onClick={() => setShow2faSetup(false)}
                  className="px-4 py-2 text-sm font-medium rounded-lg text-surface-800/60 hover:bg-surface-50 transition-colors"
                >
                  Cancel
                </button>
              </div>
            </div>
          </div>
        )}

        {twoFactorEnabled && (
          <button
            onClick={() => save2fa(false)}
            className="px-4 py-2 text-sm font-medium rounded-lg border border-red-200 text-red-600 hover:bg-red-50 transition-colors dark:border-red-800 dark:hover:bg-red-900/10"
          >
            Disable 2FA
          </button>
        )}
      </section>

      {/* Change password */}
      <section className="rounded-xl border border-surface-200 bg-surface-0 p-6">
        <h2 className="font-semibold mb-4">Change Password</h2>
        <div className="space-y-3 max-w-sm">
          <div>
            <label className="block text-sm font-medium text-surface-900 mb-1">Current Password</label>
            <input
              type="password"
              value={currentPassword}
              onChange={(e) => setCurrentPassword(e.target.value)}
              className="w-full px-3 py-2 text-sm rounded-lg border border-surface-200 bg-surface-0 focus:outline-none focus:ring-2 focus:ring-brand-500/30"
            />
          </div>
          <div>
            <label className="block text-sm font-medium text-surface-900 mb-1">New Password</label>
            <input
              type="password"
              value={newPassword}
              onChange={(e) => setNewPassword(e.target.value)}
              className="w-full px-3 py-2 text-sm rounded-lg border border-surface-200 bg-surface-0 focus:outline-none focus:ring-2 focus:ring-brand-500/30"
            />
          </div>
          <div>
            <label className="block text-sm font-medium text-surface-900 mb-1">Confirm New Password</label>
            <input
              type="password"
              value={confirmPassword}
              onChange={(e) => setConfirmPassword(e.target.value)}
              className="w-full px-3 py-2 text-sm rounded-lg border border-surface-200 bg-surface-0 focus:outline-none focus:ring-2 focus:ring-brand-500/30"
            />
          </div>
          <button
            onClick={handleChangePassword}
            disabled={changingPassword}
            className="inline-flex items-center px-4 py-2 rounded-lg text-sm font-medium bg-brand-600 text-white hover:bg-brand-700 transition-colors disabled:opacity-50"
          >
            {changingPassword ? "Updating..." : "Update Password"}
          </button>
        </div>
      </section>

      {/* Active sessions */}
      <section className="rounded-xl border border-surface-200 bg-surface-0 p-6">
        <h2 className="font-semibold mb-4">Active Sessions</h2>
        <div className="space-y-3">
          {sessions.map((session) => (
            <div key={session.id} className="flex items-center justify-between py-2 border-b border-surface-100 last:border-0 dark:border-surface-800">
              <div className="flex items-center gap-3">
                <div className="h-10 w-10 rounded-lg bg-surface-100 flex items-center justify-center text-lg dark:bg-surface-800">
                  {session.device === "Mobile" ? "📱" : "💻"}
                </div>
                <div>
                  <div className="flex items-center gap-2">
                    <p className="text-sm font-medium text-surface-900">
                      {session.browser} on {session.device}
                    </p>
                    {session.current && (
                      <span className="inline-flex items-center px-1.5 py-0.5 rounded text-[10px] font-medium bg-green-100 text-green-800 dark:bg-green-900/30 dark:text-green-300">
                        Current
                      </span>
                    )}
                  </div>
                  <p className="text-xs text-surface-800/40">
                    {session.location} · {session.ip} · Last active {new Date(session.lastActive).toLocaleDateString()}
                  </p>
                </div>
              </div>
              {!session.current && (
                <button
                  onClick={() => revokeSession(session.id)}
                  className="px-2.5 py-1 text-xs font-medium rounded-md text-red-600 border border-red-200 hover:bg-red-50 transition-colors dark:border-red-800 dark:hover:bg-red-900/10"
                >
                  Revoke
                </button>
              )}
            </div>
          ))}
        </div>
      </section>

      {/* Login history */}
      <section className="rounded-xl border border-surface-200 bg-surface-0 p-6">
        <h2 className="font-semibold mb-4">Login History</h2>
        <div className="space-y-2">
          {loginHistory.map((entry) => (
            <div key={entry.id} className="flex items-center gap-3 py-2 border-b border-surface-100 last:border-0 dark:border-surface-800">
              <div className={`h-2 w-2 rounded-full shrink-0 ${entry.success ? "bg-green-500" : "bg-red-500"}`} />
              <div className="flex-1 min-w-0">
                <p className="text-sm text-surface-800/70">
                  {entry.device} from {entry.location} ({entry.ip})
                </p>
                <p className="text-xs text-surface-800/40">
                  {new Date(entry.timestamp).toLocaleString()}
                  {!entry.success && (
                    <span className="ml-2 text-red-500 font-medium">Failed</span>
                  )}
                </p>
              </div>
            </div>
          ))}
        </div>
      </section>
    </div>
  );
}
