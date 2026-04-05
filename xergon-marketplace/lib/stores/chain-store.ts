/**
 * Lightweight chain data store (Zustand).
 *
 * Holds live chain state: providers, user balance, node status.
 * Provides actions to refresh each piece of data independently.
 * Used by hooks and components that need access to chain data
 * without re-fetching on every mount.
 */

import { create } from "zustand";
import {
  fetchProviders,
  fetchBalance,
  fetchNodeStatus,
  type ProviderInfo,
  type BalanceResponse,
  type HealthResponse,
} from "@/lib/api/chain";

interface ChainState {
  // ── Data ──
  providers: ProviderInfo[];
  userBalance: BalanceResponse | null;
  nodeStatus: HealthResponse | null;

  // ── Loading flags ──
  providersLoading: boolean;
  balanceLoading: boolean;
  nodeStatusLoading: boolean;

  // ── Actions ──
  refreshProviders: () => Promise<void>;
  refreshBalance: (userPk: string) => Promise<void>;
  refreshNodeStatus: () => Promise<void>;
}

export const useChainStore = create<ChainState>((set) => ({
  // ── Initial state ──
  providers: [],
  userBalance: null,
  nodeStatus: null,
  providersLoading: false,
  balanceLoading: false,
  nodeStatusLoading: false,

  // ── Actions ──

  refreshProviders: async () => {
    set({ providersLoading: true });
    try {
      const providers = await fetchProviders();
      set({ providers, providersLoading: false });
    } catch {
      set({ providersLoading: false });
    }
  },

  refreshBalance: async (userPk: string) => {
    if (!userPk) return;
    set({ balanceLoading: true });
    try {
      const userBalance = await fetchBalance(userPk);
      set({ userBalance, balanceLoading: false });
    } catch {
      set({ balanceLoading: false });
    }
  },

  refreshNodeStatus: async () => {
    set({ nodeStatusLoading: true });
    try {
      const nodeStatus = await fetchNodeStatus();
      set({ nodeStatus, nodeStatusLoading: false });
    } catch {
      set({ nodeStatusLoading: false });
    }
  },
}));
