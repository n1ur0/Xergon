"use client";

import { toast as sonnerToast } from "sonner";

/**
 * Enhanced toast notification utilities built on Sonner.
 *
 * Usage:
 *   import { toast } from "@/components/ui/Toast";
 *   toast.success("Rental created!");
 *   toast.error("Transaction failed", { description: "Insufficient ERG balance" });
 *   toast.info("Pending", { description: "Waiting for blockchain confirmation..." });
 */

type ToastVariant = "success" | "error" | "info" | "warning";

interface ExtendedToastOptions {
  description?: string;
  duration?: number;
}

// ── Base toast helpers ──

function createToast(
  variant: ToastVariant,
  message: string,
  options?: ExtendedToastOptions,
) {
  const duration = options?.duration ?? (variant === "error" ? 6000 : variant === "info" ? 5000 : 4000);

  const base = sonnerToast[variant];
  return base(message, {
    description: options?.description,
    duration,
  });
}

// ── Domain-specific toast presets ──

export const toast = {
  /** Generic success toast */
  success: (message: string, options?: ExtendedToastOptions) =>
    createToast("success", message, options),

  /** Generic error toast */
  error: (message: string, options?: ExtendedToastOptions) =>
    createToast("error", message, options),

  /** Generic info toast */
  info: (message: string, options?: ExtendedToastOptions) =>
    createToast("info", message, options),

  /** Generic warning toast */
  warning: (message: string, options?: ExtendedToastOptions) =>
    createToast("warning", message, options),

  // ── GPU Rental presets ──

  rentalPending: () =>
    sonnerToast.info("Rental Pending", {
      description: "Waiting for blockchain confirmation...",
      duration: 8000,
    }),

  rentalSuccess: (gpuType?: string) =>
    sonnerToast.success("GPU Rented!", {
      description: gpuType
        ? `${gpuType} has been reserved for you.`
        : "Your GPU rental is confirmed on-chain.",
      duration: 5000,
    }),

  rentalFailed: (reason?: string) =>
    sonnerToast.error("Rental Failed", {
      description: reason ?? "The rental transaction could not be completed. Please try again.",
      duration: 6000,
    }),

  insufficientBalance: () =>
    sonnerToast.error("Insufficient Balance", {
      description: "You don't have enough ERG to complete this rental. Please top up your wallet.",
      duration: 6000,
    }),

  // ── Rating presets ──

  ratingSubmitted: () =>
    sonnerToast.success("Rating Submitted", {
      description: "Thank you for your feedback!",
      duration: 4000,
    }),

  ratingFailed: () =>
    sonnerToast.error("Rating Failed", {
      description: "Could not submit your rating. Please try again.",
      duration: 6000,
    }),

  // ── Wallet presets ──

  walletConnected: (address?: string) =>
    sonnerToast.success("Wallet Connected", {
      description: address
        ? `Connected as ${address.slice(0, 10)}...${address.slice(-4)}`
        : "Your Ergo wallet is linked.",
      duration: 4000,
    }),

  walletDisconnected: () =>
    sonnerToast.info("Wallet Disconnected", {
      description: "Your wallet has been safely disconnected.",
      duration: 4000,
    }),

  // ── Network presets ──

  networkError: () =>
    sonnerToast.error("Connection Lost", {
      description: "Unable to reach the server. Check your connection and try again.",
      duration: 8000,
    }),

  rateLimited: () =>
    sonnerToast.warning("Slow Down", {
      description: "Too many requests. Please wait a moment before trying again.",
      duration: 5000,
    }),

  // ── Dismiss all ──

  dismissAll: () => sonnerToast.dismiss(),
};

export type { ExtendedToastOptions };
