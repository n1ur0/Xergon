"use client";

import { cn } from "@/lib/utils";
import { nanoergToErg, type GpuRental } from "@/lib/api/gpu";
import { Clock, MapPin, Cpu, RotateCcw, Plus } from "lucide-react";

interface MyRentalsProps {
  rentals: GpuRental[];
  onExtend?: (rental: GpuRental) => void;
  onRefund?: (rental: GpuRental) => void;
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

export function MyRentals({ rentals, onExtend, onRefund }: MyRentalsProps) {
  if (rentals.length === 0) {
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
      {rentals.map((rental) => {
        const cost = nanoergToErg(rental.total_cost_nanoerg);
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
                  <span
                    className={cn(
                      "rounded-full px-2 py-0.5 text-xs font-medium",
                      rental.active
                        ? "bg-emerald-100 text-emerald-700"
                        : "bg-surface-100 text-surface-800/40",
                    )}
                  >
                    {rental.active ? "Active" : "Expired"}
                  </span>
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
