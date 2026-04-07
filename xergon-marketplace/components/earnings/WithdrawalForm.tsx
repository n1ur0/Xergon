"use client";

import { useState } from "react";
import { cn } from "@/lib/utils";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface WithdrawalFormProps {
  balanceNanoErg: number;
  onSuccess: (txId: string) => void;
}

interface FormErrors {
  amount?: string;
  address?: string;
  general?: string;
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const NETWORK_FEE_NANO_ERG = 1_000_000; // 0.001 ERG
const MIN_WITHDRAWAL_NANO_ERG = 1_000_000; // 0.001 ERG

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function WithdrawalForm({
  balanceNanoErg,
  onSuccess,
}: WithdrawalFormProps) {
  const [amountErg, setAmountErg] = useState("");
  const [destinationAddress, setDestinationAddress] = useState("");
  const [errors, setErrors] = useState<FormErrors>({});
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [showConfirm, setShowConfirm] = useState(false);

  const balanceErg = balanceNanoErg / 1e9;
  const amountNanoErg = Math.floor(parseFloat(amountErg || "0") * 1e9);

  function validate(): boolean {
    const newErrors: FormErrors = {};

    if (!amountErg || parseFloat(amountErg) <= 0) {
      newErrors.amount = "Enter a withdrawal amount";
    } else if (amountNanoErg < MIN_WITHDRAWAL_NANO_ERG) {
      newErrors.amount = "Minimum withdrawal is 0.001 ERG";
    } else if (amountNanoErg + NETWORK_FEE_NANO_ERG > balanceNanoErg) {
      newErrors.amount = "Amount + fee exceeds your balance";
    }

    if (!destinationAddress.trim()) {
      newErrors.address = "Enter a destination address";
    } else if (!/^3[a-km-zA-HJ-NP-Z1-9]{8,}$/.test(destinationAddress.trim())) {
      newErrors.address =
        "Invalid Ergo address (must start with 3, Base58 format)";
    }

    setErrors(newErrors);
    return Object.keys(newErrors).length === 0;
  }

  function handleWithdrawClick() {
    if (validate()) {
      setShowConfirm(true);
    }
  }

  async function handleConfirm() {
    setIsSubmitting(true);
    try {
      const res = await fetch("/api/earnings/withdraw", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          amountNanoErg,
          destinationAddress: destinationAddress.trim(),
        }),
      });

      if (!res.ok) {
        const body = await res.json().catch(() => ({ error: "Withdrawal failed" }));
        setErrors({ general: body.error || "Withdrawal failed" });
        setShowConfirm(false);
        return;
      }

