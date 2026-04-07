"use client";

import { useEffect, useMemo } from "react";
import { cn } from "@/lib/utils";
import { nanoergToErg, type GpuRental } from "@/lib/api/gpu";
import { useRealtimeUpdates, type RentalEvent } from "@/hooks/use-realtime-updates";
import {
  RentalStatusBadge,
  deriveStatusFromEvent,
  type RentalStatus,
} from "@/components/gpu/RentalStatusBadge";
import { Clock, MapPin, Cpu, RotateCcw, Plus, Wifi, WifiOff, RefreshCw } from "lucide-react";

interface MyRentalsProps {
  rentals: GpuRental[];
  onExtend?: (rental: GpuRental) => void;
  onRefund?: (rental: GpuRental) => void;
  onRefresh?: () => void;
}

function timeRemaining(deadlineHeight: number, currentHeight?: number): string {
  // If no current height, show as "active"
  if (currentHeight == null) {
    const remaining = deadlineHeight;
    return `${remaining} blocks remaining`;
  }
  const remaining = deadlineHeight - currentHeight;
  if (remaining <= 0) return "Expired";
  // Rough estimate: ~2 min per block
  const hours = Math.round((remaining * 2) / 60);
  if (hours < 1) return `${remaining} blocks remaining`;
  return `~${hours} hour${hours !== 1 ? "s" : ""} remaining`;
}

export function MyRentals({ rentals, onExtend, onRefund, onRefresh }: MyRentalsProps) {
  const { isConnected, events, reconnect } = useRealtimeUpdates();

  // Apply SSE events to update rental statuses in real-time
  const updatedRentals = useMemo(() => {
    if (events.length === 0) return rentals;

    // Build a map of rental ID -> latest status from events
    const statusMap = new Map<string, RentalStatus>();
    for (const event of events) {
      if (event.type === "provider_heartbeat") continue;
      const status = deriveStatusFromEvent(event);
      if (status) {
        statusMap.set(event.rentalId, status);
      }
    }

    return rentals.map((rental) => {
      const eventId = rental.rental_box_id || rental.rental_tx_id;
      const eventStatus = statusMap.get(eventId);

      if (!eventStatus) return rental;

      const updated = { ...rental };
      switch (eventStatus) {
        case "active":
          updated.active = true;
          break;
        case "completed":
        case "failed":
          updated.active = false;
          break;
        case "pending":
          // pending doesn't change the rental state
          break;
      }
      return updated;
    });
  }, [rentals, events]);

  if (updatedRentals.length === 0) {
    return (
      <div className="rounded-xl border border-dashed border-surface-200 bg-surface-0 p-8 text-center">
        <Cpu className="w-8 h-8 text-surface-800/20 mx-auto mb-2" />
        <p className="text-sm text-surface-800/40">No active rentals</p>
        <p className="text-xs text-surface-800/30 mt-1">
          Rent a GPU from the listings above to get started.
        </p>
      </div>
    );
  }

  return (
    <div className="space-y-3">
      {/* Live status header */}
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-1.5">
          <span
            className={cn(
              "inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-[10px] font-medium",
              isConnected
                ? "bg-emerald-50 text-emerald-700 border border-emerald-200"
                : "bg-surface-50 text-surface-800/30 border border-surface-200",
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
          <span className="text-[10px] text-surface-800/30">
            Real-time status updates
          </span>
        </div>
        <div className="flex items-center gap-2">
          {!isConnected && (
            <button
              onClick={reconnect}
              className="flex items-center gap-1 rounded-lg bg-surface-100 px-2 py-0.5 text-[10px] font-medium text-surface-800/50 hover:bg-surface-200 transition-colors"
            >
              <RefreshCw className="w-3 h-3" />
              Reconnect
            </button>
          )}
          {onRefresh && (
            <button
              onClick={onRefresh}
              className="flex items-center gap-1 rounded-lg bg-surface-100 px-2 py-0.5 text-[10px] font-medium text-surface-800/50 hover:bg-surface-200 transition-colors"
            >
              <RefreshCw className="w-3 h-3" />
              Refresh
            </button>
          )}
        </div>
      </div>

      {updatedRentals.map((rental) => {
        const cost = nanoergToErg(rental.total_cost_nanoerg);
        const eventId = rental.rental_box_id || rental.rental_tx_id;
        const initialStatus = rental.active ? "active" : "completed";

        return (
          <div
            key={rental.rental_box_id}
            className={cn(
              "rounded-xl border p-4 transition-colors",
              rental.active
                ? "border-brand-200 bg-brand-50/30"
                : "border-surface-200 bg-surface-0/50 opacity-60",
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
                  {/* Real-time status badge replaces the static one */}
                  <RentalStatusBadge
                    rentalId={eventId}
                    initialStatus={initialStatus}
                  />
                </div>
                <div className="flex items-center gap-3 mt-1 text-xs text-surface-800/50">
                  <span className="flex items-center gap-1">
                    <MapPin className="w-3 h-3" />
                    {rental.region}
                  </span>
                  <span className="flex items-center gap-1">
                    <Clock className="w-3 h-3" />
                    {rental.hours_rented}h rented
                  </span>
                  <span className="font-mono">{cost.toFixed(4)} ERG</span>
                </div>
                <div className="mt-1 text-xs text-surface-800/40">
                  {timeRemaining(rental.deadline_height)}
                </div>
              </div>

              {/* Actions */}
              {rental.active && (
                <div className="flex items-center gap-2 shrink-0">
                  {onExtend && (
                    <button
                      onClick={() => onExtend(rental)}
                      className="flex items-center gap-1 rounded-lg bg-surface-100 px-3 py-1.5 text-xs font-medium text-surface-800/70 hover:bg-surface-200 transition-colors"
                    >
                      <Plus className="w-3.5 h-3.5" />
                      Extend
                    </button>
                  )}
                  {onRefund && (
                    <button
                      onClick={() => onRefund(rental)}
                      className="flex items-center gap-1 rounded-lg border border-danger-500/20 px-3 py-1.5 text-xs font-medium text-danger-600 hover:bg-danger-500/5 transition-colors"
                    >
                      <RotateCcw className="w-3.5 h-3.5" />
                      Refund
                    </button>
                  )}
                </div>
              )}
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
  );
}
