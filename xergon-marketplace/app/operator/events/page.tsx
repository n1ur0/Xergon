"use client";

import { useState, useEffect, useRef, useCallback } from "react";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface EventEntry {
  id: string;
  type: string;
  data: Record<string, unknown>;
  timestamp: Date;
  raw: string;
}

const MAX_EVENTS = 200;

const EVENT_TYPE_COLORS: Record<string, string> = {
  provider_registered: "bg-accent-100 text-accent-700",
  provider_online: "bg-accent-100 text-accent-700",
  provider_offline: "bg-danger-100 text-danger-700",
  provider_degraded: "bg-yellow-100 text-yellow-700",
  request: "bg-brand-50 text-brand-700",
  request_complete: "bg-brand-50 text-brand-700",
  error: "bg-danger-100 text-danger-700",
  health_check: "bg-surface-100 text-surface-800/60",
  model_added: "bg-purple-100 text-purple-700",
  model_removed: "bg-orange-100 text-orange-700",
  stake: "bg-emerald-100 text-emerald-700",
  unstake: "bg-orange-100 text-orange-700",
  slash: "bg-danger-100 text-danger-700",
};

const EVENT_TYPE_ICONS: Record<string, string> = {
  provider_registered: "+",
  provider_online: "^",
  provider_offline: "v",
  provider_degraded: "!",
  request: "->",
  request_complete: "<-",
  error: "x",
  health_check: "~",
  model_added: "+",
  model_removed: "-",
  stake: "$",
  unstake: "$",
  slash: "!",
};

function formatTime(date: Date): string {
  return date.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit", fractionalSecondDigits: 3 });
}

function formatDuration(ms: number): string {
  if (ms < 1000) return `${ms}ms`;
  if (ms < 60_000) return `${(ms / 1000).toFixed(1)}s`;
  return `${Math.floor(ms / 60_000)}m ${Math.floor((ms % 60_000) / 1000)}s`;
}

// ---------------------------------------------------------------------------
// Main component
// ---------------------------------------------------------------------------

