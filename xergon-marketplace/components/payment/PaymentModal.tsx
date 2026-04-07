"use client";

import { useState, useCallback, useRef } from "react";
import { cn } from "@/lib/utils";
import { ERGPaymentButton, ERGAmountDisplay, ErgoExplorerTxLink } from "./ERGPaymentButton";

// ── Types ──

interface PaymentModalProps {
  isOpen: boolean;
  onClose: () => void;
  /** If set, this is a per-use payment for a specific purpose */
  purpose?: string;
  /** Default amount in nanoerg for per-use payments */
  defaultAmountNanoerg?: number;
  /** Recipient Ergo address */
  recipientAddress: string;
  /** Called on successful payment */
  onSuccess?: (txId: string) => void;
}

type PaymentStep = "select" | "paying" | "receipt";

interface Transaction {
  id: string;
  amountNanoerg: number;
  date: string;
  status: "pending" | "confirmed" | "failed";
  txId?: string;
  description: string;
}

// ── Helpers ──

const PRESET_AMOUNTS_NANOERG = [
  { label: "0.1 ERG", nanoerg: 100_000_000 },
  { label: "0.5 ERG", nanoerg: 500_000_000 },
  { label: "1 ERG", nanoerg: 1_000_000_000 },
  { label: "5 ERG", nanoerg: 5_000_000_000 },
  { label: "10 ERG", nanoerg: 10_000_000_000 },
];

function nanoergToErg(nanoerg: number): string {
  if (nanoerg <= 0) return "0";
  const erg = nanoerg / 1e9;
  return erg.toFixed(6).replace(/0+$/, "").replace(/\.$/, "");
}

const ERG_USD_RATE = 1.85;

function ergToUsd(erg: number): string {
  return (erg * ERG_USD_RATE).toFixed(2);
}

// Mock transaction history
const MOCK_HISTORY: Transaction[] = [
  { id: "1", amountNanoerg: 500_000_000, date: "2025-12-18 14:32", status: "confirmed", txId: "abc123def456", description: "Top-up balance" },
  { id: "2", amountNanoerg: 200_000_000, date: "2025-12-17 09:15", status: "confirmed", txId: "def789ghi012", description: "Model inference" },
];

// ── Component ──

export function PaymentModal({
  isOpen,
  onClose,
  purpose,
  defaultAmountNanoerg,
  recipientAddress,
  onSuccess,
}: PaymentModalProps) {
  const [step, setStep] = useState<PaymentStep>(purpose ? "paying" : "select");
  const [selectedAmount, setSelectedAmount] = useState(defaultAmountNanoerg ?? 1_000_000_000);
  const [customAmount, setCustomAmount] = useState("");
  const [activeTab, setActiveTab] = useState<"pay" | "history">("pay");
  const [lastTxId, setLastTxId] = useState<string | null>(null);

  const isTopUp = !purpose;

  const handleAmountSelect = (nanoerg: number) => {
    setSelectedAmount(nanoerg);
    setCustomAmount("");
  };

  const handleCustomAmount = (val: string) => {
    setCustomAmount(val);
    const erg = parseFloat(val);
    if (!isNaN(erg) && erg > 0) {
      setSelectedAmount(Math.round(erg * 1e9));
    }
  };

  const handlePaymentSuccess = useCallback(
    (txId: string) => {
      setLastTxId(txId);
      setStep("receipt");
    },
    [],
  );

  const handleClose = () => {
    setStep(isTopUp ? "select" : "paying");
    setActiveTab("pay");
    setLastTxId(null);
    onClose();
  };

  if (!isOpen) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      {/* Backdrop */}
      <div className="absolute inset-0 bg-black/40 backdrop-blur-sm" onClick={handleClose} />

      {/* Modal */}
      <div className="relative w-full max-w-md mx-4 rounded-2xl bg-surface-0 shadow-xl border border-surface-200 overflow-hidden">
        {/* Header */}
        <div className="flex items-center justify-between px-6 py-4 border-b border-surface-100">
          <h2 className="text-lg font-semibold text-surface-900">
            {isTopUp ? "Top Up Balance" : purpose}
          </h2>
          <button
            onClick={handleClose}
            className="rounded-lg p-1.5 text-surface-800/40 hover:bg-surface-100 hover:text-surface-800/70 transition-colors"
          >
            <svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <path d="M18 6 6 18" />
              <path d="m6 6 12 12" />
            </svg>
          </button>
        </div>

        {/* Tabs for top-up mode */}
        {isTopUp && (
          <div className="flex items-center border-b border-surface-200">
            {(["pay", "history"] as const).map((tab) => (
              <button
                key={tab}
                onClick={() => setActiveTab(tab)}
                className={cn(
                  "flex-1 px-4 py-2.5 text-sm font-medium transition-colors relative capitalize",
                  activeTab === tab
                    ? "text-brand-600"
                    : "text-surface-800/40 hover:text-surface-800/70",
                )}
              >
                {tab === "pay" ? "Add ERG" : "History"}
                {activeTab === tab && (
                  <span className="absolute bottom-0 left-0 right-0 h-0.5 bg-brand-600" />
                )}
              </button>
            ))}
          </div>
        )}

        {/* Content */}
        <div className="p-6">
          {activeTab === "history" ? (
            <TransactionHistory />
          ) : step === "select" ? (
            <AmountSelection
              selectedAmount={selectedAmount}
              customAmount={customAmount}
              onPresetSelect={handleAmountSelect}
              onCustomChange={handleCustomAmount}
            />
          ) : step === "paying" ? (
            <PaymentFlow
              amountNanoerg={selectedAmount}
              recipientAddress={recipientAddress}
              description={purpose ?? "Top-up balance"}
              onSuccess={(txId) => {
                setLastTxId(txId);
                handlePaymentSuccess(txId);
                onSuccess?.(txId);
              }}
            />
          ) : (
            <Receipt txId={lastTxId ?? ""} amountNanoerg={selectedAmount} />
          )}
        </div>

        {/* Footer actions */}
        {step === "select" && (
          <div className="px-6 pb-6">
            <ERGPaymentButton
              amountNanoerg={selectedAmount}
              recipientAddress={recipientAddress}
              description={purpose ?? "Top-up balance"}
              onSuccess={(txId) => {
                setLastTxId(txId);
                handlePaymentSuccess(txId);
                onSuccess?.(txId);
              }}
              onError={() => setStep("select")}
              className="w-full justify-center"
            >
              {nanoergToErg(selectedAmount)} ERG (~${ergToUsd(selectedAmount / 1e9)})
            </ERGPaymentButton>
          </div>
        )}

        {step === "receipt" && (
          <div className="px-6 pb-6">
            <button
              onClick={handleClose}
              className="w-full rounded-lg bg-brand-600 px-4 py-2.5 text-sm font-medium text-white hover:bg-brand-700 transition-colors"
            >
              Done
            </button>
          </div>
        )}
      </div>
    </div>
  );
}

