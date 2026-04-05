/**
 * Small dot indicator showing a provider's real-time status.
 *
 * - Green pulsing dot for online
 * - Gray dot for offline
 * - Yellow dot for unknown
 */

interface ProviderStatusIndicatorProps {
  status: "online" | "offline" | "unknown";
  size?: "sm" | "md";
}

const sizeClasses = {
  sm: "h-2 w-2",
  md: "h-2.5 w-2.5",
} as const;

export function ProviderStatusIndicator({
  status,
  size = "sm",
}: ProviderStatusIndicatorProps) {
  const sizeClass = sizeClasses[size];

  if (status === "online") {
    return (
      <span className="relative inline-flex">
        <span
          className={`${sizeClass} rounded-full bg-green-500`}
          aria-label="Online"
        />
        <span
          className={`${sizeClass} absolute inline-flex animate-ping rounded-full bg-green-400 opacity-75`}
          aria-hidden="true"
        />
      </span>
    );
  }

  if (status === "offline") {
    return (
      <span
        className={`${sizeClass} inline-block rounded-full bg-surface-300`}
        aria-label="Offline"
      />
    );
  }

  // Unknown
  return (
    <span
      className={`${sizeClass} inline-block rounded-full bg-yellow-400`}
      aria-label="Unknown"
    />
  );
}
