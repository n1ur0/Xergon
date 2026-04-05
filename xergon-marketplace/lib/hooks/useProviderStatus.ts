/**
 * Custom hook that connects to the WebSocket at /ws/status
 * for real-time provider status updates.
 *
 * Features:
 * - Auto-connects on mount
 * - Exponential backoff reconnection (max 30s)
 * - Cleans up on unmount
 * - Syncs with the Zustand provider-status store
 */

"use client";

import { useEffect, useRef } from "react";
import {
  useProviderStatusStore,
  type ProviderLiveInfo,
  type ProviderLiveStatus,
} from "@/lib/stores/provider-status-store";

interface WsProviderListMessage {
  type: "provider_list";
  providers: Array<{
    provider_id: string;
    status: string;
    latency_ms: number;
    models: string[];
    region: string;
  }>;
  total: number;
}

interface WsProviderStatusMessage {
  type: "provider_status";
  provider_id: string;
  status: string;
  timestamp: number;
}

type WsMessage = WsProviderListMessage | WsProviderStatusMessage;

const INITIAL_BACKOFF_MS = 1_000;
const MAX_BACKOFF_MS = 30_000;
const BACKOFF_MULTIPLIER = 2;

export interface UseProviderStatusReturn {
  providers: ProviderLiveInfo[];
  isConnected: boolean;
  lastUpdate: number | null;
}

export function useProviderStatus(): UseProviderStatusReturn {
  const providersMap = useProviderStatusStore((s) => s.providers);
  const isConnected = useProviderStatusStore((s) => s.isConnected);
  const lastUpdate = useProviderStatusStore((s) => s.lastUpdate);
  const setProviders = useProviderStatusStore((s) => s.setProviders);
  const updateProviderStatus = useProviderStatusStore(
    (s) => s.updateProviderStatus,
  );
  const setConnected = useProviderStatusStore((s) => s.setConnected);
  const clearAll = useProviderStatusStore((s) => s.clearAll);

  const wsRef = useRef<WebSocket | null>(null);
  const backoffRef = useRef(INITIAL_BACKOFF_MS);
  const mountedRef = useRef(true);

  useEffect(() => {
    mountedRef.current = true;

    function connect() {
      if (!mountedRef.current) return;

      // Build WebSocket URL from current location
      const protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
      const url = `${protocol}//${window.location.host}/ws/status`;

      const ws = new WebSocket(url);
      wsRef.current = ws;

      ws.onopen = () => {
        if (!mountedRef.current) {
          ws.close();
          return;
        }
        backoffRef.current = INITIAL_BACKOFF_MS;
        setConnected(true);
      };

      ws.onmessage = (event) => {
        if (!mountedRef.current) return;

        try {
          const msg: WsMessage = JSON.parse(event.data);

          if (msg.type === "provider_list") {
            const providers: ProviderLiveInfo[] = msg.providers.map((p) => ({
              providerId: p.provider_id,
              status: p.status as ProviderLiveStatus,
              latencyMs: p.latency_ms,
              models: p.models,
              region: p.region,
            }));
            setProviders(providers);
          } else if (msg.type === "provider_status") {
            updateProviderStatus(
              msg.provider_id,
              msg.status as ProviderLiveStatus,
            );
          }
        } catch {
          // Ignore malformed messages
        }
      };

      ws.onclose = () => {
        if (!mountedRef.current) return;
        setConnected(false);
        scheduleReconnect();
      };

      ws.onerror = () => {
        // onclose will fire after onerror, so reconnection is handled there
      };
    }

    function scheduleReconnect() {
      const delay = backoffRef.current;
      backoffRef.current = Math.min(
        backoffRef.current * BACKOFF_MULTIPLIER,
        MAX_BACKOFF_MS,
      );

      setTimeout(() => {
        if (mountedRef.current) {
          connect();
        }
      }, delay);
    }

    // Start connection
    connect();

    // Cleanup on unmount
    return () => {
      mountedRef.current = false;
      if (wsRef.current) {
        wsRef.current.close();
        wsRef.current = null;
      }
      clearAll();
    };
  }, [setProviders, updateProviderStatus, setConnected, clearAll]);

  return {
    providers: Array.from(providersMap.values()),
    isConnected,
    lastUpdate,
  };
}
