"use client";

import { useState, useEffect } from "react";
import { useAuthStore } from "@/lib/stores/auth";
import { useRouter } from "next/navigation";
import Link from "next/link";
import { toast } from "sonner";

export default function SettingsPage() {
  const user = useAuthStore((s) => s.user);
  const refreshBalance = useAuthStore((s) => s.refreshBalance);
  const signOut = useAuthStore((s) => s.signOut);
  const router = useRouter();

  function truncateAddress(addr: string) {
    if (addr.length <= 16) return addr;
    return `${addr.slice(0, 10)}...${addr.slice(-4)}`;
  }

  function handleSignOut() {
    signOut();
    router.push("/signin");
  }

  // Refresh balance on mount
  useEffect(() => {
    if (user) {
      refreshBalance();
    }
  }, [user, refreshBalance]);

  if (!user) {
    return (
      <div className="max-w-2xl mx-auto px-4 py-8">
        <h1 className="text-2xl font-bold mb-2">Settings</h1>
        <p className="text-surface-800/60 mb-8">Manage your wallet and preferences.</p>
        <div className="rounded-xl border border-surface-200 bg-surface-0 p-8 text-center">
          <p className="text-surface-800/50 mb-4">Connect your wallet to access settings</p>
          <Link
            href="/signin"
            className="inline-block rounded-lg bg-brand-600 px-6 py-2 text-sm font-medium text-white transition-colors hover:bg-brand-700"
          >
            Connect Wallet
          </Link>
        </div>
      </div>
    );
  }

  return (
    <div className="max-w-2xl mx-auto px-4 py-8">
      <h1 className="text-2xl font-bold mb-2">Settings</h1>
      <p className="text-surface-800/60 mb-8">Manage your wallet and preferences.</p>

      <div className="space-y-6">
        {/* Wallet Info */}
        <section className="rounded-xl border border-surface-200 bg-surface-0 p-6">
          <h2 className="font-semibold mb-4">Wallet</h2>
          <div className="space-y-4">
            <div>
              <span className="block text-sm text-surface-800/50 mb-1">Ergo Address</span>
              <p className="font-mono text-sm bg-surface-100 rounded-lg px-3 py-2 break-all">
                {user.ergoAddress}
              </p>
            </div>
            <div>
              <span className="block text-sm text-surface-800/50 mb-1">Public Key</span>
              <p className="font-mono text-sm bg-surface-100 rounded-lg px-3 py-2 break-all">
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

        {/* Add ERG */}
        <section className="rounded-xl border border-surface-200 bg-surface-0 p-6">
          <h2 className="font-semibold mb-2">Add ERG</h2>
          <p className="text-sm text-surface-800/50 mb-4">
            Send ERG to your wallet address to pay for inference and GPU rentals.
          </p>
          <div className="rounded-lg bg-surface-900 px-4 py-3 font-mono text-sm text-surface-100 break-all mb-4">
            {user.ergoAddress}
          </div>
          <Link
            href="/pricing"
            className="inline-block rounded-lg bg-accent-600 px-4 py-2 text-sm font-medium text-white transition-colors hover:bg-accent-500"
          >
            View Pricing
          </Link>
        </section>

        {/* Danger Zone */}
        <section className="rounded-xl border border-surface-200 bg-surface-0 p-6">
          <h2 className="font-semibold mb-4">Account</h2>
          <button
            onClick={handleSignOut}
            className="text-sm text-danger-600 hover:underline"
          >
            Disconnect Wallet
          </button>
        </section>
      </div>
    </div>
  );
}
