"use client";

import { useState, useCallback } from "react";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface StarRatingProps {
  /** Current rating value (0-5) */
  value: number;
  /** Called when user clicks a star. If undefined, component is display-only. */
  onChange?: (rating: number) => void;
  /** Show as read-only (no hover/click) */
  readonly?: boolean;
  /** Size variant */
  size?: "sm" | "md" | "lg";
  /** Additional CSS class */
  className?: string;
}

// ---------------------------------------------------------------------------
// Size mapping
// ---------------------------------------------------------------------------

const SIZE_CLASSES = {
  sm: "w-4 h-4",
  md: "w-5 h-5",
  lg: "w-6 h-6",
} as const;

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function StarRating({
  value,
  onChange,
  readonly = false,
  size = "md",
  className = "",
}: StarRatingProps) {
  const [hoverValue, setHoverValue] = useState(0);
  const isInteractive = !readonly && !!onChange;

  const displayValue = hoverValue > 0 ? hoverValue : value;

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent, star: number) => {
      if (!isInteractive) return;
      if (e.key === "Enter" || e.key === " ") {
        e.preventDefault();
        onChange!(star);
      }
      if (e.key === "ArrowRight" || e.key === "ArrowUp") {
        e.preventDefault();
        onChange!(Math.min(5, value + 1));
      }
      if (e.key === "ArrowLeft" || e.key === "ArrowDown") {
        e.preventDefault();
        onChange!(Math.max(1, value - 1));
      }
    },
    [isInteractive, onChange, value]
  );

  return (
    <div
      className={`inline-flex items-center gap-0.5 ${className}`}
      role={isInteractive ? "radiogroup" : "img"}
      aria-label={isInteractive ? "Rating" : `${value} out of 5 stars`}
    >
      {[1, 2, 3, 4, 5].map((star) => {
        const isFilled = star <= displayValue;
        const isHalf = !isFilled && star - 0.5 <= displayValue;

        return (
          <button
            key={star}
            type="button"
            disabled={readonly}
            onClick={() => isInteractive && onChange!(star)}
            onMouseEnter={() => isInteractive && setHoverValue(star)}
            onMouseLeave={() => isInteractive && setHoverValue(0)}
            onKeyDown={(e) => handleKeyDown(e, star)}
            role={isInteractive ? "radio" : undefined}
            aria-checked={isInteractive ? star === value : undefined}
            aria-label={isInteractive ? `${star} star${star > 1 ? "s" : ""}` : undefined}
            className={`
              transition-transform duration-100
              ${isInteractive ? "cursor-pointer hover:scale-110 focus:outline-none focus-visible:ring-2 focus-visible:ring-brand-500 rounded-sm" : "cursor-default"}
            `}
          >
            <svg
              className={`${SIZE_CLASSES[size]} transition-colors ${
                isFilled
                  ? "text-amber-400"
                  : isHalf
                  ? "text-amber-400"
                  : "text-surface-300 dark:text-surface-600"
              }`}
              viewBox="0 0 24 24"
              fill={isFilled ? "currentColor" : isHalf ? "currentColor" : "none"}
              stroke="currentColor"
              strokeWidth="1.5"
            >
              {isHalf ? (
                <>
                  <defs>
                    <linearGradient id={`half-star-${star}`}>
                      <stop offset="50%" stopColor="currentColor" />
                      <stop offset="50%" stopColor="transparent" />
                    </linearGradient>
                  </defs>
                  <path
                    d="M12 2l3.09 6.26L22 9.27l-5 4.87 1.18 6.88L12 17.77l-6.18 3.25L7 14.14 2 9.27l6.91-1.01L12 2z"
                    fill={`url(#half-star-${star})`}
                    stroke="currentColor"
                    strokeWidth="1.5"
                  />
                </>
              ) : (
                <path d="M12 2l3.09 6.26L22 9.27l-5 4.87 1.18 6.88L12 17.77l-6.18 3.25L7 14.14 2 9.27l6.91-1.01L12 2z" />
              )}
            </svg>
          </button>
        );
      })}
    </div>
  );
}
