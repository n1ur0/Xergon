"use client";

import { useState } from "react";
import type { ServiceStatus } from "@/lib/api/health";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function statusColor(status: ServiceStatus["status"]): string {
  switch (status) {
    case "operational":
      return "bg-emerald-500";
    case "degraded":
      return "bg-amber-500";
    case "down":
      return "bg-red-500";
    case "unknown":
      return "bg-surface-300";
  }
}

function statusLabel(status: ServiceStatus["status"]): string {
  switch (status) {
    case "operational":
      return "Operational";
    case "degraded":
      return "Degraded";
    case "down":
      return "Down";
    case "unknown":
      return "Unknown";
  }
}

function statusTextColor(status: ServiceStatus["status"]): string {
  switch (status) {
    case "operational":
      return "text-emerald-600 dark:text-emerald-400";
    case "degraded":
      return "text-amber-600 dark:text-amber-400";
    case "down":
      return "text-red-600 dark:text-red-400";
    case "unknown":
      return "text-surface-800/40";
  }
}

function uptimeColor(uptime: number): string {
  if (uptime >= 99) return "bg-emerald-500";
  if (uptime >= 95) return "bg-amber-500";
  return "bg-red-500";
}

function timeAgo(dateStr: string): string {
  const diff = Date.now() - new Date(dateStr).getTime();
  const seconds = Math.floor(diff / 1000);
  if (seconds < 60) return "just now";
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes} min ago`;
  const hours = Math.floor(minutes / 60);
  return `${hours}h ago`;
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

interface ServiceCardProps {
  service: ServiceStatus;
}

export function ServiceCard({ service }: ServiceCardProps) {
  const [expanded, setExpanded] = useState(false);

  return (
    <button
      type="button"
      onClick={() => setExpanded((prev) => !prev)}
      aria-expanded={expanded}
      aria-label={`${service.name}: ${statusLabel(service.status)}`}
      className="rounded-xl border border-surface-200 bg-surface-0 p-5 text-left w-full transition-all hover:border-surface-300 hover:shadow-sm"
    >
      {/* Header: name + status badge */}
      <div className="flex items-center justify-between mb-3">
        <span className="text-sm font-semibold text-surface-900">
          {service.name}
        </span>
        <span
          className={`inline-flex items-center gap-1.5 text-xs font-medium ${statusTextColor(service.status)}`}
        >
          <span className={`h-2 w-2 rounded-full ${statusColor(service.status)}`} aria-hidden="true" />
          {statusLabel(service.status)}
        </span>
      </div>

      {/* Metrics */}
      <div className="space-y-2">
        {/* Latency */}
        <div className="flex items-center justify-between">
          <span className="text-xs text-surface-800/50">Latency</span>
          <span className="text-sm font-medium text-surface-900">
            {service.latencyMs !== null ? `${service.latencyMs}ms` : "--"}
          </span>
        </div>

        {/* Uptime */}
        <div className="flex items-center justify-between">
          <span className="text-xs text-surface-800/50">Uptime (24h)</span>
          <div className="flex items-center gap-2">
            <div
              className="h-1.5 w-16 rounded-full bg-surface-100 overflow-hidden"
              role="progressbar"
              aria-valuenow={service.uptime24h}
              aria-valuemin={0}
              aria-valuemax={100}
              aria-label={`Uptime (24h): ${service.uptime24h > 0 ? `${service.uptime24h}%` : "unavailable"}`}
            >
              <div
                className={`h-full rounded-full ${uptimeColor(service.uptime24h)} transition-all duration-500`}
                style={{ width: `${Math.min(100, service.uptime24h)}%` }}
              />
            </div>
            <span className="text-sm font-medium text-surface-900">
              {service.uptime24h > 0 ? `${service.uptime24h}%` : "--"}
            </span>
          </div>
        </div>

        {/* Incidents */}
        {service.incidents24h > 0 && (
          <div className="flex items-center justify-between">
            <span className="text-xs text-surface-800/50">Incidents (24h)</span>
            <span className="inline-flex items-center justify-center h-5 min-w-5 rounded-full bg-red-100 dark:bg-red-900/30 px-1.5 text-xs font-medium text-red-700 dark:text-red-400">
              {service.incidents24h}
            </span>
          </div>
        )}
      </div>

      {/* Expanded detail */}
      {expanded && (
        <div className="mt-3 pt-3 border-t border-surface-100 space-y-1.5">
          <div className="flex items-center justify-between">
            <span className="text-xs text-surface-800/50">Endpoint</span>
            <span className="text-xs font-mono text-surface-800/70">
              {service.url}
            </span>
          </div>
          <div className="flex items-center justify-between">
            <span className="text-xs text-surface-800/50">Last checked</span>
            <span className="text-xs text-surface-800/70">
              {timeAgo(service.lastCheck)}
            </span>
          </div>
        </div>
      )}

      {/* Expand hint */}
      <div className="mt-3 text-[10px] text-surface-800/30 text-right">
        {expanded ? "Click to collapse" : "Click for details"}
      </div>
    </button>
  );
}
