/**
 * Zustand store for real-time WebSocket provider status.
 *
 * Holds the live provider status map updated via WebSocket /ws/status.
 * Used by the useProviderStatus hook and components that need
 * real-time status indicators.
 */

import { create } from "zustand";

export type ProviderLiveStatus = "online" | "offline";

export interface ProviderLiveInfo {
  providerId: string;
  status: ProviderLiveStatus;
  latencyMs: number;
  models: string[];
  region: string;
}

interface ProviderStatusState {
  // ── Data ──
  providers: Map<string, ProviderLiveInfo>;
  isConnected: boolean;
  lastUpdate: number | null;

  // ── Actions ──
  setProviders: (providers: ProviderLiveInfo[]) => void;
  updateProviderStatus: (
    providerId: string,
    status: ProviderLiveStatus,
  ) => void;
  setConnected: (connected: boolean) => void;
  clearAll: () => void;
}

export const useProviderStatusStore = create<ProviderStatusState>((set) => ({
  providers: new Map(),
  isConnected: false,
  lastUpdate: null,

  setProviders: (providers) =>
    set(() => {
      const map = new Map<string, ProviderLiveInfo>();
      for (const p of providers) {
        map.set(p.providerId, p);
      }
      return {
        providers: map,
        lastUpdate: Date.now(),
      };
    }),

  updateProviderStatus: (providerId, status) =>
    set((state) => {
      const updated = new Map(state.providers);
      const existing = updated.get(providerId);
      updated.set(providerId, {
        providerId,
        status,
        latencyMs: existing?.latencyMs ?? 0,
        models: existing?.models ?? [],
        region: existing?.region ?? "",
      });
      return {
        providers: updated,
        lastUpdate: Date.now(),
      };
    }),

  setConnected: (connected) => set({ isConnected: connected }),

  clearAll: () =>
    set({
      providers: new Map(),
      isConnected: false,
      lastUpdate: null,
    }),
}));
