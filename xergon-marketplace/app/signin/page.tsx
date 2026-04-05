"use client";

import { useState, useEffect } from "react";
import { useAuthStore } from "@/lib/stores/auth";
import { isNautilusAvailable } from "@/lib/wallet/nautilus";
import Link from "next/link";
import { useRouter } from "next/navigation";
import { toast } from "sonner";

export default function SignInPage() {
  const signIn = useAuthStore((s) => s.signIn);
  const signInNautilus = useAuthStore((s) => s.signInNautilus);
  const router = useRouter();
  const [publicKey, setPublicKey] = useState("");
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(false);
  const [method, setMethod] = useState<"cli" | "nautilus">("cli");
  const [nautilusDetected, setNautilusDetected] = useState(false);
  const [connectedAddress, setConnectedAddress] = useState<string | null>(null);

  // Detect Nautilus extension on mount and when method changes
  useEffect(() => {
    if (method === "nautilus") {
      setNautilusDetected(isNautilusAvailable());
    }
  }, [method]);

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    setError("");
    setLoading(true);

    if (method === "nautilus") {
      await handleNautilusConnect();
      return;
    }

    const pk = publicKey.trim();
    if (!pk) {
      setError("Please enter a public key or Ergo address");
      setLoading(false);
      return;
    }

    try {
      await signIn(pk);
      toast.success("Wallet connected!");
      router.push("/playground");
    } catch (err) {
      if (err instanceof Error && err.message === "NO_BALANCE") {
        setError(
          "No ERG balance found. Fund your wallet to use Xergon Network."
        );
      } else {
        const message = err instanceof Error ? err.message : "Connection failed";
        setError(message);
        toast.error(message);
      }
    } finally {
      setLoading(false);
    }
  }

  async function handleNautilusConnect() {
    try {
      setError("");

      if (!isNautilusAvailable()) {
        setError(
          "Nautilus wallet extension is not installed. Please install it from nautiluswallet.com and try again."
        );
        setLoading(false);
        return;
      }

      await signInNautilus();
      const user = useAuthStore.getState().user;
      if (user) {
        setConnectedAddress(user.ergoAddress);
        toast.success("Nautilus wallet connected!");
        router.push("/playground");
      }
    } catch (err) {
      if (err instanceof Error) {
        if (err.message === "NO_BALANCE") {
          setError(
            "No ERG balance found in your Nautilus wallet. Fund it to use Xergon Network."
          );
        } else if (err.message.includes("rejected")) {
          setError("Connection request was rejected. Please try again.");
        } else {
          setError(err.message);
        }
        toast.error(err.message);
      } else {
        setError("Failed to connect Nautilus wallet.");
        toast.error("Failed to connect Nautilus wallet.");
      }
    } finally {
      setLoading(false);
    }
  }

  const isNautilusReady = method === "nautilus" && nautilusDetected && !connectedAddress;

  return (
    <div className="flex min-h-[calc(100vh-3.5rem)] items-center justify-center px-4">
      <div className="w-full max-w-sm">
        <div className="mb-8 text-center">
          <h1 className="text-2xl font-bold">Connect Wallet</h1>
          <p className="mt-2 text-sm text-surface-800/60">
            Sign in with your Ergo wallet to use Xergon Network
          </p>
        </div>

        {/* Method toggle */}
        <div className="flex rounded-lg border border-surface-200 bg-surface-50 p-1 mb-6">
          <button
            type="button"
            onClick={() => { setMethod("cli"); setError(""); setConnectedAddress(null); }}
            className={`flex-1 rounded-md py-2 text-sm font-medium transition-colors ${
              method === "cli"
                ? "bg-brand-600 text-white shadow-sm"
                : "text-surface-800/60 hover:text-surface-800"
            }`}
          >
            Xergon CLI
          </button>
          <button
            type="button"
            onClick={() => { setMethod("nautilus"); setError(""); setConnectedAddress(null); }}
            className={`flex-1 rounded-md py-2 text-sm font-medium transition-colors ${
              method === "nautilus"
                ? "bg-brand-600 text-white shadow-sm"
                : "text-surface-800/60 hover:text-surface-800"
            }`}
          >
            Nautilus
          </button>
        </div>

        <form onSubmit={handleSubmit} className="space-y-4">
          {error && (
            <div className="rounded-lg border border-danger-500/30 bg-danger-500/10 px-4 py-2 text-sm text-danger-600">
              {error}
            </div>
          )}

          {method === "cli" ? (
            <>
              <div>
                <label
                  htmlFor="publicKey"
                  className="mb-1 block text-sm font-medium text-surface-800/70"
                >
                  Public Key
                </label>
                <input
                  id="publicKey"
                  type="text"
                  required
                  value={publicKey}
                  onChange={(e) => setPublicKey(e.target.value)}
                  className="w-full rounded-lg border border-surface-200 bg-surface-0 px-3 py-2 text-sm font-mono outline-none focus:border-brand-500 focus:ring-2 focus:ring-brand-500/20"
                  placeholder="Paste your public key"
                />
              </div>
              <p className="text-xs text-surface-800/40">
                Run{" "}
                <code className="rounded bg-surface-100 px-1.5 py-0.5 text-xs">
                  xergon wallet address
                </code>{" "}
                in your terminal to get your public key.
              </p>
            </>
          ) : (
            <>
              {/* Nautilus wallet connect card */}
              <div className="rounded-xl border border-surface-200 bg-surface-0 p-6 text-center">
                {connectedAddress ? (
                  <>
                    <div className="mb-3 text-3xl">&#x1F514;</div>
                    <p className="text-sm font-medium text-brand-700 mb-1">
                      Nautilus Connected
                    </p>
                    <p className="text-xs text-surface-800/60 font-mono break-all">
                      {connectedAddress}
                    </p>
                  </>
                ) : nautilusDetected ? (
                  <>
                    <div className="mb-4 text-3xl">&#x1F310;</div>
                    <p className="text-sm text-surface-800/60 mb-1">
                      Nautilus wallet detected
                    </p>
                    <p className="text-xs text-surface-800/40 mb-4">
                      Click the button below to connect your Nautilus wallet and
                      verify your ERG balance.
                    </p>
                  </>
                ) : (
                  <>
                    <div className="mb-4 text-3xl opacity-50">&#x1F50C;</div>
                    <p className="text-sm text-surface-800/60 mb-1">
                      Nautilus wallet not detected
                    </p>
                    <p className="text-xs text-surface-800/40 mb-4">
                      Install the{" "}
                      <a
                        href="https://nautiluswallet.com/"
                        target="_blank"
                        rel="noopener noreferrer"
                        className="text-brand-600 hover:underline"
                      >
                        Nautilus browser extension
                      </a>{" "}
                      and refresh the page to connect.
                    </p>
                  </>
                )}
              </div>
            </>
          )}

          <button
            type="submit"
            disabled={
              loading ||
              (method === "cli" && !publicKey.trim()) ||
              (method === "nautilus" && !nautilusDetected)
            }
            className="w-full rounded-lg bg-brand-600 py-2 text-sm font-medium text-white transition-colors hover:bg-brand-700 disabled:opacity-50"
          >
            {loading
              ? "Verifying..."
              : method === "nautilus"
              ? "Connect Nautilus"
              : "Connect Wallet"}
          </button>
        </form>

        {error?.includes("No ERG balance") && (
          <div className="mt-6 rounded-xl border border-brand-500/30 bg-brand-500/5 p-4">
            <h3 className="text-sm font-semibold text-brand-700 mb-2">
              Fund your wallet
            </h3>
            <p className="text-xs text-surface-800/60 mb-3">
              Send ERG to your wallet address to start using Xergon Network. You
              can get ERG from:
            </p>
            <ul className="space-y-1 text-xs text-surface-800/60">
              <li>
                &#x2022;{" "}
                <a
                  href="https://www.coinex.com/exchange/erg-btc"
                  target="_blank"
                  rel="noopener noreferrer"
                  className="text-brand-600 hover:underline"
                >
                  CoinEx
                </a>
              </li>
              <li>
                &#x2022;{" "}
                <a
                  href="https://www.kucoin.com/trade/ERG-USDT"
                  target="_blank"
                  rel="noopener noreferrer"
                  className="text-brand-600 hover:underline"
                >
                  KuCoin
                </a>
              </li>
              <li>
                &#x2022;{" "}
                <a
                  href="https://tradeogre.com/exchange/ERG-BTC"
                  target="_blank"
                  rel="noopener noreferrer"
                  className="text-brand-600 hover:underline"
                >
                  TradeOgre
                </a>
              </li>
            </ul>
          </div>
        )}

        <p className="mt-6 text-center text-sm text-surface-800/50">
          New to Xergon?{" "}
          <Link
            href="/signin"
            className="font-medium text-brand-600 hover:underline"
          >
            Get started
          </Link>
        </p>
      </div>
    </div>
  );
}
