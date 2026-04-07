import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import { SkeletonCardGrid } from "@/components/ui/SkeletonCard";
import { PageSkeleton } from "@/components/ui/PageSkeleton";
import { SuspenseWrap } from "@/components/ui/SuspenseWrap";

describe("SkeletonCardGrid", () => {
  it("renders the specified number of skeleton cards", () => {
    const { container } = render(<SkeletonCardGrid count={3} />);
    // Skeleton cards use skeleton-shimmer class
    const cards = container.querySelectorAll("[class*='skeleton-shimmer']");
    expect(cards.length).toBeGreaterThanOrEqual(3);
  });

  it("renders at least one element", () => {
    const { container } = render(<SkeletonCardGrid count={1} />);
    expect(container.children.length).toBeGreaterThanOrEqual(1);
  });
});

describe("PageSkeleton", () => {
  it("renders a skeleton container", () => {
    const { container } = render(<PageSkeleton />);
    expect(container.children.length).toBeGreaterThanOrEqual(1);
  });
});

describe("SuspenseWrap", () => {
  it("renders children directly", () => {
    render(
      <SuspenseWrap fallback={<div>Loading...</div>}>
        <div data-testid="content">Hello World</div>
      </SuspenseWrap>
    );
    expect(screen.getByTestId("content")).toBeInTheDocument();
    expect(screen.getByText("Hello World")).toBeInTheDocument();
  });

  it("renders without crashing when no children", () => {
    const { container } = render(
      <SuspenseWrap fallback={<div>Loading...</div>}>{null as unknown as React.ReactNode}</SuspenseWrap>
    );
    expect(container).toBeInTheDocument();
  });
});
