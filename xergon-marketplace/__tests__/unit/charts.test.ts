import { describe, it, expect } from "vitest";
import {
  generateBarPath,
  generateLinePath,
  generateAreaPath,
  generateHeatmapGrid,
  colorScaleGreen,
  colorScaleBlue,
  formatAxisValue,
  calculateYAxisTicks,
  generateSparkline,
  generateSparklineArea,
} from "@/lib/utils/charts";

describe("generateBarPath", () => {
  it("returns empty string for empty data", () => {
    expect(generateBarPath([], 100, 100)).toBe("");
  });

  it("generates a path string with M and V commands", () => {
    const path = generateBarPath([10, 20, 30], 200, 100);
    expect(path).toContain("M");
    expect(path).toContain("V");
  });

  it("handles negative values", () => {
    const path = generateBarPath([-10, 10], 200, 100);
    expect(path).toContain("M");
  });
});

describe("generateLinePath", () => {
  it("returns empty string for empty data", () => {
    expect(generateLinePath([], 100, 100)).toBe("");
  });

  it("generates comma-separated x,y points", () => {
    const path = generateLinePath([1, 2, 3], 100, 100);
    expect(path).toContain(",");
  });

  it("single data point returns a valid point", () => {
    const path = generateLinePath([50], 100, 100);
    expect(path).toContain(",");
  });
});

describe("generateAreaPath", () => {
  it("returns empty string for empty data", () => {
    expect(generateAreaPath([], 100, 100)).toBe("");
  });

  it("generates a closed path (ends with Z)", () => {
    const path = generateAreaPath([10, 20], 200, 100);
    expect(path).toContain("Z");
  });
});

describe("generateHeatmapGrid", () => {
  it("generates correct number of cells", () => {
    const data = [
      [1, 2],
      [3, 4],
    ];
    const cells = generateHeatmapGrid(data, () => "red");
    expect(cells).toHaveLength(4);
  });

  it("sets x/y positions based on cellSize and gap", () => {
    const cells = generateHeatmapGrid([[5]], () => "blue", 10, 2);
    expect(cells[0].x).toBe(0);
    expect(cells[0].y).toBe(0);
    expect(cells[0].value).toBe(5);
  });

  it("sets color to transparent for zero values", () => {
    const cells = generateHeatmapGrid([[0]], () => "blue");
    expect(cells[0].color).toBe("transparent");
  });
});

describe("colorScaleGreen", () => {
  it("returns transparent for zero", () => {
    expect(colorScaleGreen(0, 0, 10)).toBe("transparent");
  });

  it("returns rgba with green channel for positive values", () => {
    const color = colorScaleGreen(5, 0, 10);
    expect(color).toContain("rgba(34,197,94,");
  });
});

describe("colorScaleBlue", () => {
  it("returns transparent for zero", () => {
    expect(colorScaleBlue(0, 0, 10)).toBe("transparent");
  });

  it("returns rgba with blue channel for positive values", () => {
    const color = colorScaleBlue(5, 0, 10);
    expect(color).toContain("rgba(59,130,246,");
  });
});

describe("formatAxisValue", () => {
  it("formats as ERG", () => {
    expect(formatAxisValue(1.5, "erg")).toContain("ERG");
    expect(formatAxisValue(0.001, "erg")).not.toContain("ERG");
  });

  it("formats as percent", () => {
    expect(formatAxisValue(75.5, "percent")).toBe("76%");
  });

  it("formats as number with K/M/B suffixes", () => {
    expect(formatAxisValue(500, "number")).toBe("500");
    expect(formatAxisValue(1500, "number")).toBe("1.5K");
    expect(formatAxisValue(1500000, "number")).toBe("1.5M");
    expect(formatAxisValue(1500000000, "number")).toBe("1.5B");
  });
});

describe("calculateYAxisTicks", () => {
  it("returns single value when min equals max", () => {
    expect(calculateYAxisTicks(5, 5)).toEqual([5]);
  });

  it("returns evenly spaced ticks", () => {
    const ticks = calculateYAxisTicks(0, 100, 5);
    expect(ticks[0]).toBe(0);
    expect(ticks[ticks.length - 1]).toBe(100);
    expect(ticks.length).toBeGreaterThan(1);
  });
});

describe("generateSparkline", () => {
  it("returns empty string for empty data", () => {
    expect(generateSparkline([], 100, 50)).toBe("");
  });

  it("returns flat line for single data point", () => {
    const path = generateSparkline([50], 100, 50);
    expect(path).toContain("M");
    expect(path).toContain("L");
  });

  it("generates M and L path commands", () => {
    const path = generateSparkline([10, 20, 30], 100, 50);
    expect(path).toMatch(/^M/);
  });
});

describe("generateSparklineArea", () => {
  it("returns empty string for empty data", () => {
    expect(generateSparklineArea([], 100, 50)).toBe("");
  });

  it("generates a closed path", () => {
    const path = generateSparklineArea([10, 20], 100, 50);
    expect(path).toContain("Z");
    expect(path).toMatch(/^M/);
  });
});