// ── Sub-components ──

function AmountSelection({
  selectedAmount,
  customAmount,
  onPresetSelect,
  onCustomChange,
}: {
  selectedAmount: number;
  customAmount: string;
  onPresetSelect: (nanoerg: number) => void;
  onCustomChange: (val: string) => void;
}) {
  return (
    <div className="space-y-4">
      <p className="text-sm text-surface-800/60">
        Select an amount to add to your balance:
      </p>

      <div className="grid grid-cols-3 gap-2">
        {PRESET_AMOUNTS_NANOERG.map((preset) => (
          <button
            key={preset.nanoerg}
            onClick={() => onPresetSelect(preset.nanoerg)}
            className={cn(
              "rounded-lg border p-3 text-center transition-all",
              selectedAmount === preset.nanoerg && !customAmount
                ? "border-brand-500 bg-brand-50 text-brand-700"
                : "border-surface-200 hover:border-surface-300 text-surface-800/70",
            )}
          >
            <div className="text-sm font-semibold">{preset.label}</div>
            <div className="text-[10px] text-surface-800/40 mt-0.5">
              ~${ergToUsd(preset.nanoerg / 1e9)}
            </div>
          </button>
        ))}
      </div>

      <div>
        <label className="block text-xs font-medium text-surface-800/50 mb-1">
          Custom amount (ERG)
        </label>
        <input
          type="number"
          value={customAmount}
          onChange={(e) => onCustomChange(e.target.value)}
          placeholder="Enter ERG amount"
          min="0.01"
          step="0.01"
          className="w-full rounded-lg border border-surface-200 bg-surface-0 px-3 py-2 text-sm placeholder:text-surface-800/30 focus:outline-none focus:ring-2 focus:ring-brand-500/40 focus:border-brand-500"
        />
      </div>

      <div className="rounded-lg bg-surface-50 p-3">
        <div className="text-xs text-surface-800/40 mb-1">Selected amount</div>
        <ERGAmountDisplay amountNanoerg={selectedAmount} />
      </div>
    </div>
  );
}

