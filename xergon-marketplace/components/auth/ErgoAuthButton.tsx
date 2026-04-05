/**
 * ErgoAuthButton -- Primary wallet connection button.
 *
 * Detects available EIP-12 wallets on mount and shows:
 * - "Connect Wallet" with wallet name when an extension is found
 * - "Use ErgoAuth" button as fallback for non-extension wallets
 *
 * On connect success, updates the auth store with the wallet address.
 */

"use client";

import { useState, useEffect, useCallback, useRef } from "react";
import { useAuthStore } from "@/lib/stores/auth";
import {
  hasWalletExtension,
  detectAvailableWallets,
  connectAndGetAddress,
  type WalletInfo,
} from "@/lib/ergoauth/eip12-connector";

interface ErgoAuthButtonProps {
  /** Called after successful wallet connection */
  onSuccess?: (address: string) => void;
  /** Called on error */
  onError?: (error: string) => void;
  /** Additional CSS classes */
  className?: string;
  /** Show as compact style (for navbar) */
  compact?: boolean;
  /** Whether to show the ErgoAuth modal when no extension is found */
  showErgoAuthFallback?: boolean;
  /** Callback to open the ErgoAuth modal */
  onOpenErgoAuth?: () => void;
}

type ConnectionState =
  | "idle"
  | "detecting"
  | "selecting"
  | "connecting"
  | "success"
  | "error";

