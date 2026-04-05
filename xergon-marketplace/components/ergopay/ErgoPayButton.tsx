"use client";

/**
 * ErgoPayButton - triggers the ErgoPay flow for mobile wallet transactions.
 *
 * On click, calls /api/ergopay/request to create a signing request,
 * then opens the ErgoPayModal showing the QR code.
 */

import { useState, useCallback } from "react";
import { toast } from "sonner";
import { ErgoPayModal } from "./ErgoPayModal";
import type { QrCodeData, ErgoPaySigningRequest } from "@/lib/ergopay/types";

export interface ErgoPayButtonProps {
  /** Recipient (target) Ergo address */
  recipientAddress: string;
  /** Amount to send in nanoERG */
  amountNanoerg: number;
  /** Optional tokens to include */
  tokens?: Array<{ tokenId: string; amount: number }>;
  /** Button label text */
  label?: string;
  /** Called when the transaction is signed successfully */
  onSigned?: (txId: string) => void;
  /** Additional CSS class */
  className?: string;
  /** Disabled state */
  disabled?: boolean;
}

export function ErgoPayButton({
  recipientAddress,
  amountNanoerg,
  tokens,
  label = "Pay with ErgoPay",
  onSigned,
  className = "",
  disabled = false,
}: ErgoPayButtonProps) {
  const [modalOpen, setModalOpen] = useState(false);
  const [modalData, setModalData] = useState<{
    requestId: string;
    signingRequest: ErgoPaySigningRequest;
    qrData: QrCodeData;
  } | null>(null);
  const [loading, setLoading] = useState(false);

  const handleOpen = useCallback(async () => {
    setLoading(true);
    try {
      const res = await fetch("/api/ergopay/request", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          // senderAddress will be derived from the connected wallet
          // For now, we pass a placeholder - the actual flow requires
          // the user to have their address available
          senderAddress: "", // will be populated by parent
          amountNanoerg,
          recipientAddress,
          tokens,
        }),
      });

      if (!res.ok) {
        const err = await res.json().catch(() => ({ error: "Unknown error" }));
        throw new Error(err.error || `Request failed (${res.status})`);
      }

      const data = await res.json();
      setModalData(data);
      setModalOpen(true);
    } catch (err) {
      toast.error("Failed to create ErgoPay request", {
        description: err instanceof Error ? err.message : "Unknown error",
      });
    } finally {
      setLoading(false);
    }
  }, [amountNanoerg, recipientAddress, tokens]);

  const handleClose = useCallback(() => {
    setModalOpen(false);
    // Small delay before clearing data for animation
    setTimeout(() => setModalData(null), 300);
  }, []);

  const handleSigned = useCallback(
    (txId: string) => {
      setModalOpen(false);
      setTimeout(() => setModalData(null), 300);
      toast.success("Transaction signed!", {
        description: `TX: ${txId.slice(0, 16)}...`,
      });
      onSigned?.(txId);
    },
    [onSigned]
  );

  return (
    <>
      <button
        onClick={handleOpen}
        disabled={disabled || loading}
        className={`inline-flex items-center gap-2 px-4 py-2.5 rounded-lg text-sm font-medium
          bg-brand-600 text-white hover:bg-brand-700 active:bg-brand-800
          transition-colors disabled:opacity-50 disabled:cursor-not-allowed
          ${className}`}
      >
        {loading ? (
          <>
            <span className="inline-block w-4 h-4 border-2 border-white/30 border-t-white rounded-full animate-spin" />
            Preparing...
          </>
        ) : (
          <>
            <svg
              className="w-4 h-4"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth="2"
              strokeLinecap="round"
              strokeLinejoin="round"
            >
              <rect x="3" y="3" width="7" height="7" />
              <rect x="14" y="3" width="7" height="7" />
              <rect x="3" y="14" width="7" height="7" />
              <rect x="14" y="14" width="3" height="3" />
              <rect x="18" y="14" width="3" height="3" />
              <rect x="14" y="18" width="3" height="3" />
              <rect x="18" y="18" width="3" height="3" />
            </svg>
            {label}
          </>
        )}
      </button>

      {modalOpen && modalData && (
        <ErgoPayModal
          requestId={modalData.requestId}
          signingRequest={modalData.signingRequest}
          qrData={modalData.qrData}
          onClose={handleClose}
          onSigned={handleSigned}
        />
      )}
    </>
  );
}
