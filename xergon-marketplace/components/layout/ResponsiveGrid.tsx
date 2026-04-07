"use client";

import { useEffect, useRef, useState, type ReactNode } from "react";
import { cn } from "@/lib/utils";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface ResponsiveGridProps {
  children: ReactNode;
  /** Minimum width per item before wrapping to next row (default: 280px) */
  minItemWidth?: number;
  /** Gap between items in px (default: 16) */
  gap?: number;
  /** Maximum number of columns (default: 4) */
  maxColumns?: number;
  /** Additional class names */
  className?: string;
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function ResponsiveGrid({
  children,
  minItemWidth = 280,
  gap = 16,
  maxColumns = 4,
  className,
}: ResponsiveGridProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const [columns, setColumns] = useState(1);

  useEffect(() => {
    function updateColumns() {
      if (!containerRef.current) return;
      const containerWidth = containerRef.current.clientWidth;

      // Calculate max columns that fit
      const totalGap = gap * (maxColumns - 1);
      const itemWidth = (containerWidth - totalGap) / maxColumns;

      if (itemWidth >= minItemWidth) {
        setColumns(maxColumns);
        return;
      }

      // Find the best column count
      for (let cols = maxColumns; cols >= 1; cols--) {
        const gapSpace = gap * (cols - 1);
        const possibleWidth = (containerWidth - gapSpace) / cols;
        if (possibleWidth >= minItemWidth) {
          setColumns(cols);
          return;
        }
      }

      setColumns(1);
    }

    updateColumns();
    window.addEventListener("resize", updateColumns);
    return () => window.removeEventListener("resize", updateColumns);
  }, [minItemWidth, gap, maxColumns]);

  const gridStyle: React.CSSProperties = {
    display: "grid",
    gridTemplateColumns: `repeat(${columns}, minmax(0, 1fr))`,
    gap: `${gap}px`,
  };

  return (
    <div ref={containerRef} style={gridStyle} className={cn("w-full", className)}>
      {children}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Alternative: CSS Grid-based responsive grid using Tailwind classes
// ---------------------------------------------------------------------------

interface AutoGridProps {
  children: ReactNode;
  /** Minimum item width breakpoint classes: "sm" (200px), "md" (280px), "lg" (360px) */
  minSize?: "sm" | "md" | "lg" | "xl";
  /** Gap size */
  gapSize?: "none" | "sm" | "md" | "lg" | "xl";
  /** Additional class names */
  className?: string;
}

const MIN_SIZE_COLS: Record<string, string> = {
  sm: "grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4",
  md: "grid-cols-1 sm:grid-cols-1 md:grid-cols-2 lg:grid-cols-3",
  lg: "grid-cols-1 md:grid-cols-2 lg:grid-cols-2 xl:grid-cols-3",
  xl: "grid-cols-1 md:grid-cols-1 lg:grid-cols-2",
};

const GAP_CLASSES: Record<string, string> = {
  none: "gap-0",
  sm: "gap-2",
  md: "gap-4",
  lg: "gap-6",
  xl: "gap-8",
};

/**
 * CSS Grid-based responsive grid using Tailwind breakpoints.
 * Lighter weight than ResponsiveGrid (no JS resize observer needed).
 */
export function AutoGrid({
  children,
  minSize = "md",
  gapSize = "md",
  className,
}: AutoGridProps) {
  return (
    <div className={cn("grid w-full", MIN_SIZE_COLS[minSize], GAP_CLASSES[gapSize], className)}>
      {children}
    </div>
  );
}
