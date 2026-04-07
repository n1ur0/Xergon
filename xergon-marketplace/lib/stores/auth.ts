import { create } from "zustand";
import { getWalletPk, setWalletPk, getWalletAddress, setWalletAddress, RELAY_BASE } from "@/lib/api/config";
import { connectNautilus, disconnectNautilus as nautilusDisconnect, isNautilusAvailable } from "@/lib/wallet/nautilus";
import { parseWalletError } from "@/lib/utils/wallet-errors";

export type WalletType = "cli" | "nautilus" | "ergoauth";

export interface WalletUser {
  publicKey: string;
  ergoAddress: string;
  balance: number; // ERG
  walletType: WalletType;
  /** ErgoAuth access token (opaque string or JWT), set when walletType is "ergoauth" */
  accessToken?: string;
}

interface AuthState {
  user: WalletUser | null;
  isAuthenticated: boolean;
  isLoading: boolean;
  walletType: WalletType | null;
  /** Whether to silently attempt reconnection when wallet connection drops */
  autoReconnect: boolean;
  /** Last wallet error message, or null */
  lastWalletError: string | null;
  signIn: (publicKey: string) => Promise<void>;
  signInNautilus: () => Promise<void>;
  /** Connect via ErgoAuth (EIP-28) or generic EIP-12 wallet with address + access token */
  connectErgoWallet: (address: string, accessToken: string, walletType?: "ergoauth") => Promise<void>;
  /** Disconnect an ErgoAuth wallet session */
  disconnectErgoWallet: () => void;
  signOut: () => void;
  disconnectNautilus: () => Promise<void>;
  restore: () => Promise<void>;
  refreshBalance: () => Promise<void>;
  /** Clear the last wallet error */
  clearWalletError: () => void;
}

const WALLET_TYPE_KEY = "xergon_wallet_type";
const AUTH_COOKIE = "xergon-auth-token";

/** Cookie helpers (client-side only) */
function setAuthCookie(publicKey: string, walletType: WalletType) {
  if (typeof window === "undefined") return;
  const payload = btoa(JSON.stringify({ pk: publicKey, wt: walletType }));
  document.cookie = `${AUTH_COOKIE}=${payload};path=/;max-age=${60 * 60 * 24 * 7};SameSite=Lax`;
}

function clearAuthCookie() {
  if (typeof window === "undefined") return;
  document.cookie = `${AUTH_COOKIE}=;path=/;max-age=0;SameSite=Lax`;
}

function getStoredWalletType(): WalletType | null {
  if (typeof window === "undefined") return null;
  const raw = localStorage.getItem(WALLET_TYPE_KEY);
  if (raw === "cli" || raw === "nautilus" || raw === "ergoauth") return raw;
  return null;
}

function setStoredWalletType(type: WalletType | null) {
  if (typeof window === "undefined") return;
  if (type) {
    localStorage.setItem(WALLET_TYPE_KEY, type);
  } else {
    localStorage.removeItem(WALLET_TYPE_KEY);
  }
}

