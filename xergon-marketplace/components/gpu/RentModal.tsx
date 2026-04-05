"use client";

import { useState, useEffect, useMemo } from "react";
import { cn } from "@/lib/utils";
import { nanoergToErg, parseGpuSpecs, type GpuListing } from "@/lib/api/gpu";
import { X, Cpu, Clock, MapPin, AlertCircle, Loader2 } from "lucide-react";

interface RentModalProps {
  listing: GpuListing | null;
  isOpen: boolean;
  onClose: () => void;
  onConfirm: (listingId: string, hours: number) => Promise<void>;
}

// ── Validation ──

interface FormErrors {
  hours?: string;
}

function validateHours(hours: number): FormErrors {
  const errors: FormErrors = {};
  if (!hours || hours < 1) {
    errors.hours = "Minimum rental is 1 hour";
  } else if (hours > 720) {
    errors.hours = "Maximum rental is 720 hours (30 days)";
  } else if (!Number.isInteger(hours) && hours < 1) {
    errors.hours = "Rental duration must be at least 1 hour";
  }
  return errors;
}

export function RentModal({ listing, isOpen, onClose, onConfirm }: RentModalProps) {
  const [hours, setHours] = useState(1);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [hoursInput, setHoursInput] = useState("1");

  // Reset form when modal opens/closes
  useEffect(() => {
    if (isOpen) {
      setHours(1);
      setHoursInput("1");
      setError(null);
      setIsLoading(false);
    }
  }, [isOpen]);

  if (!isOpen || !listing) return null;

  const specs = parseGpuSpecs(listing.gpu_specs_json);
  const pricePerHour = nanoergToErg(listing!.price_per_hour_nanoerg);
  const totalCost = pricePerHour * hours;
  const validationErrors = validateHours(hours);
  const isValid = Object.keys(validationErrors).length === 0;

  // Compute estimated end time
  const estimatedEndTime = useMemo(() => {
    const end = new Date(Date.now() + hours * 60 * 60 * 1000);
    return end.toLocaleString();
  }, [hours]);

  function handleClose() {
    if (isLoading) return; // Don't close while processing
    setError(null);
    setHours(1);
    setHoursInput("1");
    onClose();
  }

  function handleHoursChange(value: string) {
    setHoursInput(value);
    const num = Number(value);
    if (!isNaN(num) && num >= 0) {
      setHours(Math.min(720, Math.max(0, num)));
    }
  }

  function handleQuickPick(h: number) {
    setHours(h);
    setHoursInput(String(h));
  }

  async function handleConfirm() {
    if (!listing || !isValid || isLoading) return;
    setIsLoading(true);
    setError(null);
    try {
      await onConfirm(listing.listing_id, hours);
      handleClose();
    } catch (err) {
      // Don't show error if it was already handled by parent (e.g., toast shown)
      if (err instanceof Error && !err.message.includes("Failed to rent")) {
        setError(err.message);
      } else if (!(err instanceof Error)) {
        setError("Failed to rent GPU");
      }
      // If the error was handled by the parent toast system, just close
    } finally {
      setIsLoading(false);
    }
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      {/* Backdrop */}
      <div
        className="absolute inset-0 bg-black/50 backdrop-blur-sm"
        onClick={handleClose}
      />

      {/* Modal */}
      <div className="relative w-full max-w-md mx-4 rounded-xl border border-surface-200 bg-surface-0 shadow-xl animate-fade-in">
        {/* Close button */}
        {!isLoading && (
          <button
            onClick={handleClose}
            className="absolute top-3 right-3 rounded-lg p-1 text-surface-800/40 hover:text-surface-800/70 hover:bg-surface-100 transition-colors"
          >
            <X className="w-5 h-5" />
          </button>
        )}

        <div className="p-6">
          {/* Header */}
          <h2 className="text-lg font-bold text-surface-900 mb-1">Rent GPU</h2>
          <p className="text-sm text-surface-800/60">
            Confirm rental for the selected GPU instance.
          </p>

          {/* Listing summary */}
          <div className="mt-4 rounded-lg bg-surface-50 p-4 space-y-2">
            <div className="flex items-center gap-2">
              <Cpu className="w-4 h-4 text-brand-500" />
              <span className="font-semibold text-surface-900">{listing.gpu_type}</span>
            </div>
            {specs.vram_gb != null && (
              <div className="text-sm text-surface-800/60">
                {specs.vram_gb} GB VRAM
                {specs.cuda_cores != null && ` · ${specs.cuda_cores.toLocaleString()} CUDA cores`}
              </div>
            )}
            <div className="flex items-center gap-1 text-sm text-surface-800/60">
              <MapPin className="w-3.5 h-3.5" />
              {listing.region}
            </div>
          </div>

          {/* Hours input */}
          <div className="mt-4">
            <label className="block text-sm font-medium text-surface-800/70 mb-1.5">
              Rental Duration
            </label>
            <div className="flex items-center gap-2">
              <input
                type="number"
                min={1}
                max={720}
                step={1}
                value={hoursInput}
                onChange={(e) => handleHoursChange(e.target.value)}
                disabled={isLoading}
                className={cn(
                  "w-24 rounded-lg border bg-surface-50 px-3 py-2 text-sm text-surface-900 focus:outline-none focus:ring-2 focus:ring-brand-500/30 focus:border-brand-500 disabled:opacity-50 disabled:cursor-not-allowed",
                  validationErrors.hours
                    ? "border-danger-500"
                    : "border-surface-200",
                )}
              />
              <span className="text-sm text-surface-800/60 flex items-center gap-1">
                <Clock className="w-3.5 h-3.5" />
                hours
              </span>
            </div>

            {/* Validation error */}
            {validationErrors.hours && (
              <div className="mt-1.5 flex items-center gap-1 text-xs text-danger-600">
                <AlertCircle className="w-3 h-3 shrink-0" />
                {validationErrors.hours}
              </div>
            )}

            {/* Quick pick buttons */}
            <div className="flex gap-1.5 mt-2">
              {[1, 4, 8, 24, 72].map((h) => (
                <button
                  key={h}
                  onClick={() => handleQuickPick(h)}
                  disabled={isLoading}
                  className={cn(
                    "rounded-full px-2.5 py-0.5 text-xs font-medium transition-colors disabled:opacity-50 disabled:cursor-not-allowed",
                    hours === h
                      ? "bg-brand-600 text-white"
                      : "bg-surface-100 text-surface-800/60 hover:bg-surface-200",
                  )}
                >
                  {h}h
                </button>
              ))}
            </div>
          </div>

          {/* Cost summary */}
          <div className="mt-4 rounded-lg bg-brand-50 border border-brand-200 p-4">
            <div className="flex justify-between text-sm">
              <span className="text-surface-800/60">Rate</span>
              <span className="text-surface-900">{pricePerHour.toFixed(4)} ERG/hr</span>
            </div>
            <div className="flex justify-between text-sm mt-1">
              <span className="text-surface-800/60">Duration</span>
              <span className="text-surface-900">{hours} hour{hours !== 1 ? "s" : ""}</span>
            </div>
            {hours > 0 && (
              <div className="flex justify-between text-sm mt-1">
                <span className="text-surface-800/60">Est. End Time</span>
                <span className="text-surface-900 text-xs">{estimatedEndTime}</span>
              </div>
            )}
            <div className="flex justify-between font-semibold text-surface-900 mt-2 pt-2 border-t border-brand-200">
              <span>Total Cost</span>
              <span className={cn(
                totalCost > 0 ? "text-brand-700" : "text-surface-900",
              )}>
                {totalCost.toFixed(4)} ERG
              </span>
            </div>
          </div>

          {/* Error */}
          {error && (
            <div className="mt-3 rounded-lg bg-danger-500/10 border border-danger-500/20 px-3 py-2 text-sm text-danger-600">
              {error}
            </div>
          )}

          {/* Actions */}
          <div className="flex gap-3 mt-6">
            <button
              onClick={handleClose}
              disabled={isLoading}
              className="flex-1 rounded-lg border border-surface-200 px-4 py-2.5 text-sm font-medium text-surface-800/70 hover:bg-surface-50 transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
            >
              Cancel
            </button>
            <button
              onClick={handleConfirm}
              disabled={isLoading || !isValid}
              className={cn(
                "flex-1 rounded-lg px-4 py-2.5 text-sm font-medium transition-colors inline-flex items-center justify-center gap-2",
                isLoading || !isValid
                  ? "bg-brand-400 text-white cursor-not-allowed"
                  : "bg-brand-600 text-white hover:bg-brand-700",
              )}
            >
              {isLoading ? (
                <>
                  <Loader2 className="w-4 h-4 animate-spin" />
                  Processing...
                </>
              ) : (
                "Confirm Rental"
              )}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
