"use client";

import { useState, useEffect, useMemo, useCallback } from "react";
import { cn } from "@/lib/utils";
import {
  fetchMyRentals,
  nanoergToErg,
  type GpuRental,
} from "@/lib/api/gpu";
import { useAuthStore } from "@/lib/stores/auth";
import { useRealtimeUpdates, type RentalEvent } from "@/hooks/use-realtime-updates";
import {
  RentalStatusBadge,
  deriveStatusFromEvent,
  type RentalStatus,
} from "@/components/gpu/RentalStatusBadge";
import {
  Cpu,
  Clock,
  MapPin,
  Wifi,
  WifiOff,
  Filter,
  RefreshCw,
  ArrowLeft,
} from "lucide-react";

// ── Status filter ──

type StatusFilter = "all" | "active" | "completed" | "failed";

const STATUS_FILTERS: { value: StatusFilter; label: string }[] = [
  { value: "all", label: "All" },
  { value: "active", label: "Active" },
  { value: "completed", label: "Completed" },
  { value: "failed", label: "Failed" },
];

// ── Helpers ──

function timeRemaining(deadlineHeight: number): string {
  const remaining = deadlineHeight;
  return `${remaining} blocks remaining`;
}

function formatTimestamp(ts: string): string {
  try {
    return new Date(ts).toLocaleString();
  } catch {
    return ts;
  }
}

function deriveStatusFromRental(rental: GpuRental): RentalStatus {
  return rental.active ? "active" : "completed";
}

// ── Component ──