export const useAuthStore = create<AuthState>()((set, get) => ({
  user: null,
  isAuthenticated: false,
  isLoading: true,
  walletType: null,
  autoReconnect: true,
  lastWalletError: null,

  clearWalletError: () => {
    set({ lastWalletError: null });
  },

  signIn: async (publicKey: string) => {
    // Verify the public key has a staking box (balance > 0) via relay
    const res = await fetch(`${RELAY_BASE}/balance/${publicKey}`);
    if (!res.ok) {
      const body = await res.json().catch(() => ({ error: "Verification failed" }));
      throw new Error(body.error || "Failed to verify wallet");
    }

    const data = await res.json();
    const ergoAddress = data.ergo_address || publicKey;
    const balance = data.balance_erg ?? 0;

    if (balance <= 0) {
      throw new Error("NO_BALANCE");
    }

    const user: WalletUser = {
      publicKey,
      ergoAddress,
      balance,
      walletType: "cli",
    };

    setWalletPk(publicKey);
    setWalletAddress(ergoAddress);
    setStoredWalletType("cli");
    setAuthCookie(publicKey, "cli");
    set({ user, isAuthenticated: true, isLoading: false, walletType: "cli", lastWalletError: null });
  },

  signInNautilus: async () => {
    if (!isNautilusAvailable()) {
      set({
        lastWalletError:
          "Wallet extension not found. Install Nautilus from https://nautiluswallet.com/.",
      });
      throw new Error(
        "Wallet extension not found. Install Nautilus from https://nautiluswallet.com/."
      );
    }

    try {
      // Connect to Nautilus and get the Ergo address
      const ergoAddress = await connectNautilus();

      // Verify balance via relay
      const res = await fetch(`${RELAY_BASE}/balance/${ergoAddress}`);
      if (!res.ok) {
        await nautilusDisconnect();
        const body = await res.json().catch(() => ({ error: "Verification failed" }));
        set({ lastWalletError: body.error || "Failed to verify wallet via Nautilus" });
        throw new Error(body.error || "Failed to verify wallet via Nautilus");
      }

      const data = await res.json();
      const balance = data.balance_erg ?? 0;

      if (balance <= 0) {
        await nautilusDisconnect();
        set({ lastWalletError: "NO_BALANCE" });
        throw new Error("NO_BALANCE");
      }

      // For Nautilus, we use the ergo address as the public key identifier
      // The relay already verified it, and Nautilus manages signing internally
      const publicKey = data.public_key || ergoAddress;

      const user: WalletUser = {
        publicKey,
        ergoAddress,
        balance,
        walletType: "nautilus",
      };

      setWalletPk(publicKey);
      setWalletAddress(ergoAddress);
      setStoredWalletType("nautilus");
      setAuthCookie(publicKey, "nautilus");
      set({ user, isAuthenticated: true, isLoading: false, walletType: "nautilus", lastWalletError: null });
    } catch (err) {
      // Parse the error and set a user-friendly message
      const parsed = parseWalletError(err);
      set({ lastWalletError: parsed.message });

      // Re-throw the parsed message (not the raw error) so callers get the friendly text
      throw new Error(parsed.message);
    }
  },

  connectErgoWallet: async (address: string, accessToken: string, type: WalletType = "ergoauth") => {
    // Verify balance via relay using the Ergo address
    let balance = 0;
    try {
      const res = await fetch(`${RELAY_BASE}/balance/${address}`);
      if (res.ok) {
        const data = await res.json();
        balance = data.balance_erg ?? 0;
      }
    } catch {
      // Relay may be unavailable — proceed with zero balance
    }

    const user: WalletUser = {
      publicKey: address, // Use address as public key identifier for ErgoAuth
      ergoAddress: address,
      balance,
      walletType: type,
      accessToken,
    };

    setWalletPk(address);
    setWalletAddress(address);
    setStoredWalletType(type);
    setAuthCookie(address, type);
    set({
      user,
      isAuthenticated: true,
      isLoading: false,
      walletType: type,
      lastWalletError: null,
    });
  },

  disconnectErgoWallet: () => {
    setWalletPk(null);
    setWalletAddress(null);
    setStoredWalletType(null);
    clearAuthCookie();
    set({ user: null, isAuthenticated: false, walletType: null, lastWalletError: null });
  },

  signOut: () => {
    setWalletPk(null);
    setWalletAddress(null);
    setStoredWalletType(null);
    clearAuthCookie();
    set({ user: null, isAuthenticated: false, walletType: null, lastWalletError: null });
  },

  disconnectNautilus: async () => {
    try {
      await nautilusDisconnect();
    } catch {
      // Wallet may already be disconnected
    }
    setWalletPk(null);
    setWalletAddress(null);
    setStoredWalletType(null);
    clearAuthCookie();
    set({ user: null, isAuthenticated: false, walletType: null, lastWalletError: null });
  },

  restore: async () => {
    const pk = getWalletPk();
    const storedType = getStoredWalletType();
    if (!pk) {
      set({ isLoading: false });
      return;
    }

    // If wallet type is nautilus, try auto-reconnect silently
    if (storedType === "nautilus" && useAuthStore.getState().autoReconnect && isNautilusAvailable()) {
      try {
        const ergoAddress = await connectNautilus();

        // Successfully reconnected — verify balance via relay
        const res = await fetch(`${RELAY_BASE}/balance/${ergoAddress}`);
        if (res.ok) {
          const data = await res.json();
          const balance = data.balance_erg ?? 0;
          const publicKey = data.public_key || ergoAddress;

          setWalletPk(publicKey);
          setWalletAddress(ergoAddress);
          set({
            user: {
              publicKey,
              ergoAddress,
              balance,
              walletType: "nautilus",
            },
            isAuthenticated: true,
            isLoading: false,
            walletType: "nautilus",
            lastWalletError: null,
          });
          return;
        }
      } catch {
        // Auto-reconnect failed — fall through to relay-based restore
      }
    }

    // Fallback: restore from relay using stored PK
    try {
      const res = await fetch(`${RELAY_BASE}/balance/${pk}`);
      if (!res.ok) {
        // PK no longer valid or relay down
        setWalletPk(null);
        setWalletAddress(null);
        setStoredWalletType(null);
        set({ user: null, isAuthenticated: false, isLoading: false, walletType: null });
        return;
      }

      const data = await res.json();
      const ergoAddress = data.ergo_address || getWalletAddress() || pk;
      const balance = data.balance_erg ?? 0;

      setAuthCookie(pk, storedType || "cli");
      set({
        user: {
          publicKey: pk,
          ergoAddress,
          balance,
          walletType: storedType || "cli",
        },
        isAuthenticated: true,
        isLoading: false,
        walletType: storedType || "cli",
      });
    } catch {
      // Relay down — keep PK but mark loading as done
      const address = getWalletAddress();
      set({
        user: address ? { publicKey: pk, ergoAddress: address, balance: 0, walletType: storedType || "cli" } : null,
        isAuthenticated: !!address,
        isLoading: false,
      });
    }
  },

  refreshBalance: async () => {
    const { user } = get();
    if (!user) return;

    try {
      const res = await fetch(`${RELAY_BASE}/balance/${user.publicKey}`);
      if (!res.ok) return;

      const data = await res.json();
      set({
        user: {
          ...user,
          balance: data.balance_erg ?? user.balance,
        },
      });
    } catch {
      // Silently fail — balance will update on next refresh
    }
  },
}));
