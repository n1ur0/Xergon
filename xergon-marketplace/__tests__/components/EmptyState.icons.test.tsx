import { describe, it, expect, vi } from "vitest";
import { render, screen } from "@testing-library/react";

// Mock lucide-react icons
vi.mock("lucide-react", () => ({
  Cpu: ({ className }: { className?: string }) => (
    <svg data-testid="icon-cpu" className={className} />
  ),
  SearchX: ({ className }: { className?: string }) => (
    <svg data-testid="icon-searchx" className={className} />
  ),
  Inbox: ({ className }: { className?: string }) => (
    <svg data-testid="icon-inbox" className={className} />
  ),
  Monitor: ({ className }: { className?: string }) => (
    <svg data-testid="icon-monitor" className={className} />
  ),
  Server: ({ className }: { className?: string }) => (
    <svg data-testid="icon-server" className={className} />
  ),
  LucideIcon: ({ className }: { className?: string }) => (
    <svg className={className} />
  ),
}));

import { EmptyState } from "@/components/ui/EmptyState";

describe("EmptyState (with lucide mock)", () => {
  it("renders the icon container", () => {
    render(<EmptyState type="generic" />);
    const iconContainer = document.querySelector("[class*='rounded-full']");
    expect(iconContainer).toBeInTheDocument();
  });

  it("renders different icon based on type", () => {
    const { rerender } = render(<EmptyState type="no-providers" />);
    expect(screen.getByTestId("icon-server")).toBeInTheDocument();

    rerender(<EmptyState type="no-models" />);
    expect(screen.getByTestId("icon-monitor")).toBeInTheDocument();

    rerender(<EmptyState type="no-search-results" />);
    expect(screen.getByTestId("icon-searchx")).toBeInTheDocument();

    rerender(<EmptyState type="no-rentals" />);
    expect(screen.getByTestId("icon-inbox")).toBeInTheDocument();
  });
});
