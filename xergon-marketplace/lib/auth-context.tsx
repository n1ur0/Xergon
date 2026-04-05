"use client";

import {
  createContext,
  useContext,
  useEffect,
  useRef,
  type ReactNode,
} from "react";
import { useAuthStore } from "@/lib/stores/auth";
import type { WalletType } from "@/lib/stores/auth";

interface AuthContextValue {
  isAuthenticated: boolean;
  publicKey: string | null;
  ergoAddress: string | null;
  balance: number;
  walletType: WalletType | null;
  isLoading: boolean;
  signIn: (publicKey: string) => Promise<void>;
  signInNautilus: () => Promise<void>;
  signOut: () => void;
  disconnectNautilus: () => Promise<void>;
  refreshBalance: () => Promise<void>;
}

const AuthContext = createContext<AuthContextValue>({
  isAuthenticated: false,
  publicKey: null,
  ergoAddress: null,
  balance: 0,
  walletType: null,
  isLoading: true,
  signIn: async () => {},
  signInNautilus: async () => {},
  signOut: () => {},
  disconnectNautilus: async () => {},
  refreshBalance: async () => {},
});

export function AuthProvider({ children }: { children: ReactNode }) {
  const user = useAuthStore((s) => s.user);
  const isAuthenticated = useAuthStore((s) => s.isAuthenticated);
  const isLoading = useAuthStore((s) => s.isLoading);
  const walletType = useAuthStore((s) => s.walletType);
  const signIn = useAuthStore((s) => s.signIn);
  const signInNautilus = useAuthStore((s) => s.signInNautilus);
  const signOut = useAuthStore((s) => s.signOut);
  const disconnectNautilus = useAuthStore((s) => s.disconnectNautilus);
  const refreshBalance = useAuthStore((s) => s.refreshBalance);
  const intervalRef = useRef<ReturnType<typeof setInterval> | null>(null);

  // Auto-refresh balance every 30 seconds when authenticated
  useEffect(() => {
    if (isAuthenticated) {
      refreshBalance();
      intervalRef.current = setInterval(refreshBalance, 30_000);
    } else {
      if (intervalRef.current) {
        clearInterval(intervalRef.current);
        intervalRef.current = null;
      }
    }

    return () => {
      if (intervalRef.current) {
        clearInterval(intervalRef.current);
        intervalRef.current = null;
      }
    };
  }, [isAuthenticated, refreshBalance]);

  return (
    <AuthContext.Provider
      value={{
        isAuthenticated,
        publicKey: user?.publicKey ?? null,
        ergoAddress: user?.ergoAddress ?? null,
        balance: user?.balance ?? 0,
        walletType,
        isLoading,
        signIn,
        signInNautilus,
        signOut,
        disconnectNautilus,
        refreshBalance,
      }}
    >
      {children}
    </AuthContext.Provider>
  );
}

export function useAuth(): AuthContextValue {
  return useContext(AuthContext);
}
