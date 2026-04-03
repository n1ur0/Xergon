import { create } from "zustand";
import { getToken, setToken } from "@/lib/api/config";
import { api } from "@/lib/api/client";

export interface User {
  id: string;
  email: string;
  name?: string;
  credits: number;
  tier: string;
  ergoAddress?: string | null;
}

interface AuthState {
  user: User | null;
  isAuthenticated: boolean;
  isLoading: boolean;
  setUser: (user: User | null) => void;
  logout: () => void;
  setLoading: (loading: boolean) => void;
  signup: (email: string, password: string, name?: string) => Promise<void>;
  login: (email: string, password: string) => Promise<void>;
  restore: () => Promise<void>;
  refreshCredits: () => Promise<void>;
}

interface AuthResponse {
  token: string;
  user: {
    id: string;
    email: string;
    name?: string;
    tier: string;
    credits_usd: number;
  };
}

interface MeResponse {
  id: string;
  email: string;
  name?: string;
  tier: string;
  credits_usd: number;
  ergo_address?: string | null;
}

function toUser(data: { id: string; email: string; name?: string; tier: string; credits_usd: number; ergo_address?: string | null }): User {
  return {
    id: data.id,
    email: data.email,
    name: data.name,
    tier: data.tier,
    credits: data.credits_usd,
    ergoAddress: data.ergo_address ?? null,
  };
}

export const useAuthStore = create<AuthState>()((set, get) => ({
  user: null,
  isAuthenticated: false,
  isLoading: true,

  setUser: (user) =>
    set({
      user,
      isAuthenticated: user !== null,
    }),

  logout: () => {
    setToken(null);
    set({ user: null, isAuthenticated: false });
  },

  setLoading: (isLoading) => set({ isLoading }),

  signup: async (email, password, name) => {
    const data = await api.post<AuthResponse>("/auth/signup", { email, password, name });
    setToken(data.token);
    set({ user: toUser(data.user), isAuthenticated: true });
  },

  login: async (email, password) => {
    const data = await api.post<AuthResponse>("/auth/login", { email, password });
    setToken(data.token);
    set({ user: toUser(data.user), isAuthenticated: true });
  },

  restore: async () => {
    const token = getToken();
    if (!token) {
      set({ isLoading: false });
      return;
    }

    try {
      const data = await api.get<MeResponse>("/auth/me");
      set({ user: toUser(data), isAuthenticated: true, isLoading: false });
    } catch {
      // Token expired or invalid
      setToken(null);
      set({ user: null, isAuthenticated: false, isLoading: false });
    }
  },

  refreshCredits: async () => {
    const { user } = get();
    if (!user) return;

    try {
      const data = await api.get<MeResponse>("/auth/me");
      set({
        user: { ...get().user!, credits: data.credits_usd },
      });
    } catch {
      // Silently fail — credits will update on next auth check
    }
  },
}));
