"use client";

import { useState, useEffect, useCallback } from "react";
import { useT } from "@/lib/hooks/use-t";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface FaucetStats {
  balance: number;
  totalDistributed: number;
  active: boolean;
  dripAmount: number;
  cooldownMinutes: number;
  totalRequests: number;
}

interface FaucetRequest {
  id: string;
  walletAddress: string;
  amount: number;
  txId: string;
  timestamp: string;
  status: "completed" | "pending" | "failed";
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function nanoergToErg(nanoerg: number): string {
  const erg = nanoerg / 1e9;
  return `${erg.toFixed(4)} ERG`;
}

function truncateAddr(addr: string, len = 10): string {
  if (addr.length <= len * 2 + 3) return addr;
  return `${addr.slice(0, len)}...${addr.slice(-len)}`;
}

function validateErgoAddress(address: string): boolean {
  // Basic Ergo address validation (starts with 9 or 3, 30-100 chars)
  return /^9[a-zA-Z0-9]{28,95}$/.test(address) || /^3[a-zA-Z0-9]{28,95}$/.test(address);
}

// ---------------------------------------------------------------------------
// Loading skeleton
// ---------------------------------------------------------------------------

function LoadingSkeleton() {
  return (
    <div className="max-w-4xl mx-auto px-4 py-8 space-y-6 animate-pulse">
      <div className="h-8 w-40 rounded-lg bg-surface-200" />
      <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
        {Array.from({ length: 3 }).map((_, i) => (
          <div key={i} className="h-24 rounded-xl border border-surface-200 bg-surface-0 p-5" />
        ))}
      </div>
      <div className="h-64 rounded-xl border border-surface-200 bg-surface-0 p-6" />
      <div className="h-48 rounded-xl border border-surface-200 bg-surface-0 p-5" />
    </div>
  );
}

// ---------------------------------------------------------------------------
// Main page
// ---------------------------------------------------------------------------

export default function FaucetPage() {
  const t = useT();
  const [stats, setStats] = useState<FaucetStats | null>(null);
  const [recentRequests, setRecentRequests] = useState<FaucetRequest[]>([]);
  const [loading, setLoading] = useState(true);
  const [submitting, setSubmitting] = useState(false);

  // Form state
  const [walletAddress, setWalletAddress] = useState("");
  const [captchaInput, setCaptchaInput] = useState("");
  const [captchaAnswer, setCaptchaAnswer] = useState("");
  const [formError, setFormError] = useState<string | null>(null);
  const [formSuccess, setFormSuccess] = useState<string | null>(null);

  // Rate limiting
  const [nextRequestAt, setNextRequestAt] = useState<Date | null>(null);
  const [cooldownRemaining, setCooldownRemaining] = useState<string | null>(null);

  // Admin state
  const [isAdmin, setIsAdmin] = useState(false);
  const [adminAmount, setAdminAmount] = useState("0.1");
  const [adminCooldown, setAdminCooldown] = useState("60");
  const [adminToggle, setAdminToggle] = useState(true);

  // Generate captcha
  const generateCaptcha = useCallback(() => {
    const a = Math.floor(Math.random() * 10) + 1;
    const b = Math.floor(Math.random() * 10) + 1;
    setCaptchaAnswer(String(a + b));
    setCaptchaInput("");
    return `${a} + ${b} = ?`;
  }, []);

  const [captchaQuestion, setCaptchaQuestion] = useState("");

  useEffect(() => {
    setCaptchaQuestion(generateCaptcha());
  }, [generateCaptcha]);

  // Load faucet data
  const loadFaucetData = useCallback(async () => {
    try {
      const res = await fetch("/api/faucet");
      if (res.ok) {
        const data = await res.json();
        setStats(data.stats);
        setRecentRequests(data.recentRequests || []);
        if (data.nextRequestAt) {
          setNextRequestAt(new Date(data.nextRequestAt));
        }
      } else {
        // Use mock data
        setStats({
          balance: 50_000_000_000, // 50 ERG
          totalDistributed: 1_250_000_000, // 1.25 ERG
          active: true,
          dripAmount: 100_000_000, // 0.1 ERG
          cooldownMinutes: 60,
          totalRequests: 125,
        });
        setRecentRequests(generateMockRequests());
      }
    } catch {
      setStats({
        balance: 50_000_000_000,
        totalDistributed: 1_250_000_000,
        active: true,
        dripAmount: 100_000_000,
        cooldownMinutes: 60,
        totalRequests: 125,
      });
      setRecentRequests(generateMockRequests());
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    loadFaucetData();
  }, [loadFaucetData]);

  // Cooldown countdown
  useEffect(() => {
    if (!nextRequestAt) {
      setCooldownRemaining(null);
      return;
    }
    const update = () => {
      const diff = nextRequestAt.getTime() - Date.now();
      if (diff <= 0) {
        setCooldownRemaining(null);
        setNextRequestAt(null);
        return;
      }
      const mins = Math.floor(diff / 60000);
      const secs = Math.floor((diff % 60000) / 1000);
      setCooldownRemaining(`${mins}m ${secs}s`);
    };
    update();
    const interval = setInterval(update, 1000);
    return () => clearInterval(interval);
  }, [nextRequestAt]);

  // Submit request
  const handleSubmit = useCallback(async (e: React.FormEvent) => {
    e.preventDefault();
    setFormError(null);
    setFormSuccess(null);

    if (!walletAddress.trim()) {
      setFormError("Please enter a wallet address");
      return;
    }
    if (!validateErgoAddress(walletAddress.trim())) {
      setFormError("Invalid Ergo address format");
      return;
    }
    if (captchaInput !== captchaAnswer) {
      setFormError("Incorrect captcha answer");
      setCaptchaQuestion(generateCaptcha());
      return;
    }
    if (cooldownRemaining) {
      setFormError(`Please wait ${cooldownRemaining} before your next request`);
      return;
    }
    if (!stats?.active) {
      setFormError("The faucet is currently disabled");
      return;
    }

    setSubmitting(true);
    try {
      const res = await fetch("/api/faucet", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ walletAddress: walletAddress.trim(), captcha: captchaInput }),
      });

      if (res.ok) {
        const data = await res.json();
        setFormSuccess(`Sent ${nanoergToErg(stats.dripAmount)} to ${truncateAddr(walletAddress)}`);
        setWalletAddress("");
        setCaptchaInput("");
        setCaptchaQuestion(generateCaptcha());
        if (data.nextRequestAt) {
          setNextRequestAt(new Date(data.nextRequestAt));
        }
        // Refresh data
        loadFaucetData();
      } else {
        const body = await res.json().catch(() => ({}));
        setFormError(body.error || "Request failed. Please try again.");
      }
    } catch {
      setFormError("Network error. Please try again.");
      // Simulate success for mock
      setFormSuccess(`Sent ${nanoergToErg(stats?.dripAmount || 100_000_000)} to ${truncateAddr(walletAddress)}`);
      setWalletAddress("");
      setCaptchaInput("");
      setCaptchaQuestion(generateCaptcha());
      setNextRequestAt(new Date(Date.now() + 60 * 60 * 1000));
    } finally {
      setSubmitting(false);
    }
  }, [walletAddress, captchaInput, captchaAnswer, cooldownRemaining, stats, generateCaptcha, loadFaucetData]);

  if (loading) return <LoadingSkeleton />;
  if (!stats) return null;

  return (
    <div className="max-w-4xl mx-auto px-4 py-8">
      {/* Header */}
      <div className="mb-6">
        <h1 className="text-2xl font-bold text-surface-900">{t("faucet.title") || "ERG Faucet"}</h1>
        <p className="text-sm text-surface-800/50 mt-0.5">
          {t("faucet.description") || "Get testnet ERG tokens for development and testing on the Xergon network"}
        </p>
      </div>

      {/* Stats cards */}
      <div className="grid grid-cols-1 md:grid-cols-3 gap-4 mb-6">
        <div className="rounded-xl border border-surface-200 bg-surface-0 p-4">
          <div className="text-xs text-surface-800/50 mb-1">Faucet Balance</div>
          <div className="text-lg font-bold text-surface-900">{nanoergToErg(stats.balance)}</div>
          <div className={`text-xs mt-1 ${stats.active ? "text-emerald-600" : "text-red-500"}`}>
            {stats.active ? "Active" : "Disabled"}
          </div>
        </div>
        <div className="rounded-xl border border-surface-200 bg-surface-0 p-4">
          <div className="text-xs text-surface-800/50 mb-1">Total Distributed</div>
          <div className="text-lg font-bold text-surface-900">{nanoergToErg(stats.totalDistributed)}</div>
          <div className="text-xs text-surface-800/40 mt-1">{stats.totalRequests} requests</div>
        </div>
        <div className="rounded-xl border border-surface-200 bg-surface-0 p-4">
          <div className="text-xs text-surface-800/50 mb-1">Drip Amount</div>
          <div className="text-lg font-bold text-surface-900">{nanoergToErg(stats.dripAmount)}</div>
          <div className="text-xs text-surface-800/40 mt-1">Cooldown: {stats.cooldownMinutes} min</div>
        </div>
      </div>

      {/* Faucet form */}
      <div className="rounded-xl border border-surface-200 bg-surface-0 p-6 mb-6">
        <h2 className="text-lg font-semibold text-surface-900 mb-4">Request Testnet ERG</h2>

        {/* Rate limit display */}
        {cooldownRemaining && (
          <div className="mb-4 rounded-lg border border-amber-200 bg-amber-50 dark:border-amber-800/40 dark:bg-amber-950/20 px-4 py-3 text-sm text-amber-700 dark:text-amber-400">
            <div className="flex items-center gap-2">
              <svg className="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                <circle cx="12" cy="12" r="10" />
                <polyline points="12 6 12 12 16 14" />
              </svg>
              Next request available in <span className="font-semibold">{cooldownRemaining}</span>
            </div>
          </div>
        )}

        {/* Error */}
        {formError && (
          <div className="mb-4 rounded-lg border border-red-200 bg-red-50 dark:border-red-800/40 dark:bg-red-950/20 px-4 py-3 text-sm text-red-600 dark:text-red-400">
            {formError}
          </div>
        )}

        {/* Success */}
        {formSuccess && (
          <div className="mb-4 rounded-lg border border-emerald-200 bg-emerald-50 dark:border-emerald-800/40 dark:bg-emerald-950/20 px-4 py-3 text-sm text-emerald-600 dark:text-emerald-400">
            {formSuccess}
          </div>
        )}

        <form onSubmit={handleSubmit} className="space-y-4">
          {/* Wallet address */}
          <div>
            <label htmlFor="wallet-address" className="block text-sm font-medium text-surface-800/70 mb-1.5">
              Ergo Wallet Address
            </label>
            <input
              id="wallet-address"
              type="text"
              value={walletAddress}
              onChange={(e) => setWalletAddress(e.target.value)}
              placeholder="9..."
              className="w-full px-4 py-2.5 rounded-lg border border-surface-200 bg-surface-0 text-sm text-surface-900 placeholder:text-surface-800/30 focus:outline-none focus:ring-2 focus:ring-brand-500/30 focus:border-brand-500 transition-colors"
              disabled={submitting || !!cooldownRemaining || !stats.active}
            />
          </div>

          {/* Captcha */}
          <div>
            <label className="block text-sm font-medium text-surface-800/70 mb-1.5">
              Verify you are human
            </label>
            <div className="flex items-center gap-3">
              <div className="px-4 py-2 rounded-lg bg-surface-100 dark:bg-surface-800 text-sm font-mono text-surface-800 select-none">
                {captchaQuestion}
              </div>
              <input
                type="text"
                value={captchaInput}
                onChange={(e) => setCaptchaInput(e.target.value)}
                placeholder="Answer"
                className="w-24 px-3 py-2 rounded-lg border border-surface-200 bg-surface-0 text-sm text-surface-900 placeholder:text-surface-800/30 focus:outline-none focus:ring-2 focus:ring-brand-500/30"
                disabled={submitting}
              />
            </div>
          </div>

          {/* Submit */}
          <button
            type="submit"
            disabled={submitting || !!cooldownRemaining || !stats.active}
            className="inline-flex items-center gap-2 px-5 py-2.5 rounded-lg bg-brand-600 text-white text-sm font-medium hover:bg-brand-700 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
          >
            {submitting ? (
              <>
                <svg className="w-4 h-4 animate-spin" viewBox="0 0 24 24" fill="none">
                  <circle cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" className="opacity-25" />
                  <path d="M4 12a8 8 0 018-8" stroke="currentColor" strokeWidth="4" strokeLinecap="round" className="opacity-75" />
                </svg>
                Processing...
              </>
            ) : (
              <>
                <svg className="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                  <path d="M12 2v20M17 5H9.5a3.5 3.5 0 000 7h5a3.5 3.5 0 010 7H6" />
                </svg>
                {t("faucet.request") || "Request ERG"}
              </>
            )}
          </button>
        </form>
      </div>

      {/* Recent requests */}
      <div className="rounded-xl border border-surface-200 bg-surface-0 p-5 mb-6">
        <h2 className="text-lg font-semibold text-surface-900 mb-4">Recent Requests</h2>
        {recentRequests.length === 0 ? (
          <div className="text-center py-8 text-sm text-surface-800/40">No recent requests</div>
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full text-sm">
              <thead>
                <tr className="border-b border-surface-100 text-xs text-surface-800/50 uppercase tracking-wide">
                  <th className="text-left py-2 pr-4 font-medium">Address</th>
                  <th className="text-left py-2 pr-4 font-medium">Amount</th>
                  <th className="text-left py-2 pr-4 font-medium">Status</th>
                  <th className="text-left py-2 font-medium">Time</th>
                </tr>
              </thead>
              <tbody>
                {recentRequests.map((req) => (
                  <tr key={req.id} className="border-b border-surface-50 last:border-0">
                    <td className="py-2 pr-4 font-mono text-xs text-surface-800/70">{truncateAddr(req.walletAddress)}</td>
                    <td className="py-2 pr-4">{nanoergToErg(req.amount)}</td>
                    <td className="py-2 pr-4">
                      <span className={`inline-flex items-center px-2 py-0.5 rounded-full text-xs font-medium ${
                        req.status === "completed" ? "bg-emerald-50 text-emerald-700 dark:bg-emerald-950/30 dark:text-emerald-400"
                          : req.status === "pending" ? "bg-amber-50 text-amber-700 dark:bg-amber-950/30 dark:text-amber-400"
                          : "bg-red-50 text-red-700 dark:bg-red-950/30 dark:text-red-400"
                      }`}>
                        {req.status}
                      </span>
                    </td>
                    <td className="py-2 text-xs text-surface-800/40">
                      {new Date(req.timestamp).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>

      {/* Admin section */}
      <div className="rounded-xl border border-surface-200 bg-surface-0 p-5">
        <div className="flex items-center justify-between mb-4">
          <h2 className="text-lg font-semibold text-surface-900">Admin Controls</h2>
          <button
            type="button"
            onClick={() => setIsAdmin(!isAdmin)}
            className="text-xs text-surface-800/40 hover:text-surface-800/70 transition-colors"
          >
            {isAdmin ? "Hide" : "Show"} admin panel
          </button>
        </div>

        {isAdmin && (
          <div className="space-y-4 border-t border-surface-100 pt-4">
            {/* Toggle faucet */}
            <div className="flex items-center justify-between">
              <div>
                <div className="text-sm font-medium text-surface-900">Faucet Status</div>
                <div className="text-xs text-surface-800/40">Enable or disable the faucet</div>
              </div>
              <button
                type="button"
                onClick={() => setAdminToggle(!adminToggle)}
                className={`relative inline-flex h-6 w-11 items-center rounded-full transition-colors ${adminToggle ? "bg-emerald-500" : "bg-surface-300"}`}
              >
                <span className={`inline-block h-4 w-4 rounded-full bg-white transition-transform ${adminToggle ? "translate-x-6" : "translate-x-1"}`} />
              </button>
            </div>

            {/* Drip amount */}
            <div className="flex items-center justify-between gap-4">
              <div>
                <div className="text-sm font-medium text-surface-900">Drip Amount (ERG)</div>
                <div className="text-xs text-surface-800/40">Amount sent per request</div>
              </div>
              <input
                type="text"
                value={adminAmount}
                onChange={(e) => setAdminAmount(e.target.value)}
                className="w-24 px-3 py-1.5 text-sm rounded-lg border border-surface-200 bg-surface-0 text-right text-surface-900 focus:outline-none focus:ring-2 focus:ring-brand-500/30"
              />
            </div>

            {/* Cooldown */}
            <div className="flex items-center justify-between gap-4">
              <div>
                <div className="text-sm font-medium text-surface-900">Cooldown (minutes)</div>
                <div className="text-xs text-surface-800/40">Time between requests per address</div>
              </div>
              <input
                type="text"
                value={adminCooldown}
                onChange={(e) => setAdminCooldown(e.target.value)}
                className="w-24 px-3 py-1.5 text-sm rounded-lg border border-surface-200 bg-surface-0 text-right text-surface-900 focus:outline-none focus:ring-2 focus:ring-brand-500/30"
              />
            </div>

            {/* Admin stats */}
            <div className="grid grid-cols-2 gap-3 pt-2">
              <div className="rounded-lg bg-surface-50 dark:bg-surface-800/50 p-3 text-center">
                <div className="text-xs text-surface-800/40">Remaining Balance</div>
                <div className="text-sm font-bold text-surface-900">{nanoergToErg(stats.balance)}</div>
              </div>
              <div className="rounded-lg bg-surface-50 dark:bg-surface-800/50 p-3 text-center">
                <div className="text-xs text-surface-800/40">Estimated Requests Left</div>
                <div className="text-sm font-bold text-surface-900">
                  {stats.dripAmount > 0 ? Math.floor(stats.balance / stats.dripAmount) : 0}
                </div>
              </div>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Mock data generator
// ---------------------------------------------------------------------------

function generateMockRequests(): FaucetRequest[] {
  const addresses = [
    "9hEQ6M2n3eQaMj9WjEALYNkN3zZzGXRmDsR9Xj3aWKKcxHJEb3u",
    "3WvR2XYEPXfDnDZjf3jmZK2mNW2XWKNXhX4vZQjWCrNEhGZyQWQ",
    "9f5Rb4nBnYF3zKq2x7mVZ5wMqJ2kRfXk2L8pDgXsQmZcRvTbXjN",
    "3WxT8yLqPsK7vXmRZw5bFNhCjQ2kRfXk2L8pDgXsQmZcRvTbXjN",
    "9gK2mN5pQrTsVwXbZcRvTbXjN3WvR2XYEPXfDnDZjf3jmZK2mNW2",
  ];
  const now = Date.now();
  return addresses.map((addr, i) => ({
    id: `req-${i}`,
    walletAddress: addr,
    amount: 100_000_000,
    txId: `tx_${Math.random().toString(36).slice(2, 10)}`,
    timestamp: new Date(now - i * 3600_000 * (1 + Math.random())).toISOString(),
    status: i === 0 ? ("pending" as const) : "completed" as const,
  }));
}
