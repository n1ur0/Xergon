import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Navbar } from "@/components/Navbar";

// ── Mocks ────────────────────────────────────────────────────────────────

// Mock next/link to render as a simple anchor
vi.mock("next/link", () => ({
  default: ({
    href,
    children,
    ...props
  }: {
    href: string;
    children: React.ReactNode;
    [key: string]: unknown;
  }) => (
    <a href={href} {...props}>
      {children}
    </a>
  ),
}));

// Mock next/navigation
const mockPush = vi.fn();
vi.mock("next/navigation", () => ({
  useRouter: () => ({ push: mockPush, refresh: vi.fn(), back: vi.fn(), forward: vi.fn() }),
  usePathname: () => "/playground",
}));

// Mock auth store (unauthenticated by default)
const mockSignOut = vi.fn();
vi.mock("@/lib/stores/auth", () => ({
  useAuthStore: (selector: (s: Record<string, unknown>) => unknown) =>
    selector({
      isAuthenticated: false,
      user: null,
      signOut: mockSignOut,
    }),
}));

// Mock chain balance hook
vi.mock("@/lib/hooks/use-chain-data", () => ({
  useChainBalance: () => ({
    balanceErg: 0,
    stakingBoxesCount: 0,
    isLoading: false,
    error: null,
    refresh: vi.fn(),
    sufficient: true,
  }),
}));

// Mock ThemeToggle
vi.mock("@/components/ThemeToggle", () => ({
  ThemeToggle: () => <button aria-label="Theme toggle">ThemeToggle</button>,
}));

// Mock LanguageSwitcher
vi.mock("@/components/LanguageSwitcher", () => ({
  LanguageSwitcher: ({ compact }: { compact?: boolean }) => (
    <div data-testid="language-switcher" data-compact={compact ? "true" : "false"}>
      LanguageSwitcher
    </div>
  ),
}));

// Mock WalletStatus
vi.mock("@/components/wallet/WalletStatus", () => ({
  WalletStatus: () => <div data-testid="wallet-status">WalletStatus</div>,
}));

// Mock focus trap
vi.mock("@/lib/a11y/utils", () => ({
  useFocusTrap: () => ({ current: null }),
}));

// ── Tests ────────────────────────────────────────────────────────────────

describe("Navbar", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("renders the Xergon brand name", () => {
    render(<Navbar />);
    // Brand appears in both header and mobile drawer — use getAllByText
    const xSpans = screen.getAllByText("X");
    expect(xSpans.length).toBeGreaterThanOrEqual(2);
    const ergonSpans = screen.getAllByText("ergon");
    expect(ergonSpans.length).toBeGreaterThanOrEqual(2);
  });

  it("renders the brand as a link to /playground", () => {
    render(<Navbar />);
    const brandLinks = screen.getAllByText("X").map((el) => el.closest("a"));
    // The first "X" link should be the header brand
    expect(brandLinks[0]).toHaveAttribute("href", "/playground");
  });

  it("renders all main navigation links", () => {
    render(<Navbar />);
    const expectedLinks = ["Playground", "Models", "Analytics", "Explorer", "GPU Bazar", "Leaderboard", "Pricing", "Status"];
    for (const label of expectedLinks) {
      expect(screen.getByText(label, { selector: "nav a" })).toBeInTheDocument();
    }
  });

  it("renders ThemeToggle component", () => {
    render(<Navbar />);
    expect(screen.getByLabelText("Theme toggle")).toBeInTheDocument();
  });

  it("renders LanguageSwitcher component", () => {
    render(<Navbar />);
    // Both desktop and mobile LanguageSwitcher render
    const switchers = screen.getAllByTestId("language-switcher");
    expect(switchers.length).toBeGreaterThanOrEqual(1);
  });

  it('shows "Connect Wallet" when not authenticated', () => {
    render(<Navbar />);
    // Connect Wallet appears in both desktop nav and mobile drawer
    const walletLinks = screen.getAllByText("Connect Wallet");
    expect(walletLinks.length).toBeGreaterThanOrEqual(1);
  });

  it("does not show auth-only links when not authenticated", () => {
    render(<Navbar />);
    expect(screen.queryByText("Transactions")).not.toBeInTheDocument();
    expect(screen.queryByText("Become a Provider")).not.toBeInTheDocument();
    expect(screen.queryByText("Settings")).not.toBeInTheDocument();
  });

  it("renders the hamburger menu button for mobile", () => {
    render(<Navbar />);
    expect(screen.getByLabelText("Open menu")).toBeInTheDocument();
  });

  it("has the correct number of main nav links (21)", () => {
    render(<Navbar />);
    const nav = screen.getByLabelText("Main navigation");
    const links = nav.querySelectorAll("a");
    // NAV_LINKS has 20 items + 1 Provider Dashboard link when authenticated
    expect(links.length).toBe(21);
  });
});
