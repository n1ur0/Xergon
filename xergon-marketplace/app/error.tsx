"use client";

import { AlertTriangle, RefreshCw, WifiOff, ServerCrash, Clock } from "lucide-react";

type ErrorCategory = "network" | "rate_limit" | "server" | "unknown";

function categorizeError(error: Error & { digest?: string }): ErrorCategory {
  const msg = error.message.toLowerCase();

  if (
    msg.includes("failed to fetch") ||
    msg.includes("networkerror") ||
    msg.includes("net::err_") ||
    msg.includes("timeout")
  ) {
    return "network";
  }

  if (
    msg.includes("429") ||
    msg.includes("rate limit") ||
    msg.includes("too many requests")
  ) {
    return "rate_limit";
  }

  if (
    msg.includes("500") ||
    msg.includes("502") ||
    msg.includes("503") ||
    msg.includes("504") ||
    msg.includes("internal server") ||
    msg.includes("bad gateway") ||
    msg.includes("service unavailable")
  ) {
    return "server";
  }

  return "unknown";
}

const ERROR_CONFIGS: Record<ErrorCategory, {
  icon: React.ReactNode;
  title: string;
  description: string;
}> = {
  network: {
    icon: <WifiOff className="w-8 h-8 text-amber-500" />,
    title: "Connection Error",
    description: "Unable to reach the server. Check your internet connection and try again.",
  },
  rate_limit: {
    icon: <Clock className="w-8 h-8 text-yellow-500" />,
    title: "Rate Limited",
    description: "Too many requests. Please wait a moment before trying again.",
  },
  server: {
    icon: <ServerCrash className="w-8 h-8 text-danger-500" />,
    title: "Server Error",
    description: "Something went wrong on our end. Our team has been notified.",
  },
  unknown: {
    icon: <AlertTriangle className="w-8 h-8 text-surface-800/40" />,
    title: "Something Went Wrong",
    description: "An unexpected error occurred. Please try again.",
  },
};

export default function Error({
  error,
  reset,
}: {
  error: Error & { digest?: string };
  reset: () => void;
}) {
  const category = categorizeError(error);
  const config = ERROR_CONFIGS[category];

  return (
    <div className="mx-auto flex min-h-[60vh] max-w-md flex-col items-center justify-center px-4 text-center">
      <div className="mb-4 opacity-60">{config.icon}</div>
      <h2 className="text-xl font-bold text-surface-900">{config.title}</h2>
      <p className="mt-2 text-sm text-surface-800/60 max-w-sm">
        {config.description}
      </p>
      {error.digest && (
        <p className="mt-2 text-xs text-surface-800/30 font-mono">
          Error ID: {error.digest}
        </p>
      )}
      <button
        onClick={reset}
        className="mt-6 inline-flex items-center gap-2 rounded-lg bg-brand-600 px-4 py-2 text-sm font-medium text-white transition-colors hover:bg-brand-700"
      >
        <RefreshCw className="w-4 h-4" />
        Try Again
      </button>
    </div>
  );
}
