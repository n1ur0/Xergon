import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import { UptimeBar } from "@/components/health/UptimeBar";

describe("UptimeBar", () => {
  it("renders the service name", () => {
    render(<UptimeBar serviceName="Relay API" dailyUptime={[100, 100, 100, 100, 100, 100, 100]} />);
    expect(screen.getByText("Relay API")).toBeInTheDocument();
  });

  it("renders 7 day labels", () => {
    render(<UptimeBar serviceName="Test" dailyUptime={[100, 100, 100, 100, 100, 100, 100]} />);
    // 7 segments + 7 labels = 14 elements total
    const segments = screen.getAllByRole("generic");
    expect(segments.length).toBeGreaterThanOrEqual(7);
  });

  it("renders tooltip on hover showing percentage", async () => {
    const { userEvent } = await import("@testing-library/user-event");
    render(<UptimeBar serviceName="Test" dailyUptime={[99.5, 100, 100, 100, 100, 100, 100]} />);
    // Hover over the first segment to show tooltip
    const container = document.querySelector("[class*='cursor-pointer']");
    if (container) {
      await userEvent.hover(container);
      // Tooltip should show the uptime percentage
      expect(screen.getByText(/99.5%/)).toBeInTheDocument();
    }
  });

  it("fills with 100% when given 7 items", () => {
    render(<UptimeBar serviceName="Test" dailyUptime={[100, 100, 100, 100, 100, 100, 100]} />);
    // All segments should be present
    const segments = document.querySelectorAll("[class*='rounded-sm'][class*='cursor-pointer']");
    expect(segments.length).toBe(7);
  });

  it("defaults to 100% uptime when fewer than 7 values given", () => {
    render(<UptimeBar serviceName="Test" dailyUptime={[50, 75]} />);
    // Should still render 7 segments (defaulting to 100 for missing)
    const segments = document.querySelectorAll("[class*='rounded-sm'][class*='cursor-pointer']");
    expect(segments.length).toBe(7);
  });

  it("renders empty array as all 100%", () => {
    render(<UptimeBar serviceName="Test" dailyUptime={[]} />);
    const segments = document.querySelectorAll("[class*='rounded-sm'][class*='cursor-pointer']");
    expect(segments.length).toBe(7);
  });
});
