"use client";

import { useState, useEffect, useRef, useCallback } from "react";
import Link from "next/link";
import { useRouter } from "next/navigation";
import { useAuthStore } from "@/lib/stores/auth";
import { useChainBalance } from "@/lib/hooks/use-chain-data";
import { ThemeToggle } from "@/components/ThemeToggle";
import { LanguageSwitcher } from "@/components/LanguageSwitcher";
import { WalletStatus } from "@/components/wallet/WalletStatus";
import { useFocusTrap } from "@/lib/a11y/utils";

const NAV_LINKS = [
  { href: "/playground", label: "Playground" },
  { href: "/models", label: "Models" },
  { href: "/analytics", label: "Analytics" },
  { href: "/explorer", label: "Explorer" },
  { href: "/gpu", label: "GPU Bazar" },
  { href: "/leaderboard", label: "Leaderboard" },
  { href: "/pricing", label: "Pricing" },
  { href: "/commitments", label: "Commitments" },
  { href: "/oracle", label: "Oracle" },
  { href: "/health", label: "Status" },
] as const;

/** Links only shown when authenticated */
const AUTH_NAV_LINKS = [
  { href: "/transactions", label: "Transactions" },
  { href: "/become-provider", label: "Become a Provider" },
  { href: "/settings", label: "Settings" },
] as const;

function truncateAddress(addr: string): string {
  if (addr.length <= 16) return addr;
  return `${addr.slice(0, 10)}...${addr.slice(-4)}`;
}

/** Balance display showing live staking ERG balance */
function StakingBalanceBadge() {
  const { balanceErg, stakingBoxesCount, isLoading } = useChainBalance();

  if (isLoading) {
    return (
      <div className="flex items-center gap-1.5 text-sm" aria-live="polite" aria-busy="true">
        <span className="inline-block h-2 w-2 rounded-full bg-surface-300 animate-pulse" aria-hidden="true" />
        <span className="text-surface-800/40">Loading staking balance...</span>
      </div>
    );
  }

  if (stakingBoxesCount === 0 || balanceErg === 0) {
    return (
      <div className="flex items-center gap-1.5 text-sm">
        <span className="inline-block h-2 w-2 rounded-full bg-surface-300" aria-hidden="true" />
        <span className="text-surface-800/40">No staking box</span>
      </div>
    );
  }

  return (
    <div className="flex items-center gap-1.5 text-sm">
      <span className="inline-block h-2 w-2 rounded-full bg-accent-500" aria-hidden="true" />
      <span className="font-medium text-surface-900">
        {balanceErg.toFixed(4)} ERG
      </span>
      <span className="text-xs text-surface-800/30">
        ({stakingBoxesCount} box{stakingBoxesCount !== 1 ? "es" : ""})
      </span>
    </div>
  );
}

