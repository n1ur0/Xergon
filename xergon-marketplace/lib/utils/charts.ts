/**
 * Pure utility functions for SVG chart rendering.
 * Zero external dependencies — produces SVG path strings, point arrays, and metadata.
 */

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface HeatmapCell {
  x: number;
  y: number;
  value: number;
  color: string;
}

export interface ChartPadding {
  top: number;
  right: number;
  bottom: number;
  left: number;
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

function clamp(v: number, lo: number, hi: number): number {
  return Math.max(lo, Math.min(hi, v));
}

function niceNum(range: number, round: boolean): number {
  const exp = Math.floor(Math.log10(Math.max(range, 1e-10)));
  const frac = range / Math.pow(10, exp);
  let nice: number;
  if (round) {
    if (frac < 1.5) nice = 1;
    else if (frac < 3) nice = 2;
    else if (frac < 7) nice = 5;
    else nice = 10;
  } else {
    if (frac <= 1) nice = 1;
    else if (frac <= 2) nice = 2;
    else if (frac <= 5) nice = 5;
    else nice = 10;
  }
  return nice * Math.pow(10, exp);
}

function defaultPadding(): ChartPadding {
  return { top: 20, right: 20, bottom: 40, left: 60 };
}

function mapData(
  data: number[],
  width: number,
  height: number,
  padding: ChartPadding,
): { x: number; y: number }[] {
  const plotW = width - padding.left - padding.right;
  const plotH = height - padding.top - padding.bottom;
  const maxVal = Math.max(...data, 1);
  const minVal = Math.min(...data, 0);
  const range = maxVal - minVal || 1;

  return data.map((v, i) => ({
    x: padding.left + (data.length > 1 ? (i / (data.length - 1)) * plotW : plotW / 2),
    y: padding.top + plotH - ((v - minVal) / range) * plotH,
  }));
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/**
 * Generate SVG `<path>` d attribute for a bar chart.
 * Each bar is a rounded rectangle; bars are evenly spaced across the plot area.
 */
export function generateBarPath(
  data: number[],
  width: number,
  height: number,
  padding: ChartPadding = defaultPadding(),
  cornerRadius = 3,
): string {
  if (data.length === 0) return "";

  const plotW = width - padding.left - padding.right;
  const plotH = height - padding.top - padding.bottom;
  const maxVal = Math.max(...data, 1);
  const minVal = Math.min(...data, 0);
  const range = maxVal - minVal || 1;
  const baseline = padding.top + plotH - ((0 - minVal) / range) * plotH;
  const barGap = Math.max(1, Math.round(plotW / data.length * 0.2));
  const barWidth = Math.max(2, (plotW - barGap * (data.length + 1)) / data.length);
  const r = Math.min(cornerRadius, barWidth / 2, Math.abs(baseline - padding.top));

  const parts: string[] = [];

  for (let i = 0; i < data.length; i++) {
    const x = padding.left + barGap + i * (barWidth + barGap);
    const val = data[i];
    const barH = ((Math.abs(val) / range) * plotH);
    const y = val >= 0 ? baseline - barH : baseline;
    const h = Math.max(barH, 1);

    // Top-rounded rect (if positive value, round top corners; negative, round bottom)
    if (val >= 0) {
      parts.push(
        `M${x},${baseline} V${y + r} Q${x},${y} ${x + r},${y} H${x + barWidth - r} Q${x + barWidth},${y} ${x + barWidth},${y + r} V${baseline} Z`,
      );
    } else {
      parts.push(
        `M${x},${baseline} V${y + h - r} Q${x},${y + h} ${x + r},${y + h} H${x + barWidth - r} Q${x + barWidth},${y + h} ${x + barWidth},${y + h - r} V${baseline} Z`,
      );
    }
  }

  return parts.join(" ");
}

/**
 * Generate SVG polyline `points` attribute for a line chart.
 */
export function generateLinePath(
  data: number[],
  width: number,
  height: number,
  padding: ChartPadding = defaultPadding(),
): string {
  const pts = mapData(data, width, height, padding);
  if (pts.length === 0) return "";
  return pts.map((p) => `${p.x.toFixed(1)},${p.y.toFixed(1)}`).join(" ");
}

/**
 * Generate SVG filled area path (line chart + filled area below).
 */
export function generateAreaPath(
  data: number[],
  width: number,
  height: number,
  padding: ChartPadding = defaultPadding(),
): string {
  const pts = mapData(data, width, height, padding);
  if (pts.length === 0) return "";

  const plotH = height - padding.top - padding.bottom;
  const baseline = padding.top + plotH;

  let d = `M${pts[0].x.toFixed(1)},${baseline.toFixed(1)}`;
  d += ` L${pts[0].x.toFixed(1)},${pts[0].y.toFixed(1)}`;

  for (let i = 1; i < pts.length; i++) {
    d += ` L${pts[i].x.toFixed(1)},${pts[i].y.toFixed(1)}`;
  }

  d += ` L${pts[pts.length - 1].x.toFixed(1)},${baseline.toFixed(1)} Z`;
  return d;
}

/**
 * Generate a heatmap grid from a 2D data array.
 * Returns individual cells with x/y position, value, and computed color.
 */
export function generateHeatmapGrid(
  data: number[][],
  colorScale: (value: number, min: number, max: number) => string,
  cellSize = 12,
  gap = 2,
): HeatmapCell[] {
  const cells: HeatmapCell[] = [];
  const flat = data.flat();
  const min = Math.min(...flat, 0);
  const max = Math.max(...flat, 1);

  for (let row = 0; row < data.length; row++) {
    for (let col = 0; col < data[row].length; col++) {
      const value = data[row][col];
      cells.push({
        x: col * (cellSize + gap),
        y: row * (cellSize + gap),
        value,
        color: value === 0 ? "transparent" : colorScale(value, min, max),
      });
    }
  }

  return cells;
}

/**
 * Green intensity color scale (0..1 opacity based on value between min/max).
 */
export function colorScaleGreen(
  value: number,
  min: number,
  max: number,
): string {
  if (value === 0) return "transparent";
  const t = clamp((value - min) / (max - min || 1), 0, 1);
  const opacity = (0.15 + t * 0.85).toFixed(2);
  return `rgba(34,197,94,${opacity})`;
}

/**
 * Blue intensity color scale.
 */
export function colorScaleBlue(
  value: number,
  min: number,
  max: number,
): string {
  if (value === 0) return "transparent";
  const t = clamp((value - min) / (max - min || 1), 0, 1);
  const opacity = (0.15 + t * 0.85).toFixed(2);
  return `rgba(59,130,246,${opacity})`;
}

/**
 * Format an axis value for display.
 */
export function formatAxisValue(
  value: number,
  type: "number" | "erg" | "percent" = "number",
): string {
  if (type === "erg") {
    if (value >= 1) return `${value.toFixed(2)} ERG`;
    if (value >= 0.001) return `${value.toFixed(4)}`;
    if (value > 0) return `${value.toFixed(6)}`;
    return "0";
  }
  if (type === "percent") {
    return `${value.toFixed(0)}%`;
  }
  // number
  if (value >= 1_000_000_000) return `${(value / 1_000_000_000).toFixed(1)}B`;
  if (value >= 1_000_000) return `${(value / 1_000_000).toFixed(1)}M`;
  if (value >= 1_000) return `${(value / 1_000).toFixed(1)}K`;
  return value.toFixed(0);
}

/**
 * Calculate nice Y-axis tick values between min and max.
 */
export function calculateYAxisTicks(
  min: number,
  max: number,
  count: number = 5,
): number[] {
  if (max === min) return [min];
  const range = niceNum(max - min, false);
  const tickSpacing = niceNum(range / (count - 1), true);
  const niceMin = Math.floor(min / tickSpacing) * tickSpacing;
  const niceMax = Math.ceil(max / tickSpacing) * tickSpacing;

  const ticks: number[] = [];
  for (let v = niceMin; v <= niceMax + tickSpacing * 0.5; v += tickSpacing) {
    ticks.push(Math.round(v * 1e10) / 1e10); // avoid floating point drift
  }
  return ticks;
}

/**
 * Generate a tiny sparkline SVG path string (minimal footprint, no padding).
 * Returns just the `d` attribute value for a polyline-style path.
 */
export function generateSparkline(
  data: number[],
  width: number,
  height: number,
): string {
  if (data.length === 0) return "";
  if (data.length === 1) {
    const y = height / 2;
    return `M0,${y} L${width},${y}`;
  }

  const maxVal = Math.max(...data);
  const minVal = Math.min(...data);
  const range = maxVal - minVal || 1;
  const padY = height * 0.1;
  const usableH = height - padY * 2;

  const points = data.map((v, i) => {
    const x = (i / (data.length - 1)) * width;
    const y = padY + usableH - ((v - minVal) / range) * usableH;
    return `${x.toFixed(1)},${y.toFixed(1)}`;
  });

  return `M${points.join(" L")}`;
}

/**
 * Generate a filled sparkline area path (for use with `<path>` and a fill color).
 */
export function generateSparklineArea(
  data: number[],
  width: number,
  height: number,
): string {
  if (data.length === 0) return "";

  const maxVal = Math.max(...data);
  const minVal = Math.min(...data);
  const range = maxVal - minVal || 1;
  const padY = height * 0.1;
  const usableH = height - padY * 2;

  const pts = data.map((v, i) => ({
    x: (i / (data.length - 1)) * width,
    y: padY + usableH - ((v - minVal) / range) * usableH,
  }));

  let d = `M0,${height} L${pts[0].x.toFixed(1)},${pts[0].y.toFixed(1)}`;
  for (let i = 1; i < pts.length; i++) {
    d += ` L${pts[i].x.toFixed(1)},${pts[i].y.toFixed(1)}`;
  }
  d += ` L${width},${height} Z`;
  return d;
}
