"use client";

import { useState } from "react";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export type ProviderTier = "free" | "basic" | "pro" | "enterprise";

interface TierBadgeProps {
  tier: ProviderTier;
  size?: "sm" | "md" | "lg";
  /** Show tooltip with tier benefits */
  showBenefits?: boolean;
  className?: string;
}

// ---------------------------------------------------------------------------
// Config per tier
// ---------------------------------------------------------------------------

interface TierConfig {
  label: string;
  bgColor: string;
  textColor: string;
  borderColor: string;
  benefits: string[];
  icon: string;
}

const TIER_CONFIG: Record<ProviderTier, TierConfig> = {
  free: {
    label: "Free",
    bgColor: "bg-surface-100 dark:bg-surface-800",
    textColor: "text-surface-800/60 dark:text-surface-200/60",
    borderColor: "border-surface-200 dark:border-surface-700",
    benefits: [
      "Access to free-tier models",
      "Rate limited to 10 req/min",
      "Community support",
    ],
    icon: "🆓",
  },
  basic: {
    label: "Basic",
    bgColor: "bg-blue-50 dark:bg-blue-950/30",
    textColor: "text-blue-700 dark:text-blue-400",
    borderColor: "border-blue-200 dark:border-blue-800/40",
    benefits: [
      "Access to standard models",
      "100 req/min rate limit",
      "Email support",
      "Basic analytics",
    ],
    icon: "📘",
  },
  pro: {
    label: "Pro",
    bgColor: "bg-purple-50 dark:bg-purple-950/30",
    textColor: "text-purple-700 dark:text-purple-400",
    borderColor: "border-purple-200 dark:border-purple-800/40",
    benefits: [
      "Access to all models including premium",
      "1,000 req/min rate limit",
      "Priority support",
      "Advanced analytics & reports",
      "Custom model deployment",
    ],
    icon: "⭐",
  },
  enterprise: {
    label: "Enterprise",
    bgColor: "bg-amber-50 dark:bg-amber-950/30",
    textColor: "text-amber-700 dark:text-amber-400",
    borderColor: "border-amber-200 dark:border-amber-800/40",
    benefits: [
      "Unlimited access to all models",
      "Custom rate limits",
      "Dedicated account manager",
      "SLA guarantees",
      "Custom integrations",
      "On-premise deployment option",
    ],
    icon: "🏆",
  },
};

const SIZE_CLASSES = {
  sm: "px-1.5 py-0.5 text-[10px]",
  md: "px-2.5 py-1 text-xs",
  lg: "px-3.5 py-1.5 text-sm",
};

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function TierBadge({
  tier,
  size = "md",
  showBenefits = false,
  className = "",
}: TierBadgeProps) {
  const [showTooltip, setShowTooltip] = useState(false);
  const config = TIER_CONFIG[tier];

  return (
    <span
      className={`relative inline-flex items-center gap-1 ${className}`}
      onMouseEnter={() => setShowTooltip(true)}
      onMouseLeave={() => setShowTooltip(false)}
    >
      <span
        className={`inline-flex items-center gap-1 rounded-full border font-medium
                    ${config.bgColor} ${config.textColor} ${config.borderColor} ${SIZE_CLASSES[size]}`}
        aria-label={`${config.label} tier provider`}
      >
        <span aria-hidden="true">{config.icon}</span>
        {config.label}
      </span>

      {/* Benefits tooltip */}
      {showBenefits && showTooltip && (
        <span
          className="absolute bottom-full left-1/2 -translate-x-1/2 mb-2 w-56 rounded-lg
                     bg-surface-900 text-surface-0 p-3 z-10 shadow-lg
                     after:content-[''] after:absolute after:top-full after:left-1/2 after:-translate-x-1/2
                     after:border-4 after:border-transparent after:border-t-surface-900"
          role="tooltip"
        >
          <p className="text-xs font-semibold mb-1.5">{config.label} Tier Benefits</p>
          <ul className="space-y-1">
            {config.benefits.map((benefit) => (
              <li key={benefit} className="flex items-start gap-1.5 text-[10px] text-surface-0/70">
                <span className="text-emerald-400 mt-0.5 shrink-0">•</span>
                {benefit}
              </li>
            ))}
          </ul>
        </span>
      )}
    </span>
  );
}
