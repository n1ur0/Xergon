"use client";

import { type OnChainTransaction } from "@/lib/api/transactions";
import { formatNanoerg, timeAgo } from "@/lib/api/transactions";

interface TxExportProps {
  transactions: OnChainTransaction[];
}

function buildCsv(transactions: OnChainTransaction[]): string {
  const headers = [
    "Date",
    "Tx ID",
    "Type",
    "Amount (ERG)",
    "Status",
    "Confirmations",
    "Block Height",
    "Model",
    "Counterpart",
    "Description",
  ];

  const rows = transactions.map((tx) => [
    new Date(tx.timestamp).toISOString(),
    tx.txId,
    tx.type,
    tx.amountErg.toFixed(9),
    tx.status,
    String(tx.confirmations),
    String(tx.blockHeight),
    tx.model ?? "",
    tx.counterpart ?? "",
    tx.description,
  ]);

  const lines = [headers, ...rows]
    .map((row) => row.map((cell) => `"${String(cell).replace(/"/g, '""')}"`).join(","))
    .join("\n");

  return lines;
}

export function TxExport({ transactions }: TxExportProps) {
  if (transactions.length === 0) return null;

  const handleExport = () => {
    const csv = buildCsv(transactions);
    const blob = new Blob([csv], { type: "text/csv;charset=utf-8;" });
    const url = URL.createObjectURL(blob);
    const link = document.createElement("a");
    link.href = url;
    link.download = `xergon-transactions-${new Date().toISOString().slice(0, 10)}.csv`;
    document.body.appendChild(link);
    link.click();
    document.body.removeChild(link);
    URL.revokeObjectURL(url);
  };

  return (
    <button
      onClick={handleExport}
      title="Export transactions as CSV"
      className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium text-surface-800/60 hover:text-surface-900 bg-surface-100 hover:bg-surface-200 border border-surface-200 transition-colors"
    >
      <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
        <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" />
        <polyline points="7 10 12 15 17 10" />
        <line x1="12" y1="15" x2="12" y2="3" />
      </svg>
      Export CSV
    </button>
  );
}
