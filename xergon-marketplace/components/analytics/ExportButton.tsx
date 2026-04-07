"use client";

import { useCallback, useState } from "react";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export type ExportFormat = "csv" | "json";

interface ExportButtonProps {
  /** Data to export (array of objects) */
  data: Record<string, unknown>[];
  /** Filename without extension */
  filename: string;
  /** Available export formats */
  formats?: ExportFormat[];
  className?: string;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function downloadBlob(blob: Blob, filename: string) {
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  document.body.removeChild(a);
  URL.revokeObjectURL(url);
}

function toCSV(data: Record<string, unknown>[]): string {
  if (data.length === 0) return "";
  const headers = Object.keys(data[0]);
  const escape = (val: unknown): string => {
    const s = String(val ?? "");
    if (s.includes(",") || s.includes('"') || s.includes("\n")) {
      return `"${s.replace(/"/g, '""')}"`;
    }
    return s;
  };
  const rows = data.map((row) => headers.map((h) => escape(row[h])).join(","));
  return [headers.join(","), ...rows].join("\n");
}

function toJSON(data: Record<string, unknown>[]): string {
  return JSON.stringify(data, null, 2);
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function ExportButton({
  data,
  filename,
  formats = ["csv", "json"],
  className = "",
}: ExportButtonProps) {
  const [isOpen, setIsOpen] = useState(false);

  const handleExport = useCallback(
    (format: ExportFormat) => {
      if (data.length === 0) return;

      const content = format === "csv" ? toCSV(data) : toJSON(data);
      const mime = format === "csv" ? "text/csv;charset=utf-8" : "application/json;charset=utf-8";
      const blob = new Blob([content], { type: mime });
      downloadBlob(blob, `${filename}.${format}`);
      setIsOpen(false);
    },
    [data, filename]
  );

  return (
    <div className={`relative ${className}`}>
      <button
        onClick={() => setIsOpen(!isOpen)}
        className="inline-flex items-center gap-1.5 rounded-lg border border-surface-200 dark:border-surface-600
                   bg-surface-0 dark:bg-surface-900 px-3 py-1.5 text-xs font-medium
                   text-surface-800/70 dark:text-surface-200/70 hover:bg-surface-50 dark:hover:bg-surface-800
                   transition-colors focus:outline-none focus-visible:ring-2 focus-visible:ring-brand-500"
        aria-label="Export data"
        aria-expanded={isOpen}
        aria-haspopup="true"
      >
        <svg className="w-3.5 h-3.5" viewBox="0 0 20 20" fill="currentColor">
          <path fillRule="evenodd" d="M3 17a1 1 0 011-1h12a1 1 0 110 2H4a1 1 0 01-1-1zm3.293-7.707a1 1 0 011.414 0L9 10.586V3a1 1 0 112 0v7.586l1.293-1.293a1 1 0 111.414 1.414l-3 3a1 1 0 01-1.414 0l-3-3a1 1 0 010-1.414z" clipRule="evenodd" />
        </svg>
        Export
      </button>

      {isOpen && (
        <div
          className="absolute top-full mt-1 right-0 z-20 rounded-lg border border-surface-200 dark:border-surface-700
                     bg-surface-0 dark:bg-surface-900 shadow-lg py-1 min-w-[120px]"
          role="menu"
        >
          {formats.map((format) => (
            <button
              key={format}
              onClick={() => handleExport(format)}
              className="w-full px-3 py-1.5 text-xs font-medium text-surface-800/70 dark:text-surface-200/70
                         hover:bg-surface-50 dark:hover:bg-surface-800 transition-colors text-left uppercase"
              role="menuitem"
            >
              {format}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
