/**
 * WalletStatus -- Navbar indicator showing wallet connection status.
 *
 * Displays a colored dot (green/gray/red/spinning), truncated address,
 * ERG balance with live refresh, and a dropdown menu for wallet actions.
 *
 * States:
 *   connected   -> green dot + address truncated + ERG balance
 *   connecting  -> spinning dot + "Connecting..."
 *   disconnected -> gray dot
 *   error       -> red dot + error message
 */

"use client";

import { useState, useEffect, useRef, useCallback } from "react";
import { useAuthStore } from "@/lib/stores/auth";
import { getBalance } from "@/lib/wallet/nautilus";
import { isNautilusAvailable } from "@/lib/wallet/nautilus";

type Status = "connected" | "connecting" | "disconnected" | "error";

/** Explorer base URL for viewing addresses on-chain */
const EXPLORER_BASE = "https://explorer.ergoplatform.com/en/addresses/";

/** Balance refresh interval when connected (60s) */
const BALANCE_REFRESH_MS = 60_000;

export function WalletStatus() {
  const user = useAuthStore((s) => s.user);
  const walletType = useAuthStore((s) => s.walletType);
  const isAuthenticated = useAuthStore((s) => s.isAuthenticated);
  const refreshBalance = useAuthStore((s) => s.refreshBalance);
  const lastWalletError = useAuthStore((s) => s.lastWalletError);
  const clearWalletError = useAuthStore((s) => s.clearWalletError);
  const signOut = useAuthStore((s) => s.signOut);
  const disconnectNautilus = useAuthStore((s) => s.disconnectNautilus);

  const [menuOpen, setMenuOpen] = useState(false);
  const [ergBalance, setErgBalance] = useState<number | null>(null);
  const [status, setStatus] = useState<Status>("connecting");
  const menuRef = useRef<HTMLDivElement>(null);

  // Determine connection status from auth state
  useEffect(() => {
    if (!isAuthenticated || !user) {
      setStatus("disconnected");
      setErgBalance(null);
      return;
    }

    if (lastWalletError) {
      setStatus("error");
    } else if (walletType === "nautilus" && isNautilusAvailable()) {
      setStatus("connected");
    } else if (walletType === "nautilus") {
      setStatus("error");
    } else {
      setStatus("connected");
    }
  }, [isAuthenticated, user, walletType, lastWalletError]);

  // Live ERG balance refresh from wallet (not relay) every 60s
  useEffect(() => {
    if (status !== "connected" || walletType !== "nautilus") return;

    let cancelled = false;

    const fetchBalance = async () => {
      try {
        const bal = await getBalance();
        if (!cancelled) setErgBalance(bal);
      } catch {
        // Wallet may be busy — keep the last known balance
      }
    };

    fetchBalance();
    const intervalId = setInterval(fetchBalance, BALANCE_REFRESH_MS);

    return () => {
      cancelled = true;
      clearInterval(intervalId);
    };
  }, [status, walletType]);

  // Close menu on outside click
  useEffect(() => {
    function handleClickOutside(e: MouseEvent) {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) {
        setMenuOpen(false);
      }
    }
    if (menuOpen) {
      document.addEventListener("mousedown", handleClickOutside);
    }
    return () => document.removeEventListener("mousedown", handleClickOutside);
  }, [menuOpen]);

  const handleRefreshBalance = useCallback(async () => {
    setMenuOpen(false);
    if (walletType === "nautilus") {
      try {
        const bal = await getBalance();
        setErgBalance(bal);
      } catch {
        // silent
      }
    }
    refreshBalance();
  }, [walletType, refreshBalance]);

  const handleDisconnect = useCallback(async () => {
    setMenuOpen(false);
    if (walletType === "nautilus") {
      await disconnectNautilus();
    } else {
      signOut();
    }
  }, [walletType, disconnectNautilus, signOut]);

  const handleViewOnExplorer = useCallback(() => {
    if (user?.ergoAddress) {
      window.open(`${EXPLORER_BASE}${user.ergoAddress}`, "_blank");
    }
    setMenuOpen(false);
  }, [user?.ergoAddress]);

  // Don't render when not authenticated
  if (!isAuthenticated || !user) return null;

  const truncated =
    user.ergoAddress.length <= 16
      ? user.ergoAddress
      : `${user.ergoAddress.slice(0, 10)}...${user.ergoAddress.slice(-4)}`;

  const dotColor = {
    connected: "bg-emerald-500",
    connecting: "bg-surface-400 animate-spin",
    disconnected: "bg-surface-300",
    error: "bg-red-500",
  }[status];

  const dotSize = status === "connecting" ? "h-3 w-3 border-2 border-surface-200 border-t-transparent rounded-full" : "h-2 w-2 rounded-full";

  return (
    <div className="relative" ref={menuRef}>
      {/* Status button */}
      <button
        type="button"
        onClick={() => {
          if (lastWalletError) clearWalletError();
          setMenuOpen((prev) => !prev);
        }}
        className="flex items-center gap-2 px-2.5 py-1.5 rounded-lg text-sm
                   text-surface-800/70 hover:text-surface-900 hover:bg-surface-100
                   transition-colors"
        aria-label="Wallet status"
        aria-expanded={menuOpen}
      >
        <span className={`inline-block shrink-0 ${dotSize} ${dotColor}`} />
        {status === "connecting" ? (
          <span className="text-surface-800/40">Connecting...</span>
        ) : status === "error" ? (
          <span className="text-red-600">Error</span>
        ) : (
          <span className="font-mono text-xs">{truncated}</span>
        )}
        {status === "connected" && ergBalance !== null && (
          <span className="text-xs text-surface-800/50">
            {ergBalance.toFixed(4)} ERG
          </span>
        )}
      </button>

      {/* Dropdown menu */}
      {menuOpen && (
        <div
          className="absolute right-0 top-full mt-1 w-52 rounded-lg border border-surface-200
                     bg-surface-0 shadow-lg z-50 py-1"
        >
          <button
            onClick={handleRefreshBalance}
            className="block w-full text-left px-3 py-2 text-sm text-surface-800/70
                       hover:bg-surface-100 hover:text-surface-900 transition-colors"
          >
            Refresh Balance
          </button>
          <button
            onClick={handleViewOnExplorer}
            className="block w-full text-left px-3 py-2 text-sm text-surface-800/70
                       hover:bg-surface-100 hover:text-surface-900 transition-colors"
          >
            View on Explorer
          </button>
          <div className="my-1 border-t border-surface-200" />
          <button
            onClick={handleDisconnect}
            className="block w-full text-left px-3 py-2 text-sm text-red-600
                       hover:bg-red-50 transition-colors"
          >
            Disconnect
          </button>
        </div>
      )}
    </div>
  );
}