function PaymentFlow({
  amountNanoerg,
  recipientAddress,
  description,
  onSuccess,
}: {
  amountNanoerg: number;
  recipientAddress: string;
  description: string;
  onSuccess: (txId: string) => void;
}) {
  return (
    <div className="space-y-4 text-center">
      <div className="mx-auto flex h-16 w-16 items-center justify-center rounded-full bg-brand-100">
        <svg xmlns="http://www.w3.org/2000/svg" width="28" height="28" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className="text-brand-600">
          <line x1="12" x2="12" y1="2" y2="22" />
          <path d="M17 5H9.5a3.5 3.5 0 0 0 0 7h5a3.5 3.5 0 0 1 0 7H6" />
        </svg>
      </div>

      <div>
        <h3 className="text-sm font-medium text-surface-900 mb-1">Payment Details</h3>
        <ERGAmountDisplay amountNanoerg={amountNanoerg} />
        <p className="text-xs text-surface-800/40 mt-1">{description}</p>
      </div>

      {/* QR Code placeholder */}
      <div className="mx-auto w-48 h-48 rounded-lg border border-surface-200 bg-surface-50 flex items-center justify-center">
        <div className="text-center">
          <svg xmlns="http://www.w3.org/2000/svg" width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1" strokeLinecap="round" strokeLinejoin="round" className="text-surface-800/20 mx-auto mb-2">
            <rect x="3" y="3" width="7" height="7" />
            <rect x="14" y="3" width="7" height="7" />
            <rect x="3" y="14" width="7" height="7" />
            <rect x="14" y="14" width="3" height="3" />
            <rect x="18" y="14" width="3" height="3" />
            <rect x="14" y="18" width="3" height="3" />
            <rect x="18" y="18" width="3" height="3" />
          </svg>
          <p className="text-xs text-surface-800/30">QR Code</p>
        </div>
      </div>

      <p className="text-xs text-surface-800/40">
        Send exactly {nanoergToErg(amountNanoerg)} ERG to the address below
      </p>
      <div className="rounded-lg bg-surface-100 px-3 py-2 text-xs font-mono text-surface-800/60 break-all">
        {recipientAddress}
      </div>

      <ERGPaymentButton
        amountNanoerg={amountNanoerg}
        recipientAddress={recipientAddress}
        description={description}
        onSuccess={onSuccess}
        className="w-full justify-center"
      >
        {nanoergToErg(amountNanoerg)} ERG (~${ergToUsd(amountNanoerg / 1e9)})
      </ERGPaymentButton>
    </div>
  );
}

function Receipt({
  txId,
  amountNanoerg,
}: {
  txId: string;
  amountNanoerg: number;
}) {
  return (
    <div className="space-y-4 text-center">
      <div className="mx-auto flex h-16 w-16 items-center justify-center rounded-full bg-green-100">
        <svg xmlns="http://www.w3.org/2000/svg" width="28" height="28" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className="text-green-600">
          <polyline points="20 6 9 17 4 12" />
        </svg>
      </div>

      <div>
        <h3 className="text-sm font-medium text-surface-900 mb-1">Payment Confirmed</h3>
        <ERGAmountDisplay amountNanoerg={amountNanoerg} />
      </div>

      {txId && (
        <div className="rounded-lg bg-surface-50 p-3">
          <div className="text-xs text-surface-800/40 mb-1">Transaction ID</div>
          <div className="text-xs font-mono text-surface-800/60 break-all">{txId}</div>
          <div className="mt-2">
            <ErgoExplorerTxLink txId={txId} />
          </div>
        </div>
      )}
    </div>
  );
}

function TransactionHistory() {
  return (
    <div className="space-y-3">
      <h3 className="text-sm font-medium text-surface-900">Recent Transactions</h3>
      {MOCK_HISTORY.map((tx) => (
        <div
          key={tx.id}
          className="flex items-center justify-between rounded-lg border border-surface-100 p-3"
        >
          <div>
            <div className="text-sm text-surface-900">{tx.description}</div>
            <div className="text-xs text-surface-800/40">{tx.date}</div>
          </div>
          <div className="text-right">
            <div className="text-sm font-medium text-surface-900">
              {nanoergToErg(tx.amountNanoerg)} ERG
            </div>
            <div className="flex items-center gap-1 justify-end">
              <span
                className={cn(
                  "h-1.5 w-1.5 rounded-full",
                  tx.status === "confirmed"
                    ? "bg-green-500"
                    : tx.status === "pending"
                      ? "bg-amber-500"
                      : "bg-red-500",
                )}
              />
              <span className="text-xs text-surface-800/40 capitalize">{tx.status}</span>
              {tx.txId && <ErgoExplorerTxLink txId={tx.txId} />}
            </div>
          </div>
        </div>
      ))}

      {MOCK_HISTORY.length === 0 && (
        <div className="text-sm text-surface-800/40 py-4 text-center">
          No transactions yet.
        </div>
      )}
    </div>
  );
}
