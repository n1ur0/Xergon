"use client";

import { useState, useEffect, useCallback } from "react";
import {
  Wallet,
  CheckCircle2,
  AlertCircle,
  Loader2,
  Copy,
  ExternalLink,
  RefreshCw,
  Keyboard,
} from "lucide-react";
import { useAuth } from "@/lib/auth-context";
import { isNautilusAvailable, connectNautilus } from "@/lib/wallet/nautilus";

interface WalletStepProps {
  value: {
    connected: boolean;
    address: string | null;
    balance: number | null;
  };
  onChange: (connected: boolean, address: string | null, balance: number | null) => void;
}

export default function WalletStep({ value, onChange }: WalletStepProps) {
  const { isAuthenticated, ergoAddress, balance, signInNautilus } = useAuth();
  const [status, setStatus] = useState<"idle" | "connecting" | "connected" | "error">("idle");
  const [errorMsg, setErrorMsg] = useState<string | null>(null);
  const [manualAddress, setManualAddress] = useState("");
  const [showManualEntry, setShowManualEntry] = useState(false);
  const [copied, setCopied] = useState(false);
  const [nautilusDetected, setNautilusDetected] = useState(false);

  // Detect Nautilus on mount
  useEffect(() => {
    setNautilusDetected(isNautilusAvailable());
  }, []);

  // Sync with auth state if already connected
  useEffect(() => {
    if (isAuthenticated && ergoAddress) {
      setStatus("connected");
      onChange(true, ergoAddress, balance);
    }
  }, [isAuthenticated, ergoAddress, balance, onChange]);

  const handleConnectNautilus = useCallback(async () => {
    setStatus("connecting");
    setErrorMsg(null);
    try {
      await signInNautilus();
      // Auth context will update; the useEffect above syncs the value
      setStatus("connected");
    } catch (err) {
      setStatus("error");
      setErrorMsg(err instanceof Error ? err.message : "Failed to connect wallet");
    }
  }, [signInNautilus]);

  const handleManualConnect = useCallback(async () => {
    const addr = manualAddress.trim();
    if (!addr) return;
    if (!/^[39bB]/.test(addr) || addr.length < 30) {
      setErrorMsg("Invalid Ergo address format");
      return;
    }

    setStatus("connecting");
    setErrorMsg(null);
    try {
      const res = await fetch("/api/ergoauth", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ address: addr }),
      });
      if (!res.ok) {
        const body = await res.json().catch(() => ({}));
        throw new Error(body.error || "Failed to generate challenge");
      }
      const data = await res.json();
      // Build deep link for mobile wallets
      const params = new URLSearchParams({
        address: data.address,
        signingMessage: data.signingMessage,
        sigmaBoolean: data.sigmaBoolean,
        userMessage: data.userMessage || "Sign to authenticate with Xergon",
        messageSeverity: data.messageSeverity || "INFORMATION",
        replyTo: data.replyTo,
      });
      const deepLink = `ergoauth://?${params.toString()}`;
      window.open(deepLink, "_blank");
      setStatus("idle");
      setErrorMsg("Deep link opened. Complete signing in your wallet, then refresh.");
    } catch (err) {
      setStatus("error");
      setErrorMsg(err instanceof Error ? err.message : "Manual connection failed");
    }
  }, [manualAddress]);

  const handleCopyAddress = useCallback(() => {
    if (value.address) {
      navigator.clipboard.writeText(value.address);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    }
  }, [value.address]);

  const truncatedAddress = value.address
    ? `${value.address.slice(0, 8)}...${value.address.slice(-4)}`
    : null;

  // Connected state
  if (status === "connected" && value.address) {
    return (
      <div className="space-y-6">
        <div className="text-center space-y-3">
          <div className="mx-auto flex h-16 w-16 items-center justify-center rounded-full bg-emerald-100 dark:bg-emerald-900/30">
            <CheckCircle2 className="h-8 w-8 text-emerald-500" />
          </div>
          <h2 className="text-xl font-bold text-surface-900 dark:text-surface-0">
            Wallet Connected
          </h2>
          <p className="text-sm text-surface-800/60 dark:text-surface-300/60">
            Your Ergo wallet is connected and verified.
          </p>
        </div>

        <div className="mx-auto max-w-sm rounded-xl border border-surface-200 bg-surface-0 p-5 dark:border-surface-700 dark:bg-surface-900">
          <div className="flex items-center justify-between">
            <div>
              <p className="text-xs text-surface-800/50 dark:text-surface-300/50 mb-1">
                Address
              </p>
              <p className="font-mono text-sm text-surface-900 dark:text-surface-0">
                {truncatedAddress}
              </p>
            </div>
            <button
              type="button"
              onClick={handleCopyAddress}
              className="flex h-8 w-8 items-center justify-center rounded-lg text-surface-500 hover:bg-surface-100 dark:hover:bg-surface-800 transition-colors"
            >
              {copied ? (
                <CheckCircle2 className="h-4 w-4 text-emerald-500" />
              ) : (
                <Copy className="h-4 w-4" />
              )}
            </button>
          </div>
          {value.balance !== null && (
            <div className="mt-4 pt-4 border-t border-surface-200 dark:border-surface-700">
              <p className="text-xs text-surface-800/50 dark:text-surface-300/50 mb-1">
                Balance
              </p>
              <p className="text-lg font-bold text-surface-900 dark:text-surface-0">
                {value.balance.toFixed(4)} <span className="text-sm font-normal text-surface-500">ERG</span>
              </p>
            </div>
          )}
        </div>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      <div className="text-center space-y-3">
        <div className="mx-auto flex h-16 w-16 items-center justify-center rounded-2xl bg-gradient-to-br from-blue-500 to-indigo-600 shadow-lg shadow-blue-500/20">
          <Wallet className="h-8 w-8 text-white" />
        </div>
        <h2 className="text-xl font-bold text-surface-900 dark:text-surface-0">
          Connect Your Wallet
        </h2>
        <p className="text-sm text-surface-800/60 dark:text-surface-300/60 max-w-md mx-auto">
          Connect an Ergo wallet to authenticate and interact with the Xergon marketplace.
        </p>
      </div>

      {errorMsg && (
        <div className="mx-auto max-w-md flex items-center gap-2 rounded-lg border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-700 dark:border-red-900/50 dark:bg-red-950/30 dark:text-red-300">
          <AlertCircle className="h-4 w-4 shrink-0" />
          <p>{errorMsg}</p>
        </div>
      )}

      {/* Nautilus wallet */}
      <div className="mx-auto max-w-md space-y-3">
        <button
          type="button"
          onClick={handleConnectNautilus}
          disabled={status === "connecting" || !nautilusDetected}
          className="flex w-full items-center gap-3 rounded-xl border-2 border-surface-200 bg-surface-0 p-4 text-left transition-all hover:border-emerald-300 hover:shadow-sm disabled:opacity-50 disabled:cursor-not-allowed dark:border-surface-700 dark:bg-surface-900 dark:hover:border-emerald-800"
        >
          <div className="flex h-10 w-10 items-center justify-center rounded-lg bg-purple-100 text-purple-600 dark:bg-purple-900/30 dark:text-purple-400">
            <Wallet className="h-5 w-5" />
          </div>
          <div className="flex-1">
            <p className="font-semibold text-surface-900 dark:text-surface-0 text-sm">
              Nautilus Wallet
            </p>
            <p className="text-xs text-surface-800/60 dark:text-surface-300/60">
              {nautilusDetected
                ? "Browser extension detected"
                : "Extension not found"}
            </p>
          </div>
          {status === "connecting" ? (
            <Loader2 className="h-5 w-5 animate-spin text-surface-400" />
          ) : !nautilusDetected ? (
            <a
              href="https://nautiluswallet.com/"
              target="_blank"
              rel="noopener noreferrer"
              className="flex items-center gap-1 text-xs text-blue-500 hover:text-blue-600"
              onClick={(e) => e.stopPropagation()}
            >
              Install
              <ExternalLink className="h-3 w-3" />
            </a>
          ) : (
            <RefreshCw className="h-4 w-4 text-surface-400" />
          )}
        </button>

        {/* Divider */}
        <div className="relative flex items-center gap-3 py-1">
          <div className="flex-1 border-t border-surface-200 dark:border-surface-700" />
          <span className="text-xs text-surface-400">or</span>
          <div className="flex-1 border-t border-surface-200 dark:border-surface-700" />
        </div>

        {/* Manual / ErgoAuth entry */}
        {!showManualEntry ? (
          <button
            type="button"
            onClick={() => setShowManualEntry(true)}
            className="flex w-full items-center justify-center gap-2 rounded-xl border border-dashed border-surface-300 px-4 py-3 text-sm text-surface-600 transition-colors hover:border-surface-400 hover:bg-surface-50 dark:border-surface-600 dark:text-surface-400 dark:hover:border-surface-500 dark:hover:bg-surface-800/50"
          >
            <Keyboard className="h-4 w-4" />
            Enter address manually (ErgoAuth)
          </button>
        ) : (
          <div className="space-y-3 rounded-xl border border-surface-200 bg-surface-50 p-4 dark:border-surface-700 dark:bg-surface-800/50">
            <label className="block">
              <span className="text-xs font-medium text-surface-700 dark:text-surface-300">
                Ergo Address
              </span>
              <input
                type="text"
                value={manualAddress}
                onChange={(e) => setManualAddress(e.target.value)}
                placeholder="3W..."
                className="mt-1 block w-full rounded-lg border border-surface-300 bg-surface-0 px-3 py-2 font-mono text-sm placeholder:text-surface-400 focus:border-emerald-500 focus:outline-none focus:ring-1 focus:ring-emerald-500 dark:border-surface-600 dark:bg-surface-900 dark:placeholder:text-surface-500"
              />
            </label>
            <button
              type="button"
              onClick={handleManualConnect}
              disabled={!manualAddress.trim() || status === "connecting"}
              className="flex w-full items-center justify-center gap-2 rounded-lg bg-emerald-600 px-4 py-2 text-sm font-medium text-white transition-colors hover:bg-emerald-700 disabled:opacity-50 disabled:cursor-not-allowed"
            >
              {status === "connecting" ? (
                <>
                  <Loader2 className="h-4 w-4 animate-spin" />
                  Connecting...
                </>
              ) : (
                <>
                  <Wallet className="h-4 w-4" />
                  Connect with ErgoAuth
                </>
              )}
            </button>
          </div>
        )}
      </div>
    </div>
  );
}