      const data = await res.json();
      onSuccess(data.txId);
    } catch {
      setErrors({ general: "Network error. Please try again." });
      setShowConfirm(false);
    } finally {
      setIsSubmitting(false);
    }
  }

  function handleMaxClick() {
    // Max = balance - fee
    const maxErg = Math.max(0, (balanceNanoErg - NETWORK_FEE_NANO_ERG) / 1e9);
    setAmountErg(maxErg.toFixed(6));
    setErrors((prev) => ({ ...prev, amount: undefined }));
  }

  return (
    <>
      <div className="space-y-5">
        {/* Balance display */}
        <div className="rounded-xl border border-surface-200 bg-surface-0 p-5">
          <div className="text-sm text-surface-800/60 mb-1">
            Available Balance
          </div>
          <div className="text-3xl font-bold text-surface-900">
            {balanceErg.toFixed(4)}{" "}
            <span className="text-lg font-normal text-surface-800/40">ERG</span>
          </div>
          <div className="text-xs text-surface-800/30 mt-1">
            {balanceNanoErg.toLocaleString()} nanoERG
          </div>
        </div>

        {/* General error */}
        {errors.general && (
          <div className="rounded-lg border border-red-200 bg-red-50 dark:border-red-800/40 dark:bg-red-950/20 px-4 py-3 text-sm text-red-600 dark:text-red-400">
            {errors.general}
          </div>
        )}

        {/* Amount input */}
        <div>
          <label
            htmlFor="withdraw-amount"
            className="block text-sm font-medium text-surface-800/70 mb-1.5"
          >
            Amount (ERG)
          </label>
          <div className="relative">
            <input
              id="withdraw-amount"
              type="number"
              step="0.0001"
              min="0.001"
              max={balanceErg.toFixed(6)}
              placeholder="0.0000"
              value={amountErg}
              onChange={(e) => {
                setAmountErg(e.target.value);
                setErrors((prev) => ({ ...prev, amount: undefined }));
              }}
              className={cn(
                "w-full rounded-lg border bg-surface-0 px-4 py-2.5 pr-16 text-sm text-surface-900 placeholder:text-surface-800/30 focus:outline-none focus:ring-2 focus:ring-brand-500/30 focus:border-brand-500",
                errors.amount
                  ? "border-red-300 dark:border-red-700"
                  : "border-surface-200",
              )}
            />
            <button
              type="button"
              onClick={handleMaxClick}
              className="absolute right-3 top-1/2 -translate-y-1/2 text-xs font-medium text-brand-600 hover:text-brand-700 transition-colors"
            >
              MAX
            </button>
          </div>
          {errors.amount && (
            <p className="mt-1 text-xs text-red-500">{errors.amount}</p>
          )}
        </div>

        {/* Destination address */}
        <div>
          <label
            htmlFor="withdraw-address"
            className="block text-sm font-medium text-surface-800/70 mb-1.5"
          >
            Destination Address
          </label>
          <input
            id="withdraw-address"
            type="text"
            placeholder="3..."
            value={destinationAddress}
            onChange={(e) => {
              setDestinationAddress(e.target.value);
              setErrors((prev) => ({ ...prev, address: undefined }));
            }}
            className={cn(
              "w-full rounded-lg border bg-surface-0 px-4 py-2.5 font-mono text-sm text-surface-900 placeholder:text-surface-800/30 focus:outline-none focus:ring-2 focus:ring-brand-500/30 focus:border-brand-500",
              errors.address
                ? "border-red-300 dark:border-red-700"
                : "border-surface-200",
            )}
          />
          {errors.address && (
            <p className="mt-1 text-xs text-red-500">{errors.address}</p>
          )}
        </div>

        {/* Network fee */}
        <div className="flex items-center justify-between rounded-lg border border-surface-200 bg-surface-50 dark:bg-surface-900 px-4 py-3">
          <span className="text-sm text-surface-800/60">Network Fee</span>
          <span className="text-sm font-medium text-surface-900">
            {(NETWORK_FEE_NANO_ERG / 1e9).toFixed(3)} ERG
          </span>
        </div>

        {/* You receive */}
        {amountNanoErg > 0 && (
          <div className="flex items-center justify-between rounded-lg border border-emerald-200 bg-emerald-50 dark:border-emerald-800/40 dark:bg-emerald-950/20 px-4 py-3">
            <span className="text-sm text-emerald-700 dark:text-emerald-400">
              Destination receives
            </span>
            <span className="text-sm font-semibold text-emerald-800 dark:text-emerald-300">
              {(amountNanoErg / 1e9).toFixed(4)} ERG
            </span>
          </div>
        )}

        {/* Submit button */}
        <button
          type="button"
          onClick={handleWithdrawClick}
          disabled={isSubmitting}
          className="w-full rounded-lg bg-brand-600 px-4 py-2.5 text-sm font-semibold text-white transition-colors hover:bg-brand-700 disabled:opacity-50 disabled:cursor-not-allowed"
        >
          Withdraw
        </button>
      </div>

      {/* Confirmation modal */}
      {showConfirm && (
        <div className="fixed inset-0 z-[80] flex items-center justify-center p-4">
          {/* Backdrop */}
          <div
            className="absolute inset-0 bg-black/50 backdrop-blur-sm"
            onClick={() => setShowConfirm(false)}
          />

          {/* Modal */}
          <div className="relative w-full max-w-md rounded-xl border border-surface-200 bg-surface-0 p-6 shadow-2xl">
            <h3 className="text-lg font-bold text-surface-900 mb-4">
              Confirm Withdrawal
            </h3>

            <div className="space-y-3 mb-6">
              <div className="flex justify-between text-sm">
                <span className="text-surface-800/60">Amount</span>
                <span className="font-medium text-surface-900">
                  {(amountNanoErg / 1e9).toFixed(4)} ERG
                </span>
              </div>
              <div className="flex justify-between text-sm">
                <span className="text-surface-800/60">Network Fee</span>
                <span className="font-medium text-surface-900">
                  {(NETWORK_FEE_NANO_ERG / 1e9).toFixed(3)} ERG
                </span>
              </div>
              <div className="border-t border-surface-200 pt-3 flex justify-between text-sm">
                <span className="text-surface-800/60">Total</span>
                <span className="font-bold text-surface-900">
                  {((amountNanoErg + NETWORK_FEE_NANO_ERG) / 1e9).toFixed(4)}{" "}
                  ERG
                </span>
              </div>
              <div className="flex justify-between text-sm">
                <span className="text-surface-800/60">Destination</span>
                <span className="font-mono text-xs text-surface-900">
                  {destinationAddress.slice(0, 12)}...
                  {destinationAddress.slice(-6)}
                </span>
              </div>
            </div>

            <div className="flex gap-3">
              <button
                type="button"
                onClick={() => setShowConfirm(false)}
                disabled={isSubmitting}
                className="flex-1 rounded-lg border border-surface-200 bg-surface-0 px-4 py-2.5 text-sm font-medium text-surface-800 transition-colors hover:bg-surface-100 disabled:opacity-50"
              >
                Cancel
              </button>
              <button
                type="button"
                onClick={handleConfirm}
                disabled={isSubmitting}
                className="flex-1 rounded-lg bg-brand-600 px-4 py-2.5 text-sm font-semibold text-white transition-colors hover:bg-brand-700 disabled:opacity-50"
              >
                {isSubmitting ? (
                  <span className="inline-flex items-center gap-2">
                    <span className="h-4 w-4 animate-spin rounded-full border-2 border-white/30 border-t-white" />
                    Processing...
                  </span>
                ) : (
                  "Confirm"
                )}
              </button>
            </div>
          </div>
        </div>
      )}
    </>
  );
}
