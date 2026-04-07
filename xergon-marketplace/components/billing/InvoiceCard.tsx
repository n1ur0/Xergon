"use client";

import { cn } from "@/lib/utils";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export type InvoiceStatus = "paid" | "pending" | "overdue" | "refunded";

export interface Invoice {
  id: string;
  amount: number; // in ERG
  date: string;
  dueDate: string;
  status: InvoiceStatus;
  description: string;
}

interface InvoiceCardProps {
  invoice: Invoice;
  onDownload?: (invoiceId: string) => void;
}

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

const STATUS_CONFIG: Record<InvoiceStatus, { label: string; className: string }> = {
  paid: {
    label: "Paid",
    className: "bg-emerald-100 text-emerald-700 dark:bg-emerald-900/30 dark:text-emerald-400",
  },
  pending: {
    label: "Pending",
    className: "bg-amber-100 text-amber-700 dark:bg-amber-900/30 dark:text-amber-400",
  },
  overdue: {
    label: "Overdue",
    className: "bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-400",
  },
  refunded: {
    label: "Refunded",
    className: "bg-blue-100 text-blue-700 dark:bg-blue-900/30 dark:text-blue-400",
  },
};

// ---------------------------------------------------------------------------
// Formatters
// ---------------------------------------------------------------------------

function formatErg(amount: number): string {
  return amount.toFixed(4);
}

function formatDate(iso: string): string {
  return new Date(iso).toLocaleDateString("en-US", {
    month: "short",
    day: "numeric",
    year: "numeric",
  });
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function InvoiceCard({ invoice, onDownload }: InvoiceCardProps) {
  const config = STATUS_CONFIG[invoice.status];

  return (
    <div className="flex items-center gap-4 rounded-xl border border-surface-200 bg-surface-0 p-4 transition-shadow hover:shadow-sm">
      {/* Status indicator */}
      <div
        className={cn(
          "flex h-10 w-10 shrink-0 items-center justify-center rounded-lg",
          invoice.status === "paid" && "bg-emerald-50 dark:bg-emerald-950/30",
          invoice.status === "pending" && "bg-amber-50 dark:bg-amber-950/30",
          invoice.status === "overdue" && "bg-red-50 dark:bg-red-950/30",
          invoice.status === "refunded" && "bg-blue-50 dark:bg-blue-950/30",
        )}
      >
        <svg
          className={cn(
            "h-5 w-5",
            invoice.status === "paid" && "text-emerald-600 dark:text-emerald-400",
            invoice.status === "pending" && "text-amber-600 dark:text-amber-400",
            invoice.status === "overdue" && "text-red-600 dark:text-red-400",
            invoice.status === "refunded" && "text-blue-600 dark:text-blue-400",
          )}
          fill="none"
          viewBox="0 0 24 24"
          strokeWidth={1.5}
          stroke="currentColor"
        >
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            d="M19.5 14.25v-2.625a3.375 3.375 0 00-3.375-3.375h-1.5A1.125 1.125 0 0113.5 7.125v-1.5a3.375 3.375 0 00-3.375-3.375H8.25m0 12.75h7.5m-7.5 3H12M10.5 2.25H5.625c-.621 0-1.125.504-1.125 1.125v17.25c0 .621.504 1.125 1.125 1.125h12.75c.621 0 1.125-.504 1.125-1.125V11.25a9 9 0 00-9-9z"
          />
        </svg>
      </div>

      {/* Content */}
      <div className="flex-1 min-w-0">
        <div className="flex items-center gap-2 mb-0.5">
          <span className="text-sm font-semibold text-surface-900">
            {formatErg(invoice.amount)} ERG
          </span>
          <span
            className={cn(
              "inline-flex items-center rounded-full px-2 py-0.5 text-[10px] font-medium",
              config.className,
            )}
          >
            {config.label}
          </span>
        </div>
        <div className="text-xs text-surface-800/40 truncate">
          {invoice.description}
        </div>
        <div className="text-[10px] text-surface-800/30 mt-0.5">
          {formatDate(invoice.date)} &middot; Due {formatDate(invoice.dueDate)}
        </div>
      </div>

      {/* Download button */}
      {onDownload && invoice.status !== "refunded" && (
        <button
          onClick={(e) => {
            e.stopPropagation();
            onDownload(invoice.id);
          }}
          className="flex items-center gap-1.5 rounded-lg border border-surface-200 bg-surface-50 px-3 py-1.5 text-xs font-medium text-surface-800/60 transition-colors hover:bg-surface-100 hover:text-surface-900"
        >
          <svg className="h-3.5 w-3.5" fill="none" viewBox="0 0 24 24" strokeWidth={2} stroke="currentColor">
            <path strokeLinecap="round" strokeLinejoin="round" d="M3 16.5v2.25A2.25 2.25 0 005.25 21h13.5A2.25 2.25 0 0021 18.75V16.5M16.5 12L12 16.5m0 0L7.5 12m4.5 4.5V3" />
          </svg>
          PDF
        </button>
      )}
    </div>
  );
}
