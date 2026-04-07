"use client";

import { useState, useCallback } from "react";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export type DatePreset = "24h" | "7d" | "30d" | "90d" | "custom";

interface DateRange {
  start: string; // ISO date string
  end: string;   // ISO date string
}

interface DateRangePickerProps {
  value: DateRange;
  onChange: (range: DateRange) => void;
  className?: string;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function applyPreset(preset: DatePreset): DateRange {
  const now = new Date();
  const end = now.toISOString().slice(0, 10);
  let start: string;

  switch (preset) {
    case "24h": {
      const yesterday = new Date(now);
      yesterday.setDate(yesterday.getDate() - 1);
      start = yesterday.toISOString().slice(0, 10);
      break;
    }
    case "7d": {
      const d = new Date(now);
      d.setDate(d.getDate() - 7);
      start = d.toISOString().slice(0, 10);
      break;
    }
    case "30d": {
      const d = new Date(now);
      d.setDate(d.getDate() - 30);
      start = d.toISOString().slice(0, 10);
      break;
    }
    case "90d": {
      const d = new Date(now);
      d.setDate(d.getDate() - 90);
      start = d.toISOString().slice(0, 10);
      break;
    }
    case "custom":
      return { start: "", end: "" };
  }

  return { start, end };
}

function inferPreset(range: DateRange): DatePreset {
  const now = new Date();
  const endMatch = range.end === now.toISOString().slice(0, 10);
  if (!endMatch) return "custom";

  const start = new Date(range.start);
  const diffDays = Math.round((now.getTime() - start.getTime()) / (1000 * 60 * 60 * 24));

  if (diffDays <= 1) return "24h";
  if (diffDays <= 7) return "7d";
  if (diffDays <= 30) return "30d";
  if (diffDays <= 90) return "90d";
  return "custom";
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function DateRangePicker({ value, onChange, className = "" }: DateRangePickerProps) {
  const [isOpen, setIsOpen] = useState(false);
  const activePreset = inferPreset(value);
  const [localRange, setLocalRange] = useState<DateRange>(value);

  const handlePreset = useCallback(
    (preset: DatePreset) => {
      const range = applyPreset(preset);
      setLocalRange(range);
      onChange(range);
    },
    [onChange]
  );

  const handleApply = () => {
    if (localRange.start && localRange.end) {
      onChange(localRange);
    }
    setIsOpen(false);
  };

  const handleReset = () => {
    const range = applyPreset("30d");
    setLocalRange(range);
    onChange(range);
    setIsOpen(false);
  };

  const presets: DatePreset[] = ["24h", "7d", "30d", "90d"];

  return (
    <div className={`relative ${className}`}>
      {/* Trigger button */}
      <button
        onClick={() => setIsOpen(!isOpen)}
        className="inline-flex items-center gap-2 rounded-lg border border-surface-200 dark:border-surface-600
                   bg-surface-0 dark:bg-surface-900 px-3 py-1.5 text-xs font-medium
                   text-surface-800/70 dark:text-surface-200/70 hover:bg-surface-50 dark:hover:bg-surface-800
                   transition-colors focus:outline-none focus-visible:ring-2 focus-visible:ring-brand-500"
        aria-label="Select date range"
        aria-expanded={isOpen}
      >
        <svg className="w-3.5 h-3.5" viewBox="0 0 20 20" fill="currentColor">
          <path fillRule="evenodd" d="M6 2a1 1 0 00-1 1v1H4a2 2 0 00-2 2v10a2 2 0 002 2h12a2 2 0 002-2V6a2 2 0 00-2-2h-1V3a1 1 0 10-2 0v1H7V3a1 1 0 00-1-1zm0 5a1 1 0 000 2h8a1 1 0 100-2H6z" clipRule="evenodd" />
        </svg>
        {activePreset === "custom"
          ? `${localRange.start} → ${localRange.end}`
          : activePreset.toUpperCase()}
      </button>

      {/* Dropdown */}
      {isOpen && (
        <div
          className="absolute top-full mt-1 right-0 z-20 w-72 rounded-xl border border-surface-200 dark:border-surface-700
                     bg-surface-0 dark:bg-surface-900 shadow-lg p-4"
          role="dialog"
          aria-label="Date range picker"
        >
          {/* Presets */}
          <div className="mb-4">
            <p className="text-xs font-semibold text-surface-900 dark:text-surface-0 mb-2">Quick Select</p>
            <div className="flex flex-wrap gap-1.5">
              {presets.map((preset) => (
                <button
                  key={preset}
                  onClick={() => handlePreset(preset)}
                  className={`rounded-lg px-3 py-1.5 text-xs font-medium transition-colors ${
                    activePreset === preset
                      ? "bg-brand-600 text-white"
                      : "bg-surface-100 dark:bg-surface-800 text-surface-800/60 dark:text-surface-200/60 hover:bg-surface-200 dark:hover:bg-surface-700"
                  }`}
                >
                  {preset.toUpperCase()}
                </button>
              ))}
            </div>
          </div>

          {/* Custom range */}
          <div className="mb-4">
            <p className="text-xs font-semibold text-surface-900 dark:text-surface-0 mb-2">Custom Range</p>
            <div className="grid grid-cols-2 gap-2">
              <div>
                <label htmlFor="drp-start" className="block text-[10px] text-surface-800/40 mb-1">Start</label>
                <input
                  id="drp-start"
                  type="date"
                  value={localRange.start}
                  onChange={(e) => setLocalRange((prev) => ({ ...prev, start: e.target.value }))}
                  className="w-full rounded-lg border border-surface-200 dark:border-surface-600 bg-surface-50 dark:bg-surface-800
                             px-2 py-1.5 text-xs text-surface-900 dark:text-surface-0 focus:outline-none focus:ring-2 focus:ring-brand-500"
                />
              </div>
              <div>
                <label htmlFor="drp-end" className="block text-[10px] text-surface-800/40 mb-1">End</label>
                <input
                  id="drp-end"
                  type="date"
                  value={localRange.end}
                  onChange={(e) => setLocalRange((prev) => ({ ...prev, end: e.target.value }))}
                  className="w-full rounded-lg border border-surface-200 dark:border-surface-600 bg-surface-50 dark:bg-surface-800
                             px-2 py-1.5 text-xs text-surface-900 dark:text-surface-0 focus:outline-none focus:ring-2 focus:ring-brand-500"
                />
              </div>
            </div>
          </div>

          {/* Actions */}
          <div className="flex items-center gap-2">
            <button
              onClick={handleReset}
              className="flex-1 rounded-lg border border-surface-200 dark:border-surface-600 px-3 py-1.5 text-xs font-medium
                         text-surface-800/60 dark:text-surface-200/60 hover:bg-surface-50 dark:hover:bg-surface-800 transition-colors"
            >
              Reset
            </button>
            <button
              onClick={handleApply}
              className="flex-1 rounded-lg bg-brand-600 text-white px-3 py-1.5 text-xs font-medium
                         hover:bg-brand-500 transition-colors"
            >
              Apply
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
