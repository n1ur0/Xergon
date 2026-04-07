"use client";

import { useState, useEffect, useCallback, useRef } from "react";
import { cn } from "@/lib/utils";

// ── Types ──

interface ERGPaymentButtonProps {
  amountNanoerg: number;
  description: string;
  recipientAddress: string;
  onSuccess?: (txId: string) => void;
  onError?: (error: string) => void;
  className?: string;
  children?: React.ReactNode;
}

type PaymentStep = "idle" | "pending" | "confirming" | "confirmed" | "error";

interface PaymentState {
  step: PaymentStep;
  txId: string | null;
  requestId: string | null;
  error: string | null;
  pollCount: number;
}

// ── Helpers ──

function nanoergToErg(nanoerg: number): string {
  if (nanoerg <= 0) return "0";
  const erg = nanoerg / 1e9;
  return erg.toFixed(6).replace(/0+$/, "").replace(/\.$/, "");
}

const ERG_USD_RATE = 1.85; // Approximate, would be fetched from oracle

function ergToUsd(erg: number): string {
  return (erg * ERG_USD_RATE).toFixed(2);
}

// ── Component ──

export function ERGPaymentButton({
  amountNanoerg,
  description,
  recipientAddress,
  onSuccess,
  onError,
  className,
  children,
}: ERGPaymentButtonProps) {
  const [state, setState] = useState<PaymentState>({
    step: "idle",
    txId: null,
    requestId: null,
    error: null,
    pollCount: 0,
  });

  const abortRef = useRef<AbortController | null>(null);

  const ergAmount = nanoergToErg(amountNanoerg);
  const usdAmount = ergToUsd(parseFloat(ergAmount));

  const pollPaymentStatus = useCallback(
    async (requestId: string) => {
      const maxPolls = 60;
      let count = 0;

      const poll = async () => {
        if (count >= maxPolls) {
          setState((prev) => ({ ...prev, step: "error", error: "Payment timed out. Please check your wallet." }));
          onError?.("Payment timed out");
          return;
        }

        try {
          const res = await fetch(`/api/ergopay/status/${encodeURIComponent(requestId)}`);
          if (!res.ok) throw new Error("Status check failed");

          const data = await res.json();
          count++;

          if (data.status === "confirmed" && data.txId) {
            setState({ step: "confirmed", txId: data.txId, requestId, error: null, pollCount: count });
            onSuccess?.(data.txId);
            return;
          }

          if (data.status === "failed") {
            setState({ step: "error", txId: null, requestId, error: "Payment failed", pollCount: count });
            onError?.("Payment failed");
            return;
          }

          setState((prev) => ({ ...prev, pollCount: count }));
          setTimeout(poll, 3000);
        } catch {
          count++;
          setTimeout(poll, 3000);
        }
      };

      poll();
    },
    [onSuccess, onError],
  );

  const handlePay = async () => {
    setState((prev) => ({ ...prev, step: "pending", error: null }));

    try {
      const res = await fetch("/api/ergopay/request", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          amountNanoerg,
          recipientAddress,
          description,
        }),
      });

      if (!res.ok) {
        const errText = await res.text().catch(() => "Unknown error");
        throw new Error(errText);
      }

      const data = await res.json();

      if (data.requestId) {
        setState((prev) => ({ ...prev, step: "confirming", requestId: data.requestId }));
        pollPaymentStatus(data.requestId);
      } else {
        throw new Error("No request ID returned");
      }
    } catch (err) {
      const msg = err instanceof Error ? err.message : "Payment request failed";
      setState((prev) => ({ ...prev, step: "error", error: msg }));
      onError?.(msg);
    }
  };

  const isProcessing = state.step === "pending" || state.step === "confirming";

  return (
    <button
      onClick={handlePay}
      disabled={isProcessing || state.step === "confirmed"}
      className={cn(
        "inline-flex items-center gap-2 rounded-lg px-4 py-2.5 text-sm font-medium transition-colors",
        isProcessing
          ? "bg-brand-400 text-white cursor-wait"
          : state.step === "confirmed"
            ? "bg-green-500 text-white cursor-default"
            : state.step === "error"
              ? "bg-red-600 text-white hover:bg-red-700"
              : "bg-brand-600 text-white hover:bg-brand-700",
        className,
      )}
    >
      {/* ERG icon */}
      <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className="opacity-80">
        <line x1="12" x2="12" y1="2" y2="22" />
        <path d="M17 5H9.5a3.5 3.5 0 0 0 0 7h5a3.5 3.5 0 0 1 0 7H6" />
      </svg>

      {state.step === "idle" && (
        <>Pay with ERG ({children ?? `${ergAmount} ERG`})</>
      )}
      {state.step === "pending" && "Requesting..."}
      {state.step === "confirming" && `Waiting... (${state.pollCount * 3}s)`}
      {state.step === "confirmed" && "Paid!"}
      {state.step === "error" && "Retry Payment"}
    </button>
  );
}

// ── USD equivalent display ──

export function ERGAmountDisplay({ amountNanoerg }: { amountNanoerg: number }) {
  const ergAmount = nanoergToErg(amountNanoerg);
  const usdAmount = ergToUsd(parseFloat(ergAmount));

  return (
    <div className="flex items-baseline gap-1">
      <span className="font-semibold text-surface-900">{ergAmount} ERG</span>
      <span className="text-xs text-surface-800/40">~${usdAmount} USD</span>
    </div>
  );
}

// ── Transaction link ──

export function ErgoExplorerTxLink({ txId }: { txId: string }) {
  return (
    <a
      href={`https://explorer.ergoplatform.com/en/transactions/${txId}`}
      target="_blank"
      rel="noopener noreferrer"
      className="inline-flex items-center gap-1 text-xs text-brand-600 hover:text-brand-700 transition-colors"
    >
      View on ErgoExplorer
      <svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
        <path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6" />
        <polyline points="15 3 21 3 21 9" />
        <line x1="10" x2="21" y1="14" y2="3" />
      </svg>
    </a>
  );
}