export default function EventsPage() {
  const [events, setEvents] = useState<EventEntry[]>([]);
  const [connected, setConnected] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [filter, setFilter] = useState("all");
  const [paused, setPaused] = useState(false);
  const [autoScroll, setAutoScroll] = useState(true);
  const eventSourceRef = useRef<EventSource | null>(null);
  const scrollRef = useRef<HTMLDivElement>(null);
  const eventIdRef = useRef(0);

  // Event types from current events
  const eventTypes = Array.from(
    new Set(events.map((e) => e.type)),
  ).sort();

  const filtered = filter === "all" ? events : events.filter((e) => e.type === filter);

  // Auto-scroll
  useEffect(() => {
    if (autoScroll && scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [filtered.length, autoScroll]);

  const connectSSE = useCallback(() => {
    // Close existing connection
    if (eventSourceRef.current) {
      eventSourceRef.current.close();
    }

    setError(null);

    const es = new EventSource("/api/xergon-relay/events");
    eventSourceRef.current = es;

    es.onopen = () => {
      setConnected(true);
      setError(null);
    };

    es.onerror = () => {
      setConnected(false);
      // EventSource auto-reconnects, but we note the error
      if (!events.length) {
        setError("Unable to connect to event stream. Retrying...");
      }
    };

    // Listen for named event types
    const eventTypeNames = [
      "provider_registered",
      "provider_online",
      "provider_offline",
      "provider_degraded",
      "request",
      "request_complete",
      "error",
      "health_check",
      "model_added",
      "model_removed",
      "stake",
      "unstake",
      "slash",
    ];

    for (const type of eventTypeNames) {
      es.addEventListener(type, (e) => {
        if (paused) return;
        try {
          const data = JSON.parse((e as MessageEvent).data);
          const entry: EventEntry = {
            id: String(++eventIdRef.current),
            type,
            data,
            timestamp: new Date(),
            raw: (e as MessageEvent).data,
          };
          setEvents((prev) => [entry, ...prev].slice(0, MAX_EVENTS));
        } catch {
          // ignore malformed events
        }
      });
    }

    // Also listen for generic 'message' events
    es.onmessage = (e) => {
      if (paused) return;
      try {
        const data = JSON.parse(e.data);
        // Skip error events handled by addEventListener
        if (data?.message && data?.reconnect) return;

        const type = data?.type ?? data?.event ?? "message";
        const entry: EventEntry = {
          id: String(++eventIdRef.current),
          type,
          data,
          timestamp: new Date(),
          raw: e.data,
        };
        setEvents((prev) => [entry, ...prev].slice(0, MAX_EVENTS));
      } catch {
        // ignore
      }
    };

    return () => {
      es.close();
      eventSourceRef.current = null;
      setConnected(false);
    };
  }, [paused, events.length]);

  useEffect(() => {
    const cleanup = connectSSE();
    return cleanup;
  }, [connectSSE]);

  // Handle scroll to detect if user scrolled up
  const handleScroll = () => {
    if (!scrollRef.current) return;
    const { scrollTop, scrollHeight, clientHeight } = scrollRef.current;
    const atBottom = scrollHeight - scrollTop - clientHeight < 50;
    setAutoScroll(atBottom);
  };

  // ---------------------------------------------------------------------------
  // Render
  // ---------------------------------------------------------------------------

  return (
    <div className="space-y-6">
      <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-4">
        <div>
          <h1 className="text-2xl font-bold text-surface-900">Events</h1>
          <p className="text-sm text-surface-800/50 mt-1">
            Real-time network events and activity log.
          </p>
        </div>
        <div className="flex items-center gap-3">
          {/* Connection indicator */}
          <div className="flex items-center gap-2 text-sm">
            <span className={`h-2.5 w-2.5 rounded-full ${connected ? "bg-accent-500 animate-pulse" : "bg-surface-400"}`} />
            <span className={connected ? "text-accent-600" : "text-surface-800/40"}>
              {connected ? "Connected" : "Disconnected"}
            </span>
          </div>

          <button
            onClick={() => setPaused((p) => !p)}
            className={`rounded-lg border px-3 py-2 text-sm font-medium transition-colors ${
              paused
                ? "border-brand-200 bg-brand-50 text-brand-700"
                : "border-surface-200 text-surface-800/70 hover:bg-surface-100"
            }`}
          >
            {paused ? "Resume" : "Pause"}
          </button>

          <button
            onClick={() => setEvents([])}
            className="rounded-lg border border-surface-200 px-3 py-2 text-sm font-medium text-surface-800/70 hover:bg-surface-100 transition-colors"
          >
            Clear
          </button>

          {!autoScroll && (
            <button
              onClick={() => { setAutoScroll(true); scrollRef.current?.scrollTo({ top: scrollRef.current.scrollHeight }); }}
              className="rounded-lg bg-brand-600 px-3 py-2 text-sm font-medium text-white hover:bg-brand-700 transition-colors"
            >
              Scroll to bottom
            </button>
          )}
        </div>
      </div>

      {/* Event type filter */}
      {eventTypes.length > 0 && (
        <div className="flex flex-wrap gap-1.5">
          <button
            onClick={() => setFilter("all")}
            className={`rounded-full px-3 py-1.5 text-xs font-medium transition-colors ${filter === "all" ? "bg-brand-600 text-white" : "bg-surface-100 text-surface-800/60 hover:bg-surface-200"}`}
          >
            All ({events.length})
          </button>
          {eventTypes.map((type) => {
            const count = events.filter((e) => e.type === type).length;
            return (
              <button
                key={type}
                onClick={() => setFilter(type)}
                className={`rounded-full px-3 py-1.5 text-xs font-medium transition-colors ${filter === type ? "bg-brand-600 text-white" : "bg-surface-100 text-surface-800/60 hover:bg-surface-200"}`}
              >
                {type} ({count})
              </button>
            );
          })}
        </div>
      )}

      {/* Error state */}
      {error && (
        <div className="rounded-xl border border-yellow-200 bg-yellow-50/50 p-4 text-sm text-yellow-700">
          {error}
        </div>
      )}

      {/* Event feed */}
      <div
        ref={scrollRef}
        onScroll={handleScroll}
        className="rounded-xl border border-surface-200 bg-surface-0 overflow-y-auto"
        style={{ maxHeight: "calc(100vh - 300px)", minHeight: "400px" }}
      >
        {filtered.length === 0 ? (
          <div className="flex flex-col items-center justify-center py-16 text-surface-800/30">
            <svg className="w-12 h-12 mb-3" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
              <path d="M12 8v4l3 3" /><circle cx="12" cy="12" r="10" />
            </svg>
            <p className="text-sm font-medium">Waiting for events...</p>
            <p className="text-xs mt-1">
              {connected
                ? "Connected to relay event stream"
                : "Connecting to relay event stream..."}
            </p>
          </div>
        ) : (
          <div className="divide-y divide-surface-100">
            {filtered.map((event) => (
              <div
                key={event.id}
                className="px-5 py-3 hover:bg-surface-50 transition-colors"
              >
                <div className="flex items-center gap-3">
                  {/* Icon */}
                  <div className={`flex items-center justify-center w-7 h-7 rounded-full text-xs font-bold flex-shrink-0 ${
                    EVENT_TYPE_COLORS[event.type] ?? "bg-surface-100 text-surface-800/50"
                  }`}>
                    {EVENT_TYPE_ICONS[event.type] ?? "?"}
                  </div>

                  {/* Content */}
                  <div className="flex-1 min-w-0">
                    <div className="flex items-center gap-2">
                      <span className="text-sm font-medium text-surface-900">{formatEventTitle(event)}</span>
                      <span className={`inline-flex rounded-full px-2 py-0.5 text-[10px] font-medium ${
                        EVENT_TYPE_COLORS[event.type] ?? "bg-surface-100 text-surface-800/50"
                      }`}>
                        {event.type}
                      </span>
                    </div>
                    <p className="text-xs text-surface-800/40 mt-0.5 truncate">
                      {formatEventData(event)}
                    </p>
                  </div>

                  {/* Timestamp */}
                  <span className="text-xs text-surface-800/30 font-mono flex-shrink-0">
                    {formatTime(event.timestamp)}
                  </span>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>

      {/* Footer stats */}
      {events.length > 0 && (
        <div className="flex items-center justify-between text-xs text-surface-800/30 px-1">
          <span>{filtered.length} events displayed</span>
          <span>Max {MAX_EVENTS} events buffered</span>
        </div>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Event formatting helpers
// ---------------------------------------------------------------------------

function formatEventTitle(event: EventEntry): string {
  const { type, data } = event;
  switch (type) {
    case "provider_registered":
      return `Provider registered: ${data.provider_name ?? data.name ?? data.endpoint ?? "Unknown"}`;
    case "provider_online":
      return `Provider online: ${data.provider_name ?? data.name ?? data.endpoint ?? "Unknown"}`;
    case "provider_offline":
      return `Provider offline: ${data.provider_name ?? data.name ?? data.endpoint ?? "Unknown"}`;
    case "provider_degraded":
      return `Provider degraded: ${data.provider_name ?? data.name ?? data.endpoint ?? "Unknown"}`;
    case "request":
      return `Request: ${data.model ?? data.model_id ?? "Unknown model"}`;
    case "request_complete":
      return `Request complete: ${data.model ?? data.model_id ?? "Unknown model"}`;
    case "error":
      return `Error: ${data.message ?? data.error ?? "Unknown"}`;
    case "health_check":
      return `Health check: ${data.provider ?? data.endpoint ?? "Unknown"}`;
    case "model_added":
      return `Model added: ${data.model ?? data.model_id ?? "Unknown"}`;
    case "model_removed":
      return `Model removed: ${data.model ?? data.model_id ?? "Unknown"}`;
    case "stake":
      return `Stake: ${data.provider ?? "Unknown"} - ${data.amount ?? "N/A"} ERG`;
    case "unstake":
      return `Unstake: ${data.provider ?? "Unknown"} - ${data.amount ?? "N/A"} ERG`;
    case "slash":
      return `Slash: ${data.provider ?? "Unknown"}`;
    default:
      return type.replace(/_/g, " ").replace(/\b\w/g, (c) => c.toUpperCase());
  }
}

function formatEventData(event: EventEntry): string {
  const { type, data } = event;

  // Extract interesting details
  const parts: string[] = [];

  if (data.provider_name) parts.push(`provider: ${data.provider_name}`);
  else if (data.name) parts.push(`name: ${data.name}`);
  if (data.endpoint) parts.push(`endpoint: ${data.endpoint}`);
  if (data.region) parts.push(`region: ${data.region}`);
  if (data.model || data.model_id) parts.push(`model: ${data.model ?? data.model_id}`);
  if (data.latency_ms ?? data.latency) parts.push(`latency: ${data.latency_ms ?? data.latency}ms`);
  if (data.tokens_prompt ?? data.input_tokens) parts.push(`input: ${data.tokens_prompt ?? data.input_tokens} tok`);
  if (data.tokens_completion ?? data.output_tokens) parts.push(`output: ${data.tokens_completion ?? data.output_tokens} tok`);
  if (typeof data.duration_ms === "number") parts.push(`duration: ${formatDuration(data.duration_ms)}`);
  if (data.amount) parts.push(`amount: ${data.amount} ERG`);
  if (data.status) parts.push(`status: ${data.status}`);

  // For error type, include the full message
  if (type === "error" && data.message) return data.message as string;

  // Fallback to raw JSON
  if (parts.length === 0) {
    try {
      return JSON.stringify(data, null, 2).slice(0, 200);
    } catch {
      return event.raw.slice(0, 200);
    }
  }

  return parts.join(" | ");
}
