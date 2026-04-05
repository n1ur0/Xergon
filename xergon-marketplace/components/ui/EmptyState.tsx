"use client";

import { cn } from "@/lib/utils";
import {
  Cpu,
  SearchX,
  Inbox,
  Monitor,
  Server,
  type LucideIcon,
} from "lucide-react";
import React from "react";

// ── Preset empty state types ──

type EmptyStateType =
  | "no-providers"
  | "no-gpus"
  | "no-search-results"
  | "no-rentals"
  | "no-models"
  | "generic";

interface EmptyStateConfig {
  icon: LucideIcon;
  title: string;
  description: string;
  action?: string;
}

const EMPTY_STATE_CONFIGS: Record<EmptyStateType, EmptyStateConfig> = {
  "no-providers": {
    icon: Server,
    title: "No Providers Available",
    description:
      "There are no compute providers connected to the network yet. Providers will appear here once they start processing requests.",
  },
  "no-gpus": {
    icon: Cpu,
    title: "No GPU Listings",
    description:
      "No GPUs are currently available for rent. Check back soon or try adjusting your filters.",
    action: "Clear Filters",
  },
  "no-search-results": {
    icon: SearchX,
    title: "No Results Found",
    description:
      "No listings match your current search. Try adjusting your filters or search terms.",
    action: "Clear Filters",
  },
  "no-rentals": {
    icon: Inbox,
    title: "No Rental History",
    description:
      "You haven't rented any GPUs yet. Browse available listings to get started.",
    action: "Browse GPUs",
  },
  "no-models": {
    icon: Monitor,
    title: "No Models Available",
    description:
      "No models are currently available from connected providers. Models will appear once providers register them.",
  },
  generic: {
    icon: Inbox,
    title: "Nothing Here Yet",
    description: "No data to display. Check back later.",
  },
};

// ── Props ──

interface EmptyStateProps {
  type?: EmptyStateType;
  title?: string;
  description?: string;
  icon?: LucideIcon;
  action?: {
    label: string;
    onClick: () => void;
  };
  className?: string;
  children?: React.ReactNode;
}

/**
 * Reusable empty state component with SVG illustrations.
 * Provides preset configurations for common empty states.
 */
export function EmptyState({
  type = "generic",
  title,
  description,
  icon: customIcon,
  action,
  className,
  children,
}: EmptyStateProps) {
  const config = EMPTY_STATE_CONFIGS[type];
  const IconComponent = customIcon ?? config.icon;

  const displayTitle = title ?? config.title;
  const displayDescription = description ?? config.description;

  return (
    <div
      className={cn(
        "flex flex-col items-center justify-center py-16 px-4 text-center",
        className,
      )}
    >
      {/* Illustration circle with icon */}
      <div className="mb-4 flex h-16 w-16 items-center justify-center rounded-full bg-surface-100">
        <IconComponent className="w-7 h-7 text-surface-800/30" />
      </div>

      {/* Title */}
      <h3 className="text-lg font-semibold text-surface-900 mb-1">
        {displayTitle}
      </h3>

      {/* Description */}
      <p className="text-sm text-surface-800/50 max-w-md mb-4">
        {displayDescription}
      </p>

      {/* Action button (from preset or explicit prop) */}
      {(action || config.action) && (
        <button
          onClick={action?.onClick ?? (() => {})}
          className="rounded-lg bg-brand-600 px-4 py-2 text-sm font-medium text-white transition-colors hover:bg-brand-700"
        >
          {action?.label ?? config.action}
        </button>
      )}

      {/* Extra content slot */}
      {children}
    </div>
  );
}
