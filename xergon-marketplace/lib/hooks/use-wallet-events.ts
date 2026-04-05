/**
 * useWalletEvents -- React hook that listens for EIP-12 wallet events.
 *
 * Monitors for account_change, disconnect, and lock events from the Nautilus
 * wallet extension. Falls back to polling isConnected() every 30s when
 * the extension doesn't fire events (e.g. after a background reload).
 *
 * Usage:
 *   useWalletEvents()  // call inside your app shell or layout
 */

"use client";

import { useEffect, useRef, useCallback } from "react";
import { toast } from "sonner";
import { useAuthStore } from "@/lib/stores/auth";
import { isNautilusAvailable, isNautilusConnected, connectNautilus } from "@/lib/wallet/nautilus";

const POLL_INTERVAL_MS = 30_000;
const WALLET_NAME = "nautilus";

/**
 * Hook that listens for wallet lifecycle events and keeps the auth store in sync.
 *
 * - account_change: refresh balance, update auth, show toast
 * - disconnect: sign out, clear state, show toast
 * - lock: show warning toast
 * - Polling fallback: every 30s, verify isConnected()
 */
export function useWalletEvents(): void {
  const isAuthenticated = useAuthStore((s) => s.isAuthenticated);
  const walletType = useAuthStore((s) => s.walletType);
  const refreshBalance = useAuthStore((s) => s.refreshBalance);
  const signOut = useAuthStore((s) => s.signOut);
  const autoReconnect = useAuthStore((s) => s.autoReconnect);

  // Keep refs to avoid stale closures in event handlers
  const refreshBalanceRef = useRef(refreshBalance);
  const signOutRef = useRef(signOut);
  const autoReconnectRef = useRef(autoReconnect);

  useEffect(() => {
    refreshBalanceRef.current = refreshBalance;
    signOutRef.current = signOut;
    autoReconnectRef.current = autoReconnect;
  }, [refreshBalance, signOut, autoReconnect]);

  const handleAccountChange = useCallback(() => {
    toast.info("Account switched", {
      description: "Your wallet account was changed. Refreshing balance...",
    });
    refreshBalanceRef.current();
  }, []);

  const handleDisconnect = useCallback(() => {
    toast.info("Wallet disconnected", {
      description: "Your wallet has been disconnected.",
    });
    signOutRef.current();
  }, []);

  const handleLock = useCallback(() => {
    toast.warning("Wallet locked", {
      description: "Please unlock your Nautilus wallet to continue.",
    });
  }, []);

  useEffect(() => {
    // Only listen when authenticated with Nautilus
    if (!isAuthenticated || walletType !== "nautilus") return;
    if (typeof window === "undefined") return;

    const connector = window.ergoConnector?.[WALLET_NAME];
    if (!connector) return;

    // Nautilus may expose event listeners via addEventListener or named callbacks.
    // Try both patterns since extension APIs vary between versions.
    const nautilus = connector as unknown as {
      addEventListener?: (event: string, handler: () => void) => void;
      removeEventListener?: (event: string, handler: () => void) => void;
      on?: Record<string, (() => void) | undefined>;
    };

    // Event listener pattern (preferred)
    if (typeof nautilus.addEventListener === "function") {
      nautilus.addEventListener("account_change", handleAccountChange);
      nautilus.addEventListener("disconnect", handleDisconnect);
      nautilus.addEventListener("lock", handleLock);

      return () => {
        nautilus.removeEventListener?.("account_change", handleAccountChange);
        nautilus.removeEventListener?.("disconnect", handleDisconnect);
        nautilus.removeEventListener?.("lock", handleLock);
      };
    }

    // No cleanup needed for the polling-only path below
    return undefined;
  }, [isAuthenticated, walletType, handleAccountChange, handleDisconnect, handleLock]);

  // Polling fallback: every 30s, verify the wallet is still connected.
  // This catches extension reloads, background kills, etc.
  useEffect(() => {
    if (!isAuthenticated || walletType !== "nautilus") return;

    let cancelled = false;

    const poll = async () => {
      if (cancelled) return;

      try {
        const connected = await isNautilusConnected();

        if (cancelled) return;

        if (!connected) {
          // Wallet is no longer connected — attempt auto-reconnect if enabled
          if (autoReconnectRef.current && isNautilusAvailable()) {
            try {
              const address = await connectNautilus();
              if (!cancelled) {
                // Update the stored address silently
                const store = useAuthStore.getState();
                const user = store.user;
                if (user) {
                  useAuthStore.setState({
                    user: { ...user, ergoAddress: address },
                  });
                }
                toast.success("Wallet reconnected", {
                  description: "Your wallet connection was restored.",
                });
                refreshBalanceRef.current();
              }
            } catch {
              // Auto-reconnect failed — sign out
              if (!cancelled) {
                toast.error("Connection lost", {
                  description:
                    "Could not reconnect your wallet. Please sign in again.",
                });
                signOutRef.current();
              }
            }
          } else {
            // No auto-reconnect — just sign out
            if (!cancelled) {
              toast.error("Connection lost", {
                description: "Your wallet is no longer connected.",
              });
              signOutRef.current();
            }
          }
        }
      } catch {
        // isConnected() itself threw — wallet extension may be gone
        // Don't do anything aggressive; next poll will retry
      }
    };

    const intervalId = setInterval(poll, POLL_INTERVAL_MS);

    return () => {
      cancelled = true;
      clearInterval(intervalId);
    };
  }, [isAuthenticated, walletType]);
}