export function Navbar() {
  const isAuthenticated = useAuthStore((s) => s.isAuthenticated);
  const user = useAuthStore((s) => s.user);
  const [mobileOpen, setMobileOpen] = useState(false);
  const router = useRouter();
  const drawerRef = useRef<HTMLDivElement>(null);

  const handleCloseDrawer = useCallback(() => {
    setMobileOpen(false);
  }, []);

  const focusTrapRef = useFocusTrap(mobileOpen, handleCloseDrawer);

  /** Close the mobile menu and navigate */
  const handleMobileNav = useCallback((href: string) => {
    setMobileOpen(false);
    router.push(href);
  }, [router]);

  /** Close drawer on outside click (backdrop tap) */
  const handleBackdropClick = useCallback(() => {
    setMobileOpen(false);
  }, []);

  /** Lock body scroll when drawer is open */
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

  /** Close drawer on route change */
  useEffect(() => {
    const handleRouteChange = () => setMobileOpen(false);
    window.addEventListener("popstate", handleRouteChange);
    return () => window.removeEventListener("popstate", handleRouteChange);
  }, []);

  return (
    <>
      <header className="sticky top-0 z-50 border-b border-surface-200 bg-surface-0/80 backdrop-blur-md" style={{ paddingTop: "env(safe-area-inset-top)" }}>
        <div className="mx-auto flex h-14 max-w-6xl items-center justify-between px-4">
          {/* Logo */}
          <Link href="/playground" className="flex items-center gap-2 font-bold text-lg">
            <span className="text-brand-600">X</span>
            <span>ergon</span>
          </Link>

          {/* Desktop nav links — hidden on mobile */}
          <nav className="hidden md:flex items-center gap-1" aria-label="Main navigation">
            {NAV_LINKS.map((link) => (
              <Link
                key={link.href}
                href={link.href}
                className="px-3 py-1.5 text-sm rounded-lg text-surface-800/70 hover:text-surface-900 hover:bg-surface-100 transition-colors inline-flex items-center gap-1.5"
              >
                {link.label}
                {link.href === "/health" && (
                  <span className="h-1.5 w-1.5 rounded-full bg-emerald-500" aria-label="All systems operational" title="All systems operational" />
                )}
              </Link>
            ))}
            {isAuthenticated &&
              AUTH_NAV_LINKS.map((link) => (
                <Link
                  key={link.href}
                  href={link.href}
                  className="px-3 py-1.5 text-sm rounded-lg text-surface-800/70 hover:text-surface-900 hover:bg-surface-100 transition-colors"
                >
                  {link.label}
                </Link>
              ))}
            {isAuthenticated && user?.ergoAddress && (
              <Link
                href="/provider"
                className="px-3 py-1.5 text-sm rounded-lg text-surface-800/70 hover:text-surface-900 hover:bg-surface-100 transition-colors"
              >
                Provider Dashboard
              </Link>
            )}
          </nav>

          {/* Right side: theme toggle + hamburger (mobile) + wallet info / auth (desktop) */}
          <div className="flex items-center gap-3">
            <LanguageSwitcher />
            <ThemeToggle />
            {/* Desktop: wallet + auth */}
            <div className="hidden md:flex items-center gap-3">
              {isAuthenticated && user ? (
                <>
                  <WalletStatus />
                  <StakingBalanceBadge />
                  <span className="text-xs font-mono text-surface-800/40">
                    {truncateAddress(user.ergoAddress)}
                  </span>
                  <button
                    onClick={() => {
                      useAuthStore.getState().signOut();
                      router.push("/signin");
                    }}
                    className="text-sm text-surface-800/50 hover:text-surface-800/70 transition-colors"
                  >
                    Disconnect
                  </button>
                </>
              ) : (
                <Link
                  href="/signin"
                  className="rounded-lg bg-brand-600 px-4 py-1.5 text-sm font-medium text-white transition-colors hover:bg-brand-700"
                >
                  Connect Wallet
                </Link>
              )}
            </div>

            {/* Hamburger button — visible only on small screens */}
            <button
              type="button"
              aria-label={mobileOpen ? "Close menu" : "Open menu"}
              aria-expanded={mobileOpen}
              onClick={() => setMobileOpen((prev) => !prev)}
              className="inline-flex md:hidden items-center justify-center rounded-lg p-2 text-surface-800/70 hover:text-surface-900 hover:bg-surface-100 transition-colors min-h-[44px] min-w-[44px]"
            >
              {mobileOpen ? (
                /* X (close) icon */
                <svg
                  xmlns="http://www.w3.org/2000/svg"
                  width="22"
                  height="22"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  strokeWidth="2"
                  strokeLinecap="round"
                  strokeLinejoin="round"
                >
                  <line x1="18" y1="6" x2="6" y2="18" />
                  <line x1="6" y1="6" x2="18" y2="18" />
                </svg>
              ) : (
                /* Hamburger icon (3 lines) */
                <svg
                  xmlns="http://www.w3.org/2000/svg"
                  width="22"
                  height="22"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  strokeWidth="2"
                  strokeLinecap="round"
                  strokeLinejoin="round"
                >
                  <line x1="3" y1="6" x2="21" y2="6" />
                  <line x1="3" y1="12" x2="21" y2="12" />
                  <line x1="3" y1="18" x2="21" y2="18" />
                </svg>
              )}
            </button>
          </div>
        </div>
      </header>

      {/* Mobile slide-out drawer */}
      {/* Backdrop overlay */}
      <div
        className={`fixed inset-0 z-[60] bg-black/40 backdrop-blur-sm transition-opacity duration-300 md:hidden ${
          mobileOpen ? "opacity-100" : "opacity-0 pointer-events-none"
        }`}
        onClick={handleBackdropClick}
        aria-hidden="true"
      />

      {/* Drawer panel */}
      <div
        ref={(node) => {
          (drawerRef as React.MutableRefObject<HTMLDivElement | null>).current = node;
          (focusTrapRef as React.MutableRefObject<HTMLElement | null>).current = node;
        }}
        role="dialog"
        aria-modal={mobileOpen}
        aria-label="Navigation menu"
        className={`fixed top-0 right-0 z-[70] h-full w-[280px] max-w-[80vw] bg-surface-0 shadow-2xl transition-transform duration-300 ease-out md:hidden ${
          mobileOpen ? "translate-x-0" : "translate-x-full"
        }`}
        style={{ paddingTop: "env(safe-area-inset-top)" }}
      >
        {/* Drawer header */}
        <div className="flex items-center justify-between px-4 h-14 border-b border-surface-200">
          <Link
            href="/playground"
            onClick={() => setMobileOpen(false)}
            className="flex items-center gap-2 font-bold text-lg"
          >
            <span className="text-brand-600">X</span>
            <span>ergon</span>
          </Link>
          <button
            type="button"
            aria-label="Close menu"
            onClick={() => setMobileOpen(false)}
            className="inline-flex items-center justify-center rounded-lg p-2 text-surface-800/70 hover:text-surface-900 hover:bg-surface-100 transition-colors min-h-[44px] min-w-[44px]"
          >
            <svg
              xmlns="http://www.w3.org/2000/svg"
              width="22"
              height="22"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth="2"
              strokeLinecap="round"
              strokeLinejoin="round"
            >
              <line x1="18" y1="6" x2="6" y2="18" />
              <line x1="6" y1="6" x2="18" y2="18" />
            </svg>
          </button>
        </div>

        {/* Drawer content - scrollable */}
        <nav className="overflow-y-auto px-3 py-4 space-y-1" aria-label="Mobile navigation" style={{ paddingBottom: "calc(env(safe-area-inset-bottom) + 1rem)" }}>
          {/* Standard nav links */}
          {NAV_LINKS.map((link) => (
            <button
              key={link.href}
              onClick={() => handleMobileNav(link.href)}
              className="block w-full text-left px-3 py-3 text-sm rounded-lg text-surface-800/70 hover:text-surface-900 hover:bg-surface-100 transition-colors min-h-[44px] inline-flex items-center gap-1.5"
            >
              {link.label}
              {link.href === "/health" && (
                <span className="h-1.5 w-1.5 rounded-full bg-emerald-500" aria-label="All systems operational" title="All systems operational" />
              )}
            </button>
          ))}

          {/* Auth-only links */}
          {isAuthenticated &&
            AUTH_NAV_LINKS.map((link) => (
              <button
                key={link.href}
                onClick={() => handleMobileNav(link.href)}
                className="block w-full text-left px-3 py-3 text-sm rounded-lg text-surface-800/70 hover:text-surface-900 hover:bg-surface-100 transition-colors min-h-[44px]"
              >
                {link.label}
              </button>
            ))}

          {/* Provider Dashboard — only when wallet is connected */}
          {isAuthenticated && user?.ergoAddress && (
            <button
              onClick={() => handleMobileNav("/provider")}
              className="block w-full text-left px-3 py-3 text-sm rounded-lg text-surface-800/70 hover:text-surface-900 hover:bg-surface-100 transition-colors min-h-[44px]"
            >
              Provider Dashboard
            </button>
          )}

          {/* Divider + language + wallet / auth actions */}
          <div className="pt-3 mt-3 border-t border-surface-200">
            {/* Language switcher for mobile */}
            <div className="px-3 pb-2">
              <LanguageSwitcher compact />
            </div>
            {isAuthenticated && user ? (
              <div className="flex flex-col gap-3 px-3 py-2">
                <div className="flex items-center">
                  <WalletStatus />
                </div>
                <div className="flex items-center">
                  <StakingBalanceBadge />
                </div>
                <span className="text-xs font-mono text-surface-800/40">
                  {truncateAddress(user.ergoAddress)}
                </span>
                <button
                  onClick={() => {
                    useAuthStore.getState().signOut();
                    setMobileOpen(false);
                    router.push("/signin");
                  }}
                  className="text-sm text-surface-800/50 hover:text-surface-800/70 transition-colors min-h-[44px] text-left px-1"
                >
                  Disconnect
                </button>
              </div>
            ) : (
              <button
                onClick={() => handleMobileNav("/signin")}
                className="block w-full text-center rounded-lg bg-brand-600 px-4 py-3 text-sm font-medium text-white transition-colors hover:bg-brand-700 min-h-[44px]"
              >
                Connect Wallet
              </button>
            )}
          </div>
        </nav>
      </div>
    </>
  );
}
