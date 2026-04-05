"use client";

import { useState, useEffect, useRef, useCallback } from "react";
import { generateSvgQrCode } from "@/lib/ergopay/qr";
import type {
  QrCodeData,
  ErgoPaySigningRequest,
  ErgoPayStatus,
} from "@/lib/ergopay/types";
import { useFocusTrap } from "@/lib/a11y/utils";

// ---------------------------------------------------------------------------
// Props
// ---------------------------------------------------------------------------

export interface ErgoPayModalProps {
  requestId: string;
  signingRequest: ErgoPaySigningRequest;
  qrData: QrCodeData;
  onClose: () => void;
  onSigned: (txId: string) => void;
  /** Polling interval in ms (default 3000) */
  pollInterval?: number;
  /** Auto-close delay after signing in ms (default 2000) */
  autoCloseDelay?: number;
  /** Max wait time in ms before showing timeout (default 5 min) */
  timeoutMs?: number;
}

// ---------------------------------------------------------------------------
// Status display config
// ---------------------------------------------------------------------------

const STATUS_CONFIG: Record<
  ErgoPayStatus,
  { label: string; color: string; icon: string }
> = {
  pending: {
    label: "Waiting for Wallet",
    color: "text-amber-500",
    icon: "⏳",
  },
  signed: {
    label: "Transaction Signed",
    color: "text-emerald-500",
    icon: "✅",
  },
  submitted: {
    label: "Transaction Submitted",
    color: "text-emerald-500",
    icon: "🚀",
  },
  expired: {
    label: "Request Expired",
    color: "text-red-500",
    icon: "⏰",
  },
};

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function ErgoPayModal({
  requestId,
  signingRequest,
  qrData,
  onClose,
  onSigned,
  pollInterval = 3000,
  autoCloseDelay = 2000,
  timeoutMs = 5 * 60 * 1000,
}: ErgoPayModalProps) {
  const [status, setStatus] = useState<ErgoPayStatus>("pending");
  const [txId, setTxId] = useState<string | null>(null);
  const [timedOut, setTimedOut] = useState(false);
  const [qrSvg, setQrSvg] = useState<string>("");
  const startTimeRef = useRef(Date.now());
  const signedHandledRef = useRef(false);

  const focusTrapRef = useFocusTrap<HTMLDivElement>(true, onClose);

  // Generate QR code SVG on mount
  useEffect(() => {
    try {
      // Use deepLink for QR (works for both inline and URL-based)
      const svg = generateSvgQrCode(qrData.deepLink, 240, "#1a1a2e", "#ffffff");
      setQrSvg(svg);
    } catch (err) {
      console.error("[ErgoPay] Failed to generate QR:", err);
    }
  }, [qrData]);

  // Poll for status
  useEffect(() => {
    let active = true;

    async function poll() {
      if (!active) return;

      try {
        const res = await fetch(`/api/ergopay/status/${requestId}`);
        if (!res.ok) return;

        const data = await res.json();
        if (!active) return;

        setStatus(data.status);
        if (data.txId) setTxId(data.txId);
      } catch {
        // Silently retry on network error
      }
    }

    // Initial poll
    poll();

    // Set up interval
    const interval = setInterval(poll, pollInterval);

    return () => {
      active = false;
      clearInterval(interval);
    };
  }, [requestId, pollInterval]);

  // Handle timeout
  useEffect(() => {
    const timer = setTimeout(() => {
      setTimedOut(true);
    }, timeoutMs);
    return () => clearTimeout(timer);
  }, [timeoutMs]);

  // Auto-close on signed
  useEffect(() => {
    if (
      (status === "signed" || status === "submitted") &&
      txId &&
      !signedHandledRef.current
    ) {
      signedHandledRef.current = true;
      const timer = setTimeout(() => {
        onSigned(txId);
      }, autoCloseDelay);
      return () => clearTimeout(timer);
    }
  }, [status, txId, onSigned, autoCloseDelay]);

  // Close on Escape key
  useEffect(() => {
    function handleKey(e: KeyboardEvent) {
      if (e.key === "Escape") onClose();
    }
    window.addEventListener("keydown", handleKey);
    return () => window.removeEventListener("keydown", handleKey);
  }, [onClose]);

  const handleDeepLink = useCallback(() => {
    // Try to open the deep link
    window.location.href = qrData.deepLink;
  }, [qrData]);

  const handleCopyLink = useCallback(() => {
    navigator.clipboard.writeText(qrData.ergoPayUrl).then(() => {
      // Brief feedback
    });
  }, [qrData]);

  const config = STATUS_CONFIG[status] ?? STATUS_CONFIG.pending;
  const isFinal = status === "signed" || status === "submitted" || status === "expired";
  const nanoergToErg = (n: number) => (n / 1e9).toFixed(6);

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center" role="presentation">
      {/* Backdrop */}
      <div
        className="absolute inset-0 bg-black/50 backdrop-blur-sm"
        onClick={onClose}
        aria-hidden="true"
      />

      {/* Modal */}
      <div
        ref={focusTrapRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby="ergopay-title"
        className="relative bg-surface-0 border border-surface-200 rounded-2xl shadow-2xl p-6 max-w-sm w-full mx-4 animate-in fade-in zoom-in-95 duration-200"
      >
        {/* Close button */}
        <button
          onClick={onClose}
          aria-label="Close ErgoPay dialog"
          className="absolute top-4 right-4 p-1.5 rounded-lg text-surface-800/40 hover:text-surface-800/70 hover:bg-surface-100 transition-colors"
        >
          <svg className="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <line x1="18" y1="6" x2="6" y2="18" />
            <line x1="6" y1="6" x2="18" y2="18" />
          </svg>
        </button>

        {/* Header */}
        <div className="flex items-center gap-3 mb-5">
          <div className="w-10 h-10 rounded-xl bg-brand-100 flex items-center justify-center text-lg">
            {isFinal ? config.icon : "📱"}
          </div>
          <div>
            <h3 id="ergopay-title" className="font-semibold text-surface-900">ErgoPay</h3>
            <p className={`text-xs font-medium ${config.color}`}>
              {config.icon} {config.label}
            </p>
          </div>
        </div>

        {/* QR Code */}
        {!isFinal && (
          <div className="flex flex-col items-center mb-5">
            <div className="bg-white rounded-xl p-3 mb-3 border border-surface-100">
              {qrSvg ? (
                <div dangerouslySetInnerHTML={{ __html: qrSvg }} />
              ) : (
                <div className="w-60 h-60 bg-surface-100 rounded-lg animate-pulse flex items-center justify-center text-surface-800/30 text-sm">
                  Generating QR...
                </div>
              )}
            </div>
            <p className="text-xs text-surface-800/50 text-center">
              Scan with your Ergo wallet app
            </p>
          </div>
        )}

        {/* Success display */}
        {(status === "signed" || status === "submitted") && txId && (
          <div className="mb-5 p-4 rounded-xl bg-emerald-50 border border-emerald-200">
            <p className="text-sm font-medium text-emerald-700 mb-1">
              Transaction {status === "submitted" ? "submitted" : "signed"}!
            </p>
            <p className="text-xs font-mono text-emerald-600 break-all">
              {txId}
            </p>
          </div>
        )}

        {/* Expired */}
        {status === "expired" && (
          <div className="mb-5 p-4 rounded-xl bg-red-50 border border-red-200">
            <p className="text-sm font-medium text-red-700">
              This request has expired. Please try again.
            </p>
          </div>
        )}

        {/* Timeout */}
        {timedOut && status === "pending" && (
          <div className="mb-5 p-4 rounded-xl bg-amber-50 border border-amber-200">
            <p className="text-sm font-medium text-amber-700">
              Taking longer than expected. Still waiting for your wallet...
            </p>
          </div>
        )}

        {/* Transaction summary */}
        <div className="bg-surface-50 border border-surface-100 rounded-xl p-3 mb-4 space-y-2">
          <div className="flex justify-between text-sm">
            <span className="text-surface-800/50">Amount</span>
            <span className="font-mono font-medium">
              {signingRequest.sendTo?.[0]?.amount ??
                `${nanoergToErg(signingRequest.outputsTotal - signingRequest.fee)} ERG`}
            </span>
          </div>
          <div className="flex justify-between text-sm">
            <span className="text-surface-800/50">Fee</span>
            <span className="font-mono text-surface-800/60">
              {nanoergToErg(signingRequest.fee)} ERG
            </span>
          </div>
          {signingRequest.sendTo?.[0] && (
            <div className="pt-1 border-t border-surface-100">
              <span className="text-xs text-surface-800/40 block mb-0.5">
                Recipient
              </span>
              <span className="text-xs font-mono text-surface-800/70 break-all">
                {signingRequest.sendTo[0].address}
              </span>
            </div>
          )}
        </div>

        {/* Actions */}
        {!isFinal && (
          <div className="flex gap-2">
            <button
              onClick={handleDeepLink}
              className="flex-1 px-4 py-2.5 rounded-lg text-sm font-medium bg-brand-600 text-white hover:bg-brand-700 transition-colors"
            >
              Open Wallet App
            </button>
            <button
              onClick={handleCopyLink}
              aria-label="Copy ErgoPay URL"
              className="px-3 py-2.5 rounded-lg text-sm font-medium bg-surface-100 text-surface-800/70 hover:bg-surface-200 transition-colors"
            >
              <svg className="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                <rect x="9" y="9" width="13" height="13" rx="2" ry="2" />
                <path d="M5 15H4a2 2 0 01-2-2V4a2 2 0 012-2h9a2 2 0 012 2v1" />
              </svg>
            </button>
          </div>
        )}

        {isFinal && (
          <button
            onClick={onClose}
            className="w-full px-4 py-2.5 rounded-lg text-sm font-medium bg-surface-100 text-surface-800/70 hover:bg-surface-200 transition-colors"
          >
            Close
          </button>
        )}
      </div>
    </div>
  );
}
