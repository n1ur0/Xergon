"use client";

import { useState, useEffect, useRef, useCallback } from "react";

// ── Event Types ──

export type RentalEventType =
  | "rental_created"
  | "rental_active"
  | "rental_completed"
  | "rental_failed"
  | "provider_heartbeat";

export interface RentalCreatedEvent {
  type: "rental_created";
  rentalId: string;
  gpuId: string;
  providerId: string;
  timestamp: string;
}

export interface RentalActiveEvent {
  type: "rental_active";
  rentalId: string;
  timestamp: string;
}

export interface RentalCompletedEvent {
  type: "rental_completed";
  rentalId: string;
  gpuId: string;
  providerId: string;
  totalTokens: number;
  totalCost: number;
  timestamp: string;
}

export interface RentalFailedEvent {
  type: "rental_failed";
  rentalId: string;
  reason: string;
  timestamp: string;
}

export interface ProviderHeartbeatEvent {
  type: "provider_heartbeat";
  providerId: string;
  status: string;
  models: string[];
  timestamp: string;
}

export type RentalEvent =
  | RentalCreatedEvent
  | RentalActiveEvent
  | RentalCompletedEvent
  | RentalFailedEvent
  | ProviderHeartbeatEvent;

export interface UseRealtimeUpdatesOptions {
  /** Max number of events to keep in buffer (default 100) */
  maxEvents?: number;
  /** Enable/disable the connection (default true) */
  enabled?: boolean;
  /** Base URL for the SSE endpoint (default /api/xergon-relay/events) */
  url?: string;
}

export interface UseRealtimeUpdatesReturn {
  events: RentalEvent[];
  isConnected: boolean;
  lastEvent: RentalEvent | null;
  /** Manually reconnect */
  reconnect: () => void;
}

// ── Hook ──

export function useRealtimeUpdates(
  options: UseRealtimeUpdatesOptions = {},
): UseRealtimeUpdatesReturn {
  const {
    maxEvents = 100,
    enabled = true,
    url = "/api/xergon-relay/events",
  } = options;

  const [events, setEvents] = useState<RentalEvent[]>([]);
  const [isConnected, setIsConnected] = useState(false);
  const [lastEvent, setLastEvent] = useState<RentalEvent | null>(null);
  const eventSourceRef = useRef<EventSource | null>(null);
  const reconnectTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const retryCountRef = useRef(0);
  const enabledRef = useRef(enabled);
  const urlRef = useRef(url);

  // Keep refs in sync
  useEffect(() => {
    enabledRef.current = enabled;
  }, [enabled]);
  useEffect(() => {
    urlRef.current = url;
  }, [url]);

  const cleanup = useCallback(() => {
    if (eventSourceRef.current) {
      eventSourceRef.current.close();
      eventSourceRef.current = null;
    }
    if (reconnectTimeoutRef.current) {
      clearTimeout(reconnectTimeoutRef.current);
      reconnectTimeoutRef.current = null;
    }
    setIsConnected(false);
  }, []);

  const connect = useCallback(() => {
    if (!enabledRef.current) return;

    cleanup();

    const es = new EventSource(urlRef.current);
    eventSourceRef.current = es;

    es.onopen = () => {
      setIsConnected(true);
      retryCountRef.current = 0;
    };

    es.onmessage = (e) => {
      try {
        const data: RentalEvent = JSON.parse(e.data);
        setEvents((prev) => {
          const next = [...prev, data];
          return next.length > maxEvents ? next.slice(-maxEvents) : next;
        });
        setLastEvent(data);
      } catch {
        // Ignore malformed events
      }
    };

    // Handle named event types
    const eventTypes: RentalEventType[] = [
      "rental_created",
      "rental_active",
      "rental_completed",
      "rental_failed",
      "provider_heartbeat",
    ];
    for (const eventType of eventTypes) {
      es.addEventListener(eventType, (e: MessageEvent) => {
        try {
          const data: RentalEvent = JSON.parse(e.data);
          setEvents((prev) => {
            const next = [...prev, data];
            return next.length > maxEvents ? next.slice(-maxEvents) : next;
          });
          setLastEvent(data);
        } catch {
          // Ignore malformed events
        }
      });
    }

    es.onerror = () => {
      setIsConnected(false);
      eventSourceRef.current?.close();
      eventSourceRef.current = null;

      // Exponential backoff: 1s, 2s, 4s, 8s, max 30s
      const delay = Math.min(1000 * Math.pow(2, retryCountRef.current), 30_000);
      retryCountRef.current += 1;

      reconnectTimeoutRef.current = setTimeout(() => {
        if (enabledRef.current) {
          connect();
        }
      }, delay);
    };
  }, [cleanup, maxEvents]);

  // Connect on mount / when enabled changes
  useEffect(() => {
    if (enabled) {
      connect();
    } else {
      cleanup();
    }
    return cleanup;
  }, [enabled, connect, cleanup]);

  const reconnect = useCallback(() => {
    retryCountRef.current = 0;
    cleanup();
    if (enabledRef.current) {
      connect();
    }
  }, [cleanup, connect]);

  return { events, isConnected, lastEvent, reconnect };
}
