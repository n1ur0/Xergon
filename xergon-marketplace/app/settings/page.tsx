"use client";

import { useState, useEffect, useCallback } from "react";
import { useAuthStore } from "@/lib/stores/auth";
import { useRouter, useSearchParams } from "next/navigation";
import { endpoints } from "@/lib/api/client";
import type { TransactionView, CreditPack, AutoReplenishSettings, MeResponse } from "@/lib/api/client";
import Link from "next/link";
import { Suspense } from "react";
import { toast } from "sonner";

function SettingsContent() {
  const user = useAuthStore((s) => s.user);
  const setUser = useAuthStore((s) => s.setUser);
  const logout = useAuthStore((s) => s.logout);
  const refreshCredits = useAuthStore((s) => s.refreshCredits);
  const router = useRouter();
  const searchParams = useSearchParams();

  const [transactions, setTransactions] = useState<TransactionView[]>([]);
  const [loading, setLoading] = useState(true);
  const [checkoutStatus, setCheckoutStatus] = useState<string | null>(null);

  // Profile editing state
  const [profileName, setProfileName] = useState("");
  const [profileEmail, setProfileEmail] = useState("");
  const [profileSaving, setProfileSaving] = useState(false);

  // Change password state
  const [currentPassword, setCurrentPassword] = useState("");
  const [newPassword, setNewPassword] = useState("");
  const [confirmNewPassword, setConfirmNewPassword] = useState("");
  const [passwordSaving, setPasswordSaving] = useState(false);

  // Wallet linking state
  const [walletInput, setWalletInput] = useState("");
  const [walletSaving, setWalletSaving] = useState(false);

  // Auto-replenish state
  const [autoReplenish, setAutoReplenish] = useState<AutoReplenishSettings>({
    enabled: false,
    pack_id: null,
    threshold_usd: 1.0,
  });
  const [packs, setPacks] = useState<CreditPack[]>([]);
  const [arLoading, setArLoading] = useState(false);
  const [arSaving, setArSaving] = useState(false);

  useEffect(() => {
    const status = searchParams.get("checkout");
    if (status === "success") {
      setCheckoutStatus("success");
      refreshCredits();
      toast.success("Credits purchased!");
    } else if (status === "cancelled") {
      setCheckoutStatus("cancelled");
      toast.info("Checkout cancelled");
    }

    const creditsAdded = searchParams.get("credits_added");
    if (creditsAdded) {
      setCheckoutStatus(`added_${creditsAdded}`);
      refreshCredits();
    }

    // Load transactions + auto-replenish settings + packs
    if (user) {
      setProfileName(user.name ?? "");
      setProfileEmail(user.email);
      Promise.all([
        endpoints.getTransactions().then((res) => setTransactions(res.transactions)).catch(() => {}),
        endpoints.getAutoReplenish().then((res) => setAutoReplenish(res)).catch(() => {}),
        endpoints.getPacks().then((res) => setPacks(res.packs)).catch(() => {}),
      ]).finally(() => setLoading(false));
    } else {
      setLoading(false);
    }
  }, [user, searchParams, refreshCredits]);

  const handleProfileSave = useCallback(async () => {
    setProfileSaving(true);
    try {
      const updatedUser: MeResponse = await endpoints.updateProfile({
        name: profileName || undefined,
        email: profileEmail,
      });
      // Update auth store
      setUser({
        id: updatedUser.id,
        email: updatedUser.email,
        name: updatedUser.name,
        tier: updatedUser.tier,
        credits: updatedUser.credits_usd,
        ergoAddress: updatedUser.ergo_address ?? null,
      });
      toast.success("Profile updated");
    } catch (err) {
      const message = err instanceof Error ? err.message : "Failed to update profile";
      toast.error(message);
    } finally {
      setProfileSaving(false);
    }
  }, [profileName, profileEmail, setUser]);

  const handlePasswordSave = useCallback(async () => {
    if (newPassword.length < 8) {
      toast.error("New password must be at least 8 characters");
      return;
    }
    if (newPassword !== confirmNewPassword) {
      toast.error("Passwords do not match");
      return;
    }

    setPasswordSaving(true);
    try {
      await endpoints.changePassword({
        current_password: currentPassword,
        new_password: newPassword,
      });
      toast.success("Password updated");
      setCurrentPassword("");
      setNewPassword("");
      setConfirmNewPassword("");
    } catch (err) {
      const message = err instanceof Error ? err.message : "Failed to change password";
      toast.error(message);
    } finally {
      setPasswordSaving(false);
    }
  }, [currentPassword, newPassword, confirmNewPassword]);

  const handleAutoReplenishSave = useCallback(async () => {
    setArSaving(true);
    try {
      const res = await endpoints.updateAutoReplenish(autoReplenish);
      setAutoReplenish(res);
      toast.success("Settings saved");
    } catch {
      toast.error("Failed to save settings");
    } finally {
      setArSaving(false);
    }
  }, [autoReplenish]);

  function truncateAddress(addr: string) {
    if (addr.length <= 12) return addr;
    return `${addr.slice(0, 8)}...${addr.slice(-4)}`;
  }

  const handleWalletLink = useCallback(async () => {
    const addr = walletInput.trim();
    if (!addr.startsWith("9") || addr.length !== 95) {
      toast.error("Invalid Ergo address: must start with '9' and be 95 characters long");
      return;
    }

    setWalletSaving(true);
    try {
      await endpoints.updateWalletAddress(addr);
      setUser({ ...user!, ergoAddress: addr });
      setWalletInput("");
      toast.success("Wallet linked successfully");
    } catch (err) {
      const message = err instanceof Error ? err.message : "Failed to link wallet";
      toast.error(message);
    } finally {
      setWalletSaving(false);
    }
  }, [walletInput, user, setUser]);

  const handleWalletUnlink = useCallback(async () => {
    setWalletSaving(true);
    try {
      await endpoints.updateWalletAddress(null);
      setUser({ ...user!, ergoAddress: null });
      toast.success("Wallet unlinked");
    } catch (err) {
      const message = err instanceof Error ? err.message : "Failed to unlink wallet";
      toast.error(message);
    } finally {
      setWalletSaving(false);
    }
  }, [user, setUser]);

  function handleLogout() {
    logout();
    router.push("/signin");
  }

  if (!user) {
    return (
      <div className="max-w-2xl mx-auto px-4 py-8">
        <h1 className="text-2xl font-bold mb-2">Settings</h1>
        <p className="text-surface-800/60 mb-8">Manage your account and preferences.</p>
        <div className="rounded-xl border border-surface-200 bg-surface-0 p-8 text-center">
          <p className="text-surface-800/50 mb-4">Sign in to manage your account</p>
          <Link
            href="/signin"
            className="inline-block rounded-lg bg-brand-600 px-6 py-2 text-sm font-medium text-white transition-colors hover:bg-brand-700"
          >
            Sign in
          </Link>
        </div>
      </div>
    );
  }

  return (
    <div className="max-w-2xl mx-auto px-4 py-8">
      <h1 className="text-2xl font-bold mb-2">Settings</h1>
      <p className="text-surface-800/60 mb-8">Manage your account and preferences.</p>

      {checkoutStatus === "success" && (
        <div className="mb-6 rounded-lg border border-accent-500/30 bg-accent-500/10 px-4 py-3 text-sm text-accent-600">
          Payment successful! Credits have been added to your account.
        </div>
      )}

      {checkoutStatus?.startsWith("added_") && (
        <div className="mb-6 rounded-lg border border-accent-500/30 bg-accent-500/10 px-4 py-3 text-sm text-accent-600">
          ${checkoutStatus.replace("added_", "")} in credits added to your account (dev mode).
        </div>
      )}

      {checkoutStatus === "cancelled" && (
        <div className="mb-6 rounded-lg border border-surface-200 bg-surface-100 px-4 py-3 text-sm text-surface-800/60">
          Payment was cancelled. No charges were made.
        </div>
      )}

      <div className="space-y-6">
        {/* Editable Profile */}
        <section className="rounded-xl border border-surface-200 bg-surface-0 p-6">
          <h2 className="font-semibold mb-4">Profile</h2>
          <div className="space-y-4">
            <div>
              <label htmlFor="profileEmail" className="block text-sm text-surface-800/70 mb-1">
                Email
              </label>
              <input
                id="profileEmail"
                type="email"
                value={profileEmail}
                onChange={(e) => setProfileEmail(e.target.value)}
                className="w-full rounded-lg border border-surface-200 bg-surface-0 px-3 py-2 text-sm outline-none focus:border-brand-500 focus:ring-2 focus:ring-brand-500/20"
              />
            </div>

            <div>
              <label htmlFor="profileName" className="block text-sm text-surface-800/70 mb-1">
                Display Name
              </label>
              <input
                id="profileName"
                type="text"
                value={profileName}
                onChange={(e) => setProfileName(e.target.value)}
                placeholder="Your display name"
                className="w-full rounded-lg border border-surface-200 bg-surface-0 px-3 py-2 text-sm outline-none focus:border-brand-500 focus:ring-2 focus:ring-brand-500/20"
              />
            </div>

            <div className="grid grid-cols-2 gap-4 text-sm">
              <div>
                <span className="text-surface-800/50">Account Tier</span>
                <p className="capitalize font-medium">{user.tier}</p>
              </div>
              <div>
                <span className="text-surface-800/50">Credit Balance</span>
                <p className="font-medium">${user.credits.toFixed(2)} USD</p>
              </div>
              <div>
                <span className="text-surface-800/50">Rate Limit</span>
                <p>{user.tier === "pro" ? "10,000 requests / 30 days" : "10 requests / day"}</p>
              </div>
            </div>

            <button
              onClick={handleProfileSave}
              disabled={profileSaving}
              className="rounded-lg bg-brand-600 px-4 py-2 text-sm font-medium text-white transition-colors hover:bg-brand-700 disabled:opacity-50"
            >
              {profileSaving ? "Saving..." : "Save profile"}
            </button>
          </div>
        </section>

        {/* Change Password */}
        <section className="rounded-xl border border-surface-200 bg-surface-0 p-6">
          <h2 className="font-semibold mb-4">Change Password</h2>
          <div className="space-y-4">
            <div>
              <label htmlFor="currentPassword" className="block text-sm text-surface-800/70 mb-1">
                Current password
              </label>
              <input
                id="currentPassword"
                type="password"
                value={currentPassword}
                onChange={(e) => setCurrentPassword(e.target.value)}
                placeholder="Enter current password"
                className="w-full rounded-lg border border-surface-200 bg-surface-0 px-3 py-2 text-sm outline-none focus:border-brand-500 focus:ring-2 focus:ring-brand-500/20"
              />
            </div>

            <div>
              <label htmlFor="newPassword" className="block text-sm text-surface-800/70 mb-1">
                New password
              </label>
              <input
                id="newPassword"
                type="password"
                value={newPassword}
                onChange={(e) => setNewPassword(e.target.value)}
                placeholder="Min 8 characters"
                className="w-full rounded-lg border border-surface-200 bg-surface-0 px-3 py-2 text-sm outline-none focus:border-brand-500 focus:ring-2 focus:ring-brand-500/20"
              />
            </div>

            <div>
              <label htmlFor="confirmNewPassword" className="block text-sm text-surface-800/70 mb-1">
                Confirm new password
              </label>
              <input
                id="confirmNewPassword"
                type="password"
                value={confirmNewPassword}
                onChange={(e) => setConfirmNewPassword(e.target.value)}
                placeholder="Confirm new password"
                className="w-full rounded-lg border border-surface-200 bg-surface-0 px-3 py-2 text-sm outline-none focus:border-brand-500 focus:ring-2 focus:ring-brand-500/20"
              />
            </div>

            <button
              onClick={handlePasswordSave}
              disabled={passwordSaving || !currentPassword || !newPassword || !confirmNewPassword}
              className="rounded-lg bg-brand-600 px-4 py-2 text-sm font-medium text-white transition-colors hover:bg-brand-700 disabled:opacity-50"
            >
              {passwordSaving ? "Updating..." : "Change password"}
            </button>
          </div>
        </section>

        {/* Ergo Wallet */}
        <section id="wallet" className="rounded-xl border border-surface-200 bg-surface-0 p-6">
          <h2 className="font-semibold mb-4">Wallet</h2>
          <div className="space-y-4">
            {user.ergoAddress ? (
              <>
                <div>
                  <span className="block text-sm text-surface-800/50 mb-1">Linked Ergo Address</span>
                  <p className="font-mono text-sm bg-surface-100 rounded-lg px-3 py-2 break-all">
                    {truncateAddress(user.ergoAddress)}
                  </p>
                </div>
                <button
                  onClick={handleWalletUnlink}
                  disabled={walletSaving}
                  className="rounded-lg bg-surface-200 px-4 py-2 text-sm font-medium text-surface-800 transition-colors hover:bg-surface-300 disabled:opacity-50"
                >
                  {walletSaving ? "Updating..." : "Unlink wallet"}
                </button>
              </>
            ) : (
              <>
                <div>
                  <label htmlFor="walletAddress" className="block text-sm text-surface-800/70 mb-1">
                    Ergo Wallet Address
                  </label>
                  <input
                    id="walletAddress"
                    type="text"
                    value={walletInput}
                    onChange={(e) => setWalletInput(e.target.value)}
                    placeholder="9..."
                    className="w-full rounded-lg border border-surface-200 bg-surface-0 px-3 py-2 text-sm font-mono outline-none focus:border-brand-500 focus:ring-2 focus:ring-brand-500/20"
                  />
                </div>
                <button
                  onClick={handleWalletLink}
                  disabled={walletSaving || !walletInput.trim()}
                  className="rounded-lg bg-brand-600 px-4 py-2 text-sm font-medium text-white transition-colors hover:bg-brand-700 disabled:opacity-50"
                >
                  {walletSaving ? "Linking..." : "Link Wallet"}
                </button>
              </>
            )}
            <p className="text-xs text-surface-800/40">
              Linking a wallet is required to register as a compute provider.
            </p>
          </div>
        </section>

        {/* Add Credits */}
        <section className="rounded-xl border border-surface-200 bg-surface-0 p-6">
          <h2 className="font-semibold mb-2">Add Credits</h2>
          <p className="text-sm text-surface-800/50 mb-4">
            Purchase credits to use GPU inference. No subscription required.
          </p>
          <Link
            href="/pricing"
            className="inline-block rounded-lg bg-accent-600 px-4 py-2 text-sm font-medium text-white transition-colors hover:bg-accent-500"
          >
            View credit packs
          </Link>
        </section>

        {/* Auto-Replenish */}
        <section className="rounded-xl border border-surface-200 bg-surface-0 p-6">
          <h2 className="font-semibold mb-2">Auto-Replenish</h2>
          <p className="text-sm text-surface-800/50 mb-4">
            Automatically purchase credits when your balance drops below a threshold.
            Requires a saved payment method via Stripe.
          </p>

          <div className="space-y-4">
            <label className="flex items-center gap-3 text-sm">
              <input
                type="checkbox"
                checked={autoReplenish.enabled}
                onChange={(e) => setAutoReplenish((prev) => ({ ...prev, enabled: e.target.checked }))}
                className="rounded border-surface-300"
                disabled={arLoading || arSaving}
              />
              <span>Enable auto-replenish</span>
            </label>

            {autoReplenish.enabled && (
              <>
                <div>
                  <label className="block text-sm text-surface-800/70 mb-1">
                    Credit pack to purchase
                  </label>
                  <select
                    value={autoReplenish.pack_id ?? ""}
                    onChange={(e) => setAutoReplenish((prev) => ({ ...prev, pack_id: e.target.value || null }))}
                    className="w-full rounded-lg border border-surface-200 bg-surface-0 px-3 py-2 text-sm"
                    disabled={arSaving}
                  >
                    <option value="">Select a pack...</option>
                    {packs.map((pack) => (
                      <option key={pack.id} value={pack.id}>
                        {pack.display_price} (+${pack.bonus_credits_usd.toFixed(0)} bonus = ${(pack.amount_usd + pack.bonus_credits_usd).toFixed(2)} total)
                      </option>
                    ))}
                  </select>
                </div>

                <div>
                  <label className="block text-sm text-surface-800/70 mb-1">
                    Replenish when balance drops below
                  </label>
                  <div className="flex items-center gap-2">
                    <span className="text-sm text-surface-800/50">$</span>
                    <input
                      type="number"
                      step="0.50"
                      min="0.50"
                      value={autoReplenish.threshold_usd}
                      onChange={(e) => setAutoReplenish((prev) => ({ ...prev, threshold_usd: parseFloat(e.target.value) || 1.0 }))}
                      className="w-24 rounded-lg border border-surface-200 bg-surface-0 px-3 py-2 text-sm"
                      disabled={arSaving}
                    />
                    <span className="text-sm text-surface-800/50">USD</span>
                  </div>
                </div>
              </>
            )}

            <button
              onClick={handleAutoReplenishSave}
              disabled={arSaving || (autoReplenish.enabled && !autoReplenish.pack_id)}
              className="rounded-lg bg-brand-600 px-4 py-2 text-sm font-medium text-white transition-colors hover:bg-brand-700 disabled:opacity-50"
            >
              {arSaving ? "Saving..." : "Save settings"}
            </button>
          </div>
        </section>

        {/* Transaction History */}
        <section className="rounded-xl border border-surface-200 bg-surface-0 p-6">
          <h2 className="font-semibold mb-4">Transaction History</h2>
          {loading ? (
            <p className="text-sm text-surface-800/40">Loading...</p>
          ) : transactions.length === 0 ? (
            <p className="text-sm text-surface-800/40">No transactions yet.</p>
          ) : (
            <div className="divide-y divide-surface-100">
              {transactions.map((tx) => (
                <div key={tx.id} className="flex items-center justify-between py-2 text-sm">
                  <div>
                    <span className="capitalize font-medium">{tx.kind}</span>
                    <span className="ml-2 text-surface-800/40">{tx.description}</span>
                  </div>
                  <div className="text-right">
                    <span className={tx.amount_usd >= 0 ? "text-accent-600" : "text-danger-600"}>
                      {tx.amount_usd >= 0 ? "+" : ""}{tx.amount_usd.toFixed(2)}
                    </span>
                    <div className="text-xs text-surface-800/40">
                      Balance: ${tx.balance_after.toFixed(2)}
                    </div>
                  </div>
                </div>
              ))}
            </div>
          )}
        </section>

        {/* Actions */}
        <section className="rounded-xl border border-surface-200 bg-surface-0 p-6">
          <h2 className="font-semibold mb-4">Account</h2>
          <button
            onClick={handleLogout}
            className="text-sm text-danger-600 hover:underline"
          >
            Sign out
          </button>
        </section>
      </div>
    </div>
  );
}

export default function SettingsPage() {
  return (
    <Suspense fallback={<div className="max-w-2xl mx-auto px-4 py-8"><p>Loading...</p></div>}>
      <SettingsContent />
    </Suspense>
  );
}
