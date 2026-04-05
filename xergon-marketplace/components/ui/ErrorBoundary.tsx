"use client";

import React, { Component, type ReactNode, type ErrorInfo } from "react";
import { AlertTriangle, RefreshCw, WifiOff, ServerCrash, Clock } from "lucide-react";
import { cn } from "@/lib/utils";

// ── Error type detection ──

type ErrorCategory = "network" | "rate_limit" | "server" | "unknown";

function categorizeError(error: Error): ErrorCategory {
  const msg = error.message.toLowerCase();

  // Network errors
  if (
    msg.includes("failed to fetch") ||
    msg.includes("networkerror") ||
    msg.includes("net::err_") ||
    msg.includes("typeerror") ||
    msg.includes("aborterror") ||
    msg.includes("timeout")
  ) {
    return "network";
  }

  // Rate limit (429)
  if (
    msg.includes("429") ||
    msg.includes("rate limit") ||
    msg.includes("too many requests")
  ) {
    return "rate_limit";
  }

  // Server errors (5xx)
  if (
    msg.includes("500") ||
    msg.includes("502") ||
    msg.includes("503") ||
    msg.includes("504") ||
    msg.includes("internal server") ||
    msg.includes("bad gateway") ||
    msg.includes("service unavailable") ||
    msg.includes("gateway timeout")
  ) {
    return "server";
  }

  return "unknown";
}

interface ErrorConfig {
  icon: ReactNode;
  title: string;
  description: string;
  accentColor: string;
}

const ERROR_CONFIGS: Record<ErrorCategory, ErrorConfig> = {
  network: {
    icon: <WifiOff className="w-8 h-8" />,
    title: "Connection Error",
    description: "Unable to reach the server. Check your internet connection and try again.",
    accentColor: "text-amber-500",
  },
  rate_limit: {
    icon: <Clock className="w-8 h-8" />,
    title: "Rate Limited",
    description: "Too many requests. Please wait a moment before trying again.",
    accentColor: "text-yellow-500",
  },
  server: {
    icon: <ServerCrash className="w-8 h-8" />,
    title: "Server Error",
    description: "Something went wrong on our end. Our team has been notified. Please try again.",
    accentColor: "text-danger-500",
  },
  unknown: {
    icon: <AlertTriangle className="w-8 h-8" />,
    title: "Something Went Wrong",
    description: "An unexpected error occurred. Please try again or contact support if the problem persists.",
    accentColor: "text-surface-800/40",
  },
};

// ── ErrorBoundary Props ──

interface ErrorBoundaryProps {
  children: ReactNode;
  fallback?: ReactNode;
  /** Optional custom message shown when error is caught */
  context?: string;
  /** Called when error is caught — useful for logging */
  onError?: (error: Error, errorInfo: ErrorInfo) => void;
  /** Minimum height for the error fallback layout */
  minheight?: string;
  className?: string;
}

interface ErrorBoundaryState {
  hasError: boolean;
  error: Error | null;
  errorCategory: ErrorCategory;
}

/**
 * React Error Boundary that catches render errors in child components.
 * Displays a friendly error message with retry functionality.
 * Never shows raw error stacks to users.
 */
export class ErrorBoundary extends Component<ErrorBoundaryProps, ErrorBoundaryState> {
  constructor(props: ErrorBoundaryProps) {
    super(props);
    this.state = {
      hasError: false,
      error: null,
      errorCategory: "unknown",
    };
  }

  static getDerivedStateFromError(error: Error): ErrorBoundaryState {
    return {
      hasError: true,
      error,
      errorCategory: categorizeError(error),
    };
  }

  componentDidCatch(error: Error, errorInfo: ErrorInfo) {
    console.error("[ErrorBoundary]", error, errorInfo);

    // Call optional error handler
    this.props.onError?.(error, errorInfo);
  }

  handleRetry = () => {
    this.setState({ hasError: false, error: null, errorCategory: "unknown" });
  };

  render() {
    if (this.state.hasError) {
      // Use custom fallback if provided
      if (this.props.fallback) {
        return this.props.fallback;
      }

      const config = ERROR_CONFIGS[this.state.errorCategory];

      return (
        <div
          className={cn(
            "flex flex-col items-center justify-center px-4 py-12 text-center",
            this.props.className,
          )}
          style={{ minHeight: this.props.minheight ?? "300px" }}
        >
          {/* Icon */}
          <div className={cn("mb-4 opacity-60", config.accentColor)}>
            {config.icon}
          </div>

          {/* Title */}
          <h3 className="text-lg font-semibold text-surface-900 mb-1">
            {config.title}
          </h3>

          {/* Description */}
          <p className="text-sm text-surface-800/60 max-w-md mb-1">
            {config.description}
          </p>

          {/* Context hint */}
          {this.props.context && (
            <p className="text-xs text-surface-800/40 mb-4">
              While loading: {this.props.context}
            </p>
          )}

          {/* Retry button */}
          <button
            onClick={this.handleRetry}
            className="inline-flex items-center gap-2 rounded-lg bg-brand-600 px-4 py-2 text-sm font-medium text-white transition-colors hover:bg-brand-700"
          >
            <RefreshCw className="w-4 h-4" />
            Try Again
          </button>
        </div>
      );
    }

    return this.props.children;
  }
}

// ── Functional wrapper for async API errors (not React render errors) ──

interface ApiErrorDisplayProps {
  error: Error | string | null;
  onRetry?: () => void;
  className?: string;
}

/**
 * Displays a friendly error message for API errors.
 * Use this for errors caught in try/catch blocks (not React render errors).
 */
export function ApiErrorDisplay({ error, onRetry, className }: ApiErrorDisplayProps) {
  if (!error) return null;

  const message = typeof error === "string" ? error : error.message;
  const category = typeof error === "string" ? "unknown" : categorizeError(error);
  const config = ERROR_CONFIGS[category];

  return (
    <div
      className={cn(
        "rounded-xl border border-surface-200 bg-surface-0 p-6 text-center",
        className,
      )}
    >
      <div className={cn("mb-3 opacity-50", config.accentColor)}>
        {config.icon}
      </div>
      <h3 className="text-base font-semibold text-surface-900 mb-1">
        {config.title}
      </h3>
      <p className="text-sm text-surface-800/60 max-w-sm mx-auto mb-4">
        {config.description}
      </p>

      {onRetry && (
        <button
          onClick={onRetry}
          className="inline-flex items-center gap-2 rounded-lg bg-brand-600 px-4 py-2 text-sm font-medium text-white transition-colors hover:bg-brand-700"
        >
          <RefreshCw className="w-4 h-4" />
          Try Again
        </button>
      )}
    </div>
  );
}
