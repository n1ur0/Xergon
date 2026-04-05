"use client";

import { useState, useRef, useCallback, useEffect } from "react";
import { usePlaygroundStore } from "@/lib/stores/playground";
import { cn } from "@/lib/utils";
import { Search, ChevronUp } from "lucide-react";

interface ModelSelectorProps {
  models: { id: string; name: string }[];
}

export function ModelSelector({ models }: ModelSelectorProps) {
  const selectedModel = usePlaygroundStore((s) => s.selectedModel);
  const setModel = usePlaygroundStore((s) => s.setModel);
  const [mobileOpen, setMobileOpen] = useState(false);
  const [search, setSearch] = useState("");
  const sheetRef = useRef<HTMLDivElement>(null);
  const startY = useRef<number | null>(null);
  const currentY = useRef<number | null>(null);

  const filteredModels = models.filter((m) =>
    m.name.toLowerCase().includes(search.toLowerCase()),
  );

  const selectedModelObj = models.find((m) => m.id === selectedModel);

  const handleSelect = useCallback(
    (id: string) => {
      setModel(id);
      setMobileOpen(false);
      setSearch("");
    },
    [setModel],
  );

  // Swipe down to close
  const handleTouchStart = useCallback((e: React.TouchEvent) => {
    startY.current = e.touches[0].clientY;
    currentY.current = e.touches[0].clientY;
  }, []);

  const handleTouchMove = useCallback((e: React.TouchEvent) => {
    currentY.current = e.touches[0].clientY;
    if (sheetRef.current && startY.current !== null) {
      const diff = currentY.current - startY.current;
      if (diff > 0) {
        sheetRef.current.style.transform = `translateY(${diff}px)`;
      }
    }
  }, []);

  const handleTouchEnd = useCallback(() => {
    if (sheetRef.current) {
      sheetRef.current.style.transform = "";
      if (startY.current !== null && currentY.current !== null) {
        const diff = currentY.current - startY.current;
        if (diff > 100) {
          setMobileOpen(false);
          setSearch("");
        }
      }
    }
    startY.current = null;
    currentY.current = null;
  }, []);

  // Lock body scroll when sheet is open
  useEffect(() => {
    if (mobileOpen) {
      document.body.style.overflow = "hidden";
    } else {
      document.body.style.overflow = "";
    }
    return () => {
      document.body.style.overflow = "";
    };
  }, [mobileOpen]);

  return (
    <>
      {/* Mobile: chip/pill trigger */}
      <button
        type="button"
        onClick={() => setMobileOpen(true)}
        className={cn(
          "md:hidden flex items-center gap-1.5 rounded-full border px-3 py-2 text-sm font-medium transition-colors min-h-[44px]",
          "border-surface-200 bg-surface-0 text-surface-800/70 hover:bg-surface-50 active:bg-surface-100",
        )}
      >
        <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
          <path d="M12 2a3 3 0 0 0-3 3v7a3 3 0 0 0 6 0V5a3 3 0 0 0-3-3Z" />
          <path d="M19 10v2a7 7 0 0 1-14 0v-2" />
          <line x1="12" x2="12" y1="19" y2="22" />
        </svg>
        <span className="max-w-[140px] truncate">
          {selectedModelObj?.name ?? "Select model"}
        </span>
        <ChevronUp className="w-3.5 h-3.5 text-surface-800/40" />
      </button>

      {/* Desktop: standard dropdown */}
      <select
        value={selectedModel}
        onChange={(e) => setModel(e.target.value)}
        className={cn(
          "hidden md:block rounded-lg border border-surface-200 bg-surface-0 px-3 py-2 text-sm",
          "focus:outline-none focus:ring-2 focus:ring-brand-500/40 focus:border-brand-500",
          "transition-shadow",
        )}
      >
        <option value="" disabled>
          Select a model...
        </option>
        {models.map((m) => (
          <option key={m.id} value={m.id}>
            {m.name}
          </option>
        ))}
      </select>

      {/* Mobile bottom sheet */}
      {/* Backdrop */}
      <div
        className={`fixed inset-0 z-[80] bg-black/40 backdrop-blur-sm transition-opacity duration-300 md:hidden ${
          mobileOpen ? "opacity-100" : "opacity-0 pointer-events-none"
        }`}
        onClick={() => {
          setMobileOpen(false);
          setSearch("");
        }}
        aria-hidden="true"
      />

      {/* Sheet panel */}
      <div
        ref={sheetRef}
        role="dialog"
        aria-modal={mobileOpen}
        aria-label="Select a model"
        className={cn(
          "fixed bottom-0 left-0 right-0 z-[90] rounded-t-2xl bg-surface-0 shadow-2xl transition-transform duration-300 ease-out md:hidden",
          mobileOpen ? "translate-y-0" : "translate-y-full",
        )}
        style={{ paddingBottom: "env(safe-area-inset-bottom)" }}
        onTouchStart={handleTouchStart}
        onTouchMove={handleTouchMove}
        onTouchEnd={handleTouchEnd}
      >
        {/* Drag handle */}
        <div className="flex justify-center pt-3 pb-2">
          <div className="h-1 w-10 rounded-full bg-surface-300" />
        </div>

        {/* Sheet header */}
        <div className="px-4 pb-3 border-b border-surface-100">
          <h3 className="text-sm font-semibold text-surface-900 mb-2">Select a Model</h3>
          {/* Search input */}
          <div className="relative">
            <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-surface-800/40" />
            <input
              type="text"
              placeholder="Search models..."
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              className="w-full rounded-lg border border-surface-200 bg-surface-50 pl-9 pr-3 py-2.5 text-sm text-surface-900 placeholder:text-surface-800/40 focus:outline-none focus:ring-2 focus:ring-brand-500/30 focus:border-brand-500 min-h-[44px]"
              autoFocus={mobileOpen}
            />
          </div>
        </div>

        {/* Model list */}
        <div className="overflow-y-auto max-h-[50vh] px-2 py-2" style={{ paddingBottom: "calc(env(safe-area-inset-bottom) + 0.5rem)" }}>
          {filteredModels.length === 0 && (
            <div className="py-8 text-center text-sm text-surface-800/40">
              No models found
            </div>
          )}
          {filteredModels.map((m) => (
            <button
              key={m.id}
              type="button"
              onClick={() => handleSelect(m.id)}
              className={cn(
                "flex w-full items-center gap-3 rounded-lg px-3 py-3 text-left text-sm transition-colors min-h-[48px]",
                m.id === selectedModel
                  ? "bg-brand-50 text-brand-700 font-medium"
                  : "text-surface-800/70 hover:bg-surface-50 active:bg-surface-100",
              )}
            >
              <span className="truncate">{m.name}</span>
              {m.id === selectedModel && (
                <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round" className="ml-auto flex-shrink-0 text-brand-600">
                  <path d="M20 6 9 17l-5-5" />
                </svg>
              )}
            </button>
          ))}
        </div>
      </div>
    </>
  );
}
