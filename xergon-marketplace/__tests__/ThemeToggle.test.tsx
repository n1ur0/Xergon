import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { ThemeToggle } from "@/components/ThemeToggle";

// ── Mocks ────────────────────────────────────────────────────────────────

const mockToggleTheme = vi.fn();

let mockThemeState: {
  theme: "light" | "dark" | "system";
  resolvedTheme: "light" | "dark";
  toggleTheme: () => void;
} = {
  theme: "system",
  resolvedTheme: "dark",
  toggleTheme: mockToggleTheme,
};

vi.mock("@/lib/stores/theme", () => ({
  useThemeStore: (selector: (s: typeof mockThemeState) => unknown) =>
    selector(mockThemeState),
}));

// Mock lucide-react icons
vi.mock("lucide-react", () => ({
  Sun: () => <svg data-testid="sun-icon" />,
  Moon: () => <svg data-testid="moon-icon" />,
  Monitor: () => <svg data-testid="monitor-icon" />,
}));

describe("ThemeToggle", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    // Reset to default state
    mockThemeState = {
      theme: "system",
      resolvedTheme: "dark",
      toggleTheme: mockToggleTheme,
    };
  });

  it("renders a toggle button", () => {
    render(<ThemeToggle />);
    const button = screen.getByRole("button");
    expect(button).toBeInTheDocument();
  });

  it("shows the system icon when theme is 'system'", () => {
    mockThemeState.theme = "system";
    mockThemeState.resolvedTheme = "dark";
    render(<ThemeToggle />);
    expect(screen.getByTestId("monitor-icon")).toBeInTheDocument();
  });

  it("shows the sun icon when theme is 'light'", () => {
    mockThemeState.theme = "light";
    mockThemeState.resolvedTheme = "light";
    render(<ThemeToggle />);
    expect(screen.getByTestId("sun-icon")).toBeInTheDocument();
  });

  it("shows the moon icon when theme is 'dark'", () => {
    mockThemeState.theme = "dark";
    mockThemeState.resolvedTheme = "dark";
    render(<ThemeToggle />);
    expect(screen.getByTestId("moon-icon")).toBeInTheDocument();
  });

  it("calls toggleTheme when clicked", async () => {
    const user = userEvent.setup();
    render(<ThemeToggle />);
    const button = screen.getByRole("button");
    await user.click(button);
    expect(mockToggleTheme).toHaveBeenCalledOnce();
  });

  it("has an accessible label describing current theme", () => {
    mockThemeState.theme = "dark";
    mockThemeState.resolvedTheme = "dark";
    render(<ThemeToggle />);
    const button = screen.getByRole("button");
    expect(button).toHaveAttribute("aria-label", "Dark theme (Dark) — click to switch");
  });

  it("shows correct label for system theme", () => {
    mockThemeState.theme = "system";
    mockThemeState.resolvedTheme = "light";
    render(<ThemeToggle />);
    const button = screen.getByRole("button");
    expect(button).toHaveAttribute("aria-label", "System theme (Light) — click to switch");
  });
});