export function ErgoAuthButton({
  onSuccess,
  onError,
  className = "",
  compact = false,
  showErgoAuthFallback = true,
  onOpenErgoAuth,
}: ErgoAuthButtonProps) {
  const [state, setState] = useState<ConnectionState>("detecting");
  const [availableWallets, setAvailableWallets] = useState<WalletInfo[]>([]);
  const [selectedWallet, setSelectedWallet] = useState<WalletInfo | null>(null);
  const [dropdownOpen, setDropdownOpen] = useState(false);
  const [errorMsg, setErrorMsg] = useState<string | null>(null);
  const dropdownRef = useRef<HTMLDivElement>(null);

  const signInNautilus = useAuthStore((s) => s.signInNautilus);
  const connectErgoWallet = useAuthStore((s) => s.connectErgoWallet);
  const isAuthenticated = useAuthStore((s) => s.isAuthenticated);

  // Detect available wallets on mount
  useEffect(() => {
    const wallets = detectAvailableWallets();
    setAvailableWallets(wallets);
    setState("idle");
  }, []);

  // Close dropdown on outside click
  useEffect(() => {
    function handleClickOutside(e: MouseEvent) {
      if (dropdownRef.current && !dropdownRef.current.contains(e.target as Node)) {
        setDropdownOpen(false);
      }
    }
    if (dropdownOpen) {
      document.addEventListener("mousedown", handleClickOutside);
    }
    return () => document.removeEventListener("mousedown", handleClickOutside);
  }, [dropdownOpen]);

  const handleConnectExtension = useCallback(
    async (wallet: WalletInfo) => {
      setState("connecting");
      setErrorMsg(null);
      setDropdownOpen(false);
      setSelectedWallet(wallet);

      try {
        const { address } = await connectAndGetAddress(wallet.name);

        if (wallet.name === "nautilus") {
          // Use the existing Nautilus flow (includes balance check via relay)
          await signInNautilus();
        } else {
          // Generic EIP-12 wallet: use connectErgoWallet
          const accessToken = `eip12_${Date.now()}_${address.slice(0, 8)}`;
          await connectErgoWallet(address, accessToken, wallet.name as "ergoauth");
        }

        setState("success");
        onSuccess?.(address);
      } catch (err) {
        const message =
          err instanceof Error ? err.message : "Connection failed";
        setState("error");
        setErrorMsg(message);
        onError?.(message);

        // Reset to idle after a delay so user can retry
        setTimeout(() => {
          if (state !== "success") setState("idle");
        }, 3000);
      }
    },
    [signInNautilus, connectErgoWallet, onSuccess, onError, state]
  );

  const handleErgoAuth = useCallback(() => {
    setDropdownOpen(false);
    if (onOpenErgoAuth) {
      onOpenErgoAuth();
    }
  }, [onOpenErgoAuth]);

  // Don't render if already authenticated
  if (isAuthenticated) return null;

  // Detecting state
  if (state === "detecting") {
    return (
      <button
        disabled
        className={`inline-flex items-center justify-center gap-2 rounded-lg bg-brand-600
          px-4 py-1.5 text-sm font-medium text-white opacity-60 cursor-not-allowed
          ${compact ? "px-3 py-1 text-xs" : ""} ${className}`}
      >
        <span className="inline-block h-3 w-3 animate-spin rounded-full border-2 border-white/30 border-t-white" />
        {compact ? "..." : "Detecting..."}
      </button>
    );
  }

  // Success state
  if (state === "success") {
    return (
      <div
        className={`inline-flex items-center gap-2 rounded-lg bg-accent-500/10
          px-4 py-1.5 text-sm font-medium text-accent-600
          ${compact ? "px-3 py-1 text-xs" : ""} ${className}`}
      >
        <span className="inline-block h-2 w-2 rounded-full bg-accent-500" />
        Connected
      </div>
    );
  }

  // No wallets available + showErgoAuth
  if (availableWallets.length === 0 && showErgoAuthFallback) {
    return (
      <button
        type="button"
        onClick={handleErgoAuth}
        className={`inline-flex items-center justify-center gap-2 rounded-lg bg-brand-600
          px-4 py-1.5 text-sm font-medium text-white transition-colors
          hover:bg-brand-700 active:bg-brand-800
          ${compact ? "px-3 py-1 text-xs" : ""} ${className}`}
      >
        <svg
          xmlns="http://www.w3.org/2000/svg"
          width={compact ? 14 : 16}
          height={compact ? 14 : 16}
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
          strokeLinecap="round"
          strokeLinejoin="round"
        >
          <rect x="3" y="11" width="18" height="11" rx="2" ry="2" />
          <path d="M7 11V7a5 5 0 0 1 10 0v4" />
        </svg>
        {compact ? "ErgoAuth" : "Use ErgoAuth"}
      </button>
    );
  }

  // Single wallet available — show direct connect button
  if (availableWallets.length === 1) {
    const wallet = availableWallets[0];
    const isConnecting = state === "connecting" && selectedWallet?.name === wallet.name;

    return (
      <button
        type="button"
        onClick={() => handleConnectExtension(wallet)}
        disabled={isConnecting}
        className={`inline-flex items-center justify-center gap-2 rounded-lg bg-brand-600
          px-4 py-1.5 text-sm font-medium text-white transition-colors
          hover:bg-brand-700 active:bg-brand-800 disabled:opacity-60 disabled:cursor-not-allowed
          ${compact ? "px-3 py-1 text-xs" : ""} ${className}`}
      >
        {isConnecting ? (
          <>
            <span className="inline-block h-3 w-3 animate-spin rounded-full border-2 border-white/30 border-t-white" />
            Connecting...
          </>
        ) : (
          <>
            <span>{wallet.icon}</span>
            {compact ? "Connect" : `Connect ${wallet.displayName}`}
          </>
        )}
      </button>
    );
  }

  // Multiple wallets — show dropdown
  return (
    <div className={`relative ${className}`} ref={dropdownRef}>
      <button
        type="button"
        onClick={() => setDropdownOpen((prev) => !prev)}
        className={`inline-flex items-center justify-center gap-2 rounded-lg bg-brand-600
          px-4 py-1.5 text-sm font-medium text-white transition-colors
          hover:bg-brand-700 active:bg-brand-800
          ${compact ? "px-3 py-1 text-xs" : ""}`}
      >
        <svg
          xmlns="http://www.w3.org/2000/svg"
          width={compact ? 14 : 16}
          height={compact ? 14 : 16}
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
          strokeLinecap="round"
          strokeLinejoin="round"
        >
          <rect x="3" y="11" width="18" height="11" rx="2" ry="2" />
          <path d="M7 11V7a5 5 0 0 1 10 0v4" />
        </svg>
        {compact ? "Connect" : "Connect Wallet"}
        <svg
          xmlns="http://www.w3.org/2000/svg"
          width="12"
          height="12"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
          strokeLinecap="round"
          strokeLinejoin="round"
        >
          <polyline points="6 9 12 15 18 9" />
        </svg>
      </button>

      {dropdownOpen && (
        <div
          className="absolute right-0 top-full mt-1 w-52 rounded-lg border border-surface-200
                     bg-surface-0 shadow-lg z-50 py-1 animate-fade-in"
        >
          {availableWallets.map((wallet) => (
            <button
              key={wallet.name}
              type="button"
              onClick={() => handleConnectExtension(wallet)}
              className="flex w-full items-center gap-2 px-3 py-2.5 text-sm text-surface-800/70
                         hover:bg-surface-100 hover:text-surface-900 transition-colors"
            >
              <span className="text-base">{wallet.icon}</span>
              <span>{wallet.displayName}</span>
            </button>
          ))}

          {showErgoAuthFallback && (
            <>
              <div className="my-1 border-t border-surface-200" />
              <button
                type="button"
                onClick={handleErgoAuth}
                className="flex w-full items-center gap-2 px-3 py-2.5 text-sm text-surface-800/70
                           hover:bg-surface-100 hover:text-surface-900 transition-colors"
              >
                <svg
                  xmlns="http://www.w3.org/2000/svg"
                  width="16"
                  height="16"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  strokeWidth="2"
                  strokeLinecap="round"
                  strokeLinejoin="round"
                >
                  <rect x="3" y="11" width="18" height="11" rx="2" ry="2" />
                  <path d="M7 11V7a5 5 0 0 1 10 0v4" />
                </svg>
                <span>Use ErgoAuth</span>
              </button>
            </>
          )}
        </div>
      )}

      {/* Error message */}
      {errorMsg && (
        <div className="absolute right-0 top-full mt-2 w-64 rounded-lg bg-red-50 border border-red-200 p-3 text-xs text-red-600 z-50">
          {errorMsg}
        </div>
      )}
    </div>
  );
}