export default function RentalsPage() {
  const user = useAuthStore((s) => s.user);
  const { events, isConnected, reconnect } = useRealtimeUpdates();

  const [rentals, setRentals] = useState<GpuRental[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [statusFilter, setStatusFilter] = useState<StatusFilter>("all");

  // Fetch rentals
  const loadRentals = useCallback(() => {
    if (!user?.ergoAddress) {
      setIsLoading(false);
      return;
    }
    setIsLoading(true);
    fetchMyRentals(user.ergoAddress)
      .then((data) => setRentals(data))
      .catch(() => setRentals([]))
      .finally(() => setIsLoading(false));
  }, [user?.ergoAddress]);

  useEffect(() => {
    loadRentals();
  }, [loadRentals]);

  // Apply SSE events to rentals in real-time
  useEffect(() => {
    if (events.length === 0) return;

    // Process only the latest batch of events
    const newEvents = events.slice(-10);

    setRentals((prev) => {
      let updated = [...prev];

      for (const event of newEvents) {
        if (event.type === "provider_heartbeat") continue;

        const idx = updated.findIndex(
          (r) =>
            r.rental_box_id === event.rentalId ||
            r.rental_tx_id === event.rentalId,
        );

        if (idx >= 0) {
          const rental = { ...updated[idx] };
          switch (event.type) {
            case "rental_active":
              rental.active = true;
              break;
            case "rental_completed":
              rental.active = false;
              break;
            case "rental_failed":
              rental.active = false;
              break;
          }
          updated[idx] = rental;
        }
      }

      return updated;
    });
  }, [events.length]); // eslint-disable-line react-hooks/exhaustive-deps

  // Build status lookup from events for rentals not yet in our list
  const eventStatusMap = useMemo(() => {
    const map = new Map<string, RentalStatus>();
    for (const event of events) {
      if (event.type === "provider_heartbeat") continue;
      const status = deriveStatusFromEvent(event);
      if (status) {
        map.set(event.rentalId, status);
      }
    }
    return map;
  }, [events]);

  // Filter rentals by status
  const filteredRentals = useMemo(() => {
    return rentals.filter((rental) => {
      if (statusFilter === "all") return true;

      // Check SSE events for latest status first
      const eventStatus = eventStatusMap.get(
        rental.rental_box_id || rental.rental_tx_id,
      );

      let status: RentalStatus;
      if (eventStatus) {
        status = eventStatus;
      } else {
        status = deriveStatusFromRental(rental);
      }

      return status === statusFilter;
    });
  }, [rentals, statusFilter, eventStatusMap]);

  // Counts for filters
  const counts = useMemo(() => {
    const c: Record<StatusFilter | RentalStatus, number> = {
      all: 0, active: 0, completed: 0, failed: 0, pending: 0,
    };
    for (const rental of rentals) {
      const eventId = rental.rental_box_id || rental.rental_tx_id;
      const eventStatus = eventStatusMap.get(eventId);
      const status = eventStatus ?? deriveStatusFromRental(rental);
      c[status]++;
      c.all++;
    }
    return c;
  }, [rentals, eventStatusMap]);

  // ── Render ──

  if (!user?.ergoAddress) {
    return (
      <div className="max-w-4xl mx-auto px-4 py-8">
        <div className="rounded-xl border border-dashed border-surface-200 bg-surface-0 p-12 text-center">
          <Cpu className="w-10 h-10 text-surface-800/20 mx-auto mb-3" />
          <h2 className="text-lg font-semibold text-surface-900 mb-1">
            Wallet Required
          </h2>
          <p className="text-sm text-surface-800/50">
            Connect your Ergo wallet to view your rental history.
          </p>
        </div>
      </div>
    );
  }

  return (
    <div className="max-w-4xl mx-auto px-4 py-8">
      {/* Header */}
      <div className="mb-6">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-3">
            <button
              onClick={() => (window.location.href = "/gpu")}
              className="rounded-lg p-1.5 text-surface-800/40 hover:text-surface-800/70 hover:bg-surface-100 transition-colors"
            >
              <ArrowLeft className="w-4 h-4" />
            </button>
            <div>
              <h1 className="text-2xl font-bold text-surface-900">
                My Rentals
              </h1>
              <p className="text-sm text-surface-800/50">
                Track your GPU rentals in real-time
              </p>
            </div>
          </div>

          {/* Live indicator + refresh */}
          <div className="flex items-center gap-3">
            <span
              className={cn(
                "flex items-center gap-1.5 rounded-full px-2.5 py-1 text-xs font-medium",
                isConnected
                  ? "bg-emerald-50 text-emerald-700 border border-emerald-200"
                  : "bg-surface-50 text-surface-800/40 border border-surface-200",
              )}
            >
              {isConnected ? (
                <>
                  <Wifi className="w-3 h-3" />
                  Live
                </>
              ) : (
                <>
                  <WifiOff className="w-3 h-3" />
                  Offline
                </>
              )}
            </span>
            {!isConnected && (
              <button
                onClick={reconnect}
                className="flex items-center gap-1 rounded-lg bg-surface-100 px-2.5 py-1 text-xs font-medium text-surface-800/60 hover:bg-surface-200 transition-colors"
              >
                <RefreshCw className="w-3 h-3" />
                Reconnect
              </button>
            )}
            <button
              onClick={loadRentals}
              className="flex items-center gap-1 rounded-lg bg-surface-100 px-2.5 py-1 text-xs font-medium text-surface-800/60 hover:bg-surface-200 transition-colors"
            >
              <RefreshCw className="w-3 h-3" />
              Refresh
            </button>
          </div>
        </div>
      </div>

      {/* Filters */}
      <div className="flex items-center gap-1 mb-6 border-b border-surface-200">
        <Filter className="w-3.5 h-3.5 text-surface-800/30 mr-1" />
        {STATUS_FILTERS.map((f) => (
          <button
            key={f.value}
            onClick={() => setStatusFilter(f.value)}
            className={cn(
              "px-3 py-2 text-sm font-medium border-b-2 transition-colors",
              statusFilter === f.value
                ? "border-brand-600 text-brand-600"
                : "border-transparent text-surface-800/50 hover:text-surface-800/70",
            )}
          >
            {f.label}
            <span className="ml-1.5 rounded-full bg-surface-100 text-surface-800/40 px-1.5 py-0.5 text-[10px]">
              {counts[f.value]}
            </span>
          </button>
        ))}
      </div>

      {/* Rental list */}
      {isLoading ? (
        <div className="space-y-3">
          {Array.from({ length: 3 }).map((_, i) => (
            <div
              key={i}
              className="rounded-xl border border-surface-200 bg-surface-0 p-4 animate-pulse"
            >
              <div className="flex items-center gap-3">
                <div className="h-4 w-32 rounded bg-surface-200" />
                <div className="h-5 w-16 rounded-full bg-surface-200" />
              </div>
              <div className="mt-2 flex gap-3">
                <div className="h-3 w-20 rounded bg-surface-100" />
                <div className="h-3 w-16 rounded bg-surface-100" />
              </div>
            </div>
          ))}
        </div>
      ) : filteredRentals.length === 0 ? (
        <div className="rounded-xl border border-dashed border-surface-200 bg-surface-0 p-12 text-center">
          <Cpu className="w-10 h-10 text-surface-800/20 mx-auto mb-3" />
          <p className="text-sm text-surface-800/40">
            {statusFilter === "all"
              ? "No rentals yet"
              : `No ${statusFilter} rentals found`}
          </p>
          <p className="text-xs text-surface-800/30 mt-1">
            {statusFilter === "all"
              ? "Rent a GPU from the marketplace to get started."
              : "Try a different filter or check back later."}
          </p>
        </div>
      ) : (
        <div className="space-y-3">
          {filteredRentals.map((rental) => {
            const cost = nanoergToErg(rental.total_cost_nanoerg);
            const eventId = rental.rental_box_id || rental.rental_tx_id;
            const initialStatus = deriveStatusFromRental(rental);

            return (
              <div
                key={rental.rental_box_id}
                className={cn(
                  "rounded-xl border p-4 transition-colors",
                  rental.active
                    ? "border-brand-200 bg-brand-50/30"
                    : "border-surface-200 bg-surface-0/50 opacity-70",
                )}
              >
                <div className="flex flex-col sm:flex-row sm:items-center gap-3">
                  {/* GPU info */}
                  <div className="flex-1 min-w-0">
                    <div className="flex items-center gap-2">
                      <Cpu className="w-4 h-4 text-brand-500 shrink-0" />
                      <span className="font-semibold text-surface-900 truncate">
                        {rental.gpu_type}
                      </span>
                      {/* Real-time status badge */}
                      <RentalStatusBadge
                        rentalId={eventId}
                        initialStatus={initialStatus}
                      />
                    </div>
                    <div className="flex flex-wrap items-center gap-3 mt-1 text-xs text-surface-800/50">
                      <span className="flex items-center gap-1">
                        <MapPin className="w-3 h-3" />
                        {rental.region}
                      </span>
                      <span className="flex items-center gap-1">
                        <Clock className="w-3 h-3" />
                        {rental.hours_rented}h rented
                      </span>
                      <span className="font-mono">
                        {cost.toFixed(4)} ERG
                      </span>
                    </div>
                    <div className="mt-1 text-xs text-surface-800/40">
                      {timeRemaining(rental.deadline_height)}
                    </div>
                  </div>
                </div>

                {/* TX ID */}
                <div className="mt-2 pt-2 border-t border-surface-100">
                  <span className="text-xs text-surface-800/30 font-mono truncate block">
                    TX: {rental.rental_tx_id}
                  </span>
                </div>
              </div>
            );
          })}
        </div>
      )}

      {/* Footer */}
      <div className="mt-8 text-sm text-surface-800/40 text-center">
        All prices in ERG. Rentals are settled on the Ergo blockchain.
        <br />
        <span className="text-[11px] text-surface-800/25">
          Real-time updates powered by SSE
          {isConnected ? " (connected)" : " (disconnected)"}
        </span>
      </div>
    </div>
  );
}
