"use client";

import { cn } from "@/lib/utils";
import { useRealtimeUpdates, type RentalEvent, type RentalEventType } from "@/hooks/use-realtime-updates";
import { Activity, CheckCircle2, AlertTriangle, XCircle, Loader2, Wifi, WifiOff } from "lucide-react";

// ── Status types ──

export type RentalStatus = "pending" | "active" | "completed" | "failed";

export interface RentalStatusBadgeProps {
  rentalId: string;
  /** Override the initial status before SSE events arrive */
  initialStatus?: RentalStatus;
  className?: string;
  showConnectionStatus?: boolean;
}

// ── Status helpers ──

const STATUS_CONFIG: Record<
  RentalStatus,
  { label: string; bg: string; text: string; icon: typeof Activity }
> = {
  pending: {
    label: "Pending",
    bg: "bg-amber-100",
    text: "text-amber-700",
    icon: Loader2,
  },
  active: {
    label: "Active",
    bg: "bg-emerald-100",
    text: "text-emerald-700",
    icon: Activity,
  },
  completed: {
    label: "Completed",
    bg: "bg-blue-100",
    text: "text-blue-700",
    icon: CheckCircle2,
  },
  failed: {
    label: "Failed",
    bg: "bg-red-100",
    text: "text-red-700",
    icon: XCircle,
  },
};

function deriveStatusFromEvent(event: RentalEvent): RentalStatus | null {
  switch (event.type) {
    case "rental_created":
      return "pending";
    case "rental_active":
      return "active";
    case "rental_completed":
      return "completed";
    case "rental_failed":
      return "failed";
    case "provider_heartbeat":
      return null;
    default:
      return null;
  }
}

// ── Component ──

export function RentalStatusBadge({
  rentalId,
  initialStatus,
  className,
  showConnectionStatus = false,
}: RentalStatusBadgeProps) {
  const { isConnected, events } = useRealtimeUpdates();

  // Find the most recent event for this rental
  const relevantEvent = [...events]
    .reverse()
    .find((e) => {
      if (e.type === "provider_heartbeat") return false;
      return e.rentalId === rentalId;
    });

  const status = relevantEvent
    ? deriveStatusFromEvent(relevantEvent)
    : initialStatus ?? null;

  if (!status) {
    return showConnectionStatus ? (
      <ConnectionIndicator isConnected={isConnected} className={className} />
    ) : null;
  }

  const config = STATUS_CONFIG[status];
  const Icon = config.icon;

  return (
    <div className={cn("flex items-center gap-1.5", className)}>
      <span
        className={cn(
          "inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-xs font-medium",
          config.bg,
          config.text,
        )}
      >
        <Icon
          className={cn(
            "w-3 h-3",
            status === "active" && "animate-pulse",
            status === "pending" && "animate-spin",
          )}
        />
        {config.label}
      </span>
      {showConnectionStatus && (
        <ConnectionIndicator isConnected={isConnected} />
      )}
    </div>
  );
}

// ── Connection indicator ──

function ConnectionIndicator({
  isConnected,
  className,
}: {
  isConnected: boolean;
  className?: string;
}) {
  const Icon = isConnected ? Wifi : WifiOff;

  return (
    <span
      className={cn(
        "inline-flex items-center gap-0.5 text-[10px] font-medium",
        isConnected ? "text-emerald-600" : "text-surface-800/30",
        className,
      )}
      title={isConnected ? "Live updates connected" : "Live updates disconnected"}
    >
      <Icon className="w-3 h-3" />
      {isConnected ? "Live" : "Offline"}
    </span>
  );
}

// ── Exported helpers for use in other components ──

export { deriveStatusFromEvent, STATUS_CONFIG };
