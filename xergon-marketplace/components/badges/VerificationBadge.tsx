"use client";

import { useState } from "react";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export type VerificationStatus = "verified" | "pending" | "unverified";

interface VerificationBadgeProps {
  status: VerificationStatus;
  size?: "sm" | "md" | "lg";
  /** Optional extra details for tooltip */
  details?: string;
  className?: string;
}

// ---------------------------------------------------------------------------
// Config per status
// ---------------------------------------------------------------------------

const STATUS_CONFIG: Record<
  VerificationStatus,
  {
    label: string;
    bgColor: string;
    textColor: string;
    icon: React.ReactNode;
  }
> = {
  verified: {
    label: "Verified",
    bgColor: "bg-emerald-50 dark:bg-emerald-950/30",
    textColor: "text-emerald-700 dark:text-emerald-400",
    icon: (
      <svg className="w-full h-full" viewBox="0 0 20 20" fill="currentColor">
        <path fillRule="evenodd" d="M6.267 3.455a3.066 3.066 0 001.745-.723 3.066 3.066 0 013.976 0 3.066 3.066 0 001.745.723 3.066 3.066 0 012.812 2.812c.051.643.304 1.254.723 1.745a3.066 3.066 0 010 3.976 3.066 3.066 0 00-.723 1.745 3.066 3.066 0 01-2.812 2.812 3.066 3.066 0 00-1.745.723 3.066 3.066 0 01-3.976 0 3.066 3.066 0 00-1.745-.723 3.066 3.066 0 01-2.812-2.812 3.066 3.066 0 00-.723-1.745 3.066 3.066 0 010-3.976 3.066 3.066 0 00.723-1.745 3.066 3.066 0 012.812-2.812zm7.44 5.252a1 1 0 00-1.414-1.414L9 10.586 7.707 9.293a1 1 0 00-1.414 1.414l2 2a1 1 0 001.414 0l4-4z" clipRule="evenodd" />
      </svg>
    ),
  },
  pending: {
    label: "Pending",
    bgColor: "bg-amber-50 dark:bg-amber-950/30",
    textColor: "text-amber-700 dark:text-amber-400",
    icon: (
      <svg className="w-full h-full" viewBox="0 0 20 20" fill="currentColor">
        <path fillRule="evenodd" d="M10 18a8 8 0 100-16 8 8 0 000 16zm1-12a1 1 0 10-2 0v4a1 1 0 00.293.707l2.828 2.829a1 1 0 101.415-1.415L11 9.586V6z" clipRule="evenodd" />
      </svg>
    ),
  },
  unverified: {
    label: "Unverified",
    bgColor: "bg-surface-100 dark:bg-surface-800",
    textColor: "text-surface-800/50 dark:text-surface-200/50",
    icon: (
      <svg className="w-full h-full" viewBox="0 0 20 20" fill="currentColor">
        <path fillRule="evenodd" d="M10 9a3 3 0 100-6 3 3 0 000 6zm-7 9a7 7 0 1114 0H3z" clipRule="evenodd" />
      </svg>
    ),
  },
};

const SIZE_CLASSES = {
  sm: "w-4 h-4",
  md: "w-5 h-5",
  lg: "w-6 h-6",
};

const LABEL_SIZE_CLASSES = {
  sm: "text-[10px]",
  md: "text-xs",
  lg: "text-sm",
};

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function VerificationBadge({
  status,
  size = "md",
  details,
  className = "",
}: VerificationBadgeProps) {
  const [showTooltip, setShowTooltip] = useState(false);
  const config = STATUS_CONFIG[status];

  return (
    <span
      className={`relative inline-flex items-center gap-1 ${className}`}
      onMouseEnter={() => setShowTooltip(true)}
      onMouseLeave={() => setShowTooltip(false)}
    >
      <span
        className={`inline-flex items-center justify-center rounded-full ${config.bgColor} ${config.textColor} ${SIZE_CLASSES[size]}`}
        aria-label={`${config.label} provider`}
      >
        {config.icon}
      </span>
      {size !== "sm" && (
        <span className={`font-medium ${config.textColor} ${LABEL_SIZE_CLASSES[size]}`}>
          {config.label}
        </span>
      )}

      {/* Tooltip */}
      {showTooltip && details && (
        <span
          className="absolute bottom-full left-1/2 -translate-x-1/2 mb-2 px-2 py-1 rounded-md
                     bg-surface-900 text-surface-0 text-[10px] whitespace-nowrap z-10
                     after:content-[''] after:absolute after:top-full after:left-1/2 after:-translate-x-1/2
                     after:border-4 after:border-transparent after:border-t-surface-900"
          role="tooltip"
        >
          {details}
        </span>
      )}
    </span>
  );
}
