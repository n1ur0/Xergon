"use client";

import { cn } from "@/lib/utils";
import { parseGpuSpecs, nanoergToErg, type GpuListing, type GpuReputation } from "@/lib/api/gpu";
import { Cpu, MapPin, Star } from "lucide-react";

interface GpuCardProps {
  listing: GpuListing;
  reputation?: GpuReputation | null;
  onRent: (listing: GpuListing) => void;
}

export function GpuCard({ listing, reputation, onRent }: GpuCardProps) {
  const specs = parseGpuSpecs(listing.gpu_specs_json);
  const pricePerHour = nanoergToErg(listing.price_per_hour_nanoerg);

  return (
    <div
      className={cn(
        "relative rounded-xl border p-4 sm:p-5 transition-all hover:shadow-md",
        listing.available
          ? "border-surface-200 bg-surface-0 hover:border-brand-300"
          : "border-surface-200/60 bg-surface-0/50 opacity-60",
      )}
    >
      {/* Unavailable badge */}
      {!listing.available && (
        <div className="absolute -top-2 -right-2 rounded-full bg-surface-800 px-2.5 py-0.5 text-xs font-semibold text-white shadow-sm">
          Rented
        </div>
      )}

      {/* Header: GPU type + region - stacked on mobile */}
      <div className="mb-3">
        <div className="flex flex-col sm:flex-row sm:items-start sm:justify-between gap-1.5">
          <div className="min-w-0">
            <h2 className="font-semibold text-surface-900 leading-tight flex items-center gap-1.5 truncate">
              <Cpu className="w-4 h-4 text-brand-500 flex-shrink-0" />
              <span className="truncate">{listing.gpu_type}</span>
            </h2>
            <p className="text-xs text-surface-800/40 mt-0.5 font-mono truncate">
              {listing.provider_pk}
            </p>
          </div>
          <span className="inline-flex items-center gap-1 rounded-full bg-surface-100 px-2 py-0.5 text-xs text-surface-800/60 self-start flex-shrink-0">
            <MapPin className="w-3 h-3" />
            {listing.region}
          </span>
        </div>
      </div>

      {/* Specs grid - 2 cols on mobile, same on desktop */}
      <div className="grid grid-cols-2 gap-2 mb-4">
        {specs.vram_gb != null && (
          <div className="rounded-lg bg-surface-50 px-2.5 py-2">
            <div className="text-xs text-surface-800/40">VRAM</div>
            <div className="text-sm font-medium text-surface-900">{specs.vram_gb} GB</div>
          </div>
        )}
        {specs.cuda_cores != null && (
          <div className="rounded-lg bg-surface-50 px-2.5 py-2">
            <div className="text-xs text-surface-800/40">CUDA Cores</div>
            <div className="text-sm font-medium text-surface-900">{specs.cuda_cores.toLocaleString()}</div>
          </div>
        )}
        {specs.memory_type && (
          <div className="rounded-lg bg-surface-50 px-2.5 py-2">
            <div className="text-xs text-surface-800/40">Memory</div>
            <div className="text-sm font-medium text-surface-900">{specs.memory_type}</div>
          </div>
        )}
        {specs.memory_bandwidth && (
          <div className="rounded-lg bg-surface-50 px-2.5 py-2">
            <div className="text-xs text-surface-800/40">Bandwidth</div>
            <div className="text-sm font-medium text-surface-900">{specs.memory_bandwidth}</div>
          </div>
        )}
      </div>

      {/* Provider rating */}
      {reputation && reputation.total_ratings > 0 && (
        <div className="flex items-center gap-1 mb-3">
          <div className="flex">
            {Array.from({ length: 5 }).map((_, i) => (
              <Star
                key={i}
                className={cn(
                  "w-3.5 h-3.5",
                  i < Math.round(reputation.average_rating)
                    ? "fill-amber-400 text-amber-400"
                    : "text-surface-200",
                )}
              />
            ))}
          </div>
          <span className="text-xs text-surface-800/50">
            {reputation.average_rating.toFixed(1)} ({reputation.total_ratings})
          </span>
        </div>
      )}

      {/* Footer: price + rent - full width button on mobile */}
      <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-3 pt-3 border-t border-surface-100">
        <div className="flex items-baseline gap-1">
          <span className="text-lg font-bold text-surface-900">
            {pricePerHour.toFixed(4)}
          </span>
          <span className="text-sm text-surface-800/50">ERG/hr</span>
        </div>
        <button
          onClick={() => onRent(listing)}
          disabled={!listing.available}
          className={cn(
            "w-full sm:w-auto rounded-lg px-4 py-3 sm:px-3 sm:py-1.5 text-sm font-medium transition-colors min-h-[44px] sm:min-h-0",
            listing.available
              ? "bg-brand-600 text-white hover:bg-brand-700 active:bg-brand-800"
              : "bg-surface-100 text-surface-800/30 cursor-not-allowed",
          )}
        >
          Rent
        </button>
      </div>
    </div>
  );
}
