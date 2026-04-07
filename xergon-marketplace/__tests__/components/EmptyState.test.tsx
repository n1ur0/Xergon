import { describe, it, expect, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import { EmptyState } from "@/components/ui/EmptyState";

describe("EmptyState", () => {
  it("renders the default 'generic' type", () => {
    render(<EmptyState />);
    expect(screen.getByText("Nothing Here Yet")).toBeInTheDocument();
    expect(
      screen.getByText("No data to display. Check back later.")
    ).toBeInTheDocument();
  });

  it("renders preset type 'no-search-results'", () => {
    render(<EmptyState type="no-search-results" />);
    expect(screen.getByText("No Results Found")).toBeInTheDocument();
  });

  it("renders preset type 'no-models'", () => {
    render(<EmptyState type="no-models" />);
    expect(screen.getByText("No Models Available")).toBeInTheDocument();
  });

  it("renders preset type 'no-providers'", () => {
    render(<EmptyState type="no-providers" />);
    expect(screen.getByText("No Providers Available")).toBeInTheDocument();
  });

  it("renders custom title and description", () => {
    render(
      <EmptyState title="Custom Title" description="Custom description text" />
    );
    expect(screen.getByText("Custom Title")).toBeInTheDocument();
    expect(screen.getByText("Custom description text")).toBeInTheDocument();
  });

  it("renders the action button with label", () => {
    const onClick = vi.fn();
    render(<EmptyState action={{ label: "Click Me", onClick }} />);
    const button = screen.getByRole("button", { name: "Click Me" });
    expect(button).toBeInTheDocument();
  });

  it("calls action onClick when button is clicked", async () => {
    const onClick = vi.fn();
    const { userEvent } = await import("@testing-library/user-event");
    render(<EmptyState action={{ label: "Do Something", onClick }} />);
    await userEvent.click(screen.getByRole("button", { name: "Do Something" }));
    expect(onClick).toHaveBeenCalledOnce();
  });

  it("renders preset action button for types that have one", () => {
    render(<EmptyState type="no-search-results" />);
    expect(
      screen.getByRole("button", { name: "Clear Filters" })
    ).toBeInTheDocument();
  });

  it("renders children when provided", () => {
    render(
      <EmptyState>
        <span data-testid="child">Extra content</span>
      </EmptyState>
    );
    expect(screen.getByTestId("child")).toBeInTheDocument();
    expect(screen.getByText("Extra content")).toBeInTheDocument();
  });

  it("applies custom className", () => {
    const { container } = render(
      <EmptyState className="my-custom-class" />
    );
    expect(container.firstChild).toHaveClass("my-custom-class");
  });
});
