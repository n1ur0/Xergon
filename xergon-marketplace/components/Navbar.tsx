"use client";

import { useState } from "react";
import Link from "next/link";
import { useRouter } from "next/navigation";
import { CreditsBadge } from "./CreditsBadge";
import { useAuthStore } from "@/lib/stores/auth";

const NAV_LINKS = [
  { href: "/playground", label: "Playground" },
  { href: "/models", label: "Models" },
  { href: "/leaderboard", label: "Leaderboard" },
  { href: "/pricing", label: "Pricing" },
] as const;

/** Links only shown when authenticated */
const AUTH_NAV_LINKS = [
  { href: "/become-provider", label: "Become a Provider" },
  { href: "/settings", label: "Settings" },
] as const;

export function Navbar() {
  const isAuthenticated = useAuthStore((s) => s.isAuthenticated);
  const hasWallet = useAuthStore((s) => !!s.user?.ergoAddress);
  const [mobileOpen, setMobileOpen] = useState(false);
  const router = useRouter();

  /** Close the mobile menu and navigate */
  const handleMobileNav = (href: string) => {
    setMobileOpen(false);
    router.push(href);
  };

  return (
    <header className="sticky top-0 z-50 border-b border-surface-200 bg-surface-0/80 backdrop-blur-md">
      <div className="mx-auto flex h-14 max-w-6xl items-center justify-between px-4">
        {/* Logo */}
        <Link href="/playground" className="flex items-center gap-2 font-bold text-lg">
          <span className="text-brand-600">X</span>
          <span>ergon</span>
        </Link>

        {/* Desktop nav links — hidden on mobile */}
        <nav className="hidden md:flex items-center gap-1">
          {NAV_LINKS.map((link) => (
            <Link
              key={link.href}
              href={link.href}
              className="px-3 py-1.5 text-sm rounded-lg text-surface-800/70 hover:text-surface-900 hover:bg-surface-100 transition-colors"
            >
              {link.label}
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
          {isAuthenticated && hasWallet && (
            <Link
              href="/provider"
              className="px-3 py-1.5 text-sm rounded-lg text-surface-800/70 hover:text-surface-900 hover:bg-surface-100 transition-colors"
            >
              Provider Dashboard
            </Link>
          )}
        </nav>

        {/* Right side: hamburger (mobile) + credits + auth (desktop) */}
        <div className="flex items-center gap-3">
          {/* Desktop: credits + auth */}
          <div className="hidden md:flex items-center gap-3">
            {isAuthenticated ? (
              <>
                <CreditsBadge />
                <button
                  onClick={() => useAuthStore.getState().logout()}
                  className="text-sm text-surface-800/50 hover:text-surface-800/70 transition-colors"
                >
                  Sign out
                </button>
              </>
            ) : (
              <Link
                href="/signin"
                className="rounded-lg bg-brand-600 px-4 py-1.5 text-sm font-medium text-white transition-colors hover:bg-brand-700"
              >
                Sign in
              </Link>
            )}
          </div>

          {/* Hamburger button — visible only on small screens */}
          <button
            type="button"
            aria-label={mobileOpen ? "Close menu" : "Open menu"}
            aria-expanded={mobileOpen}
            onClick={() => setMobileOpen((prev) => !prev)}
            className="inline-flex md:hidden items-center justify-center rounded-lg p-2 text-surface-800/70 hover:text-surface-900 hover:bg-surface-100 transition-colors"
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

      {/* Mobile dropdown menu */}
      <div
        className={`
          overflow-hidden md:hidden
          transition-all duration-200 ease-in-out
          ${mobileOpen ? "max-h-96 opacity-100" : "max-h-0 opacity-0"}
        `}
      >
        <nav className="border-t border-surface-200 bg-surface-900 px-4 py-3 space-y-1">
          {/* Standard nav links */}
          {NAV_LINKS.map((link) => (
            <button
              key={link.href}
              onClick={() => handleMobileNav(link.href)}
              className="block w-full text-left px-3 py-2.5 text-sm rounded-lg text-surface-200/80 hover:text-white hover:bg-surface-800 transition-colors"
            >
              {link.label}
            </button>
          ))}

          {/* Auth-only links */}
          {isAuthenticated &&
            AUTH_NAV_LINKS.map((link) => (
              <button
                key={link.href}
                onClick={() => handleMobileNav(link.href)}
                className="block w-full text-left px-3 py-2.5 text-sm rounded-lg text-surface-200/80 hover:text-white hover:bg-surface-800 transition-colors"
              >
                {link.label}
              </button>
            ))}

          {/* Provider Dashboard — only when wallet is linked */}
          {isAuthenticated && hasWallet && (
            <button
              onClick={() => handleMobileNav("/provider")}
              className="block w-full text-left px-3 py-2.5 text-sm rounded-lg text-surface-200/80 hover:text-white hover:bg-surface-800 transition-colors"
            >
              Provider Dashboard
            </button>
          )}

          {/* Divider + credits / auth actions */}
          <div className="pt-2 mt-2 border-t border-surface-800">
            {isAuthenticated ? (
              <div className="flex items-center justify-between px-3 py-2">
                <CreditsBadge />
                <button
                  onClick={() => {
                    useAuthStore.getState().logout();
                    setMobileOpen(false);
                  }}
                  className="text-sm text-surface-200/50 hover:text-surface-200/70 transition-colors"
                >
                  Sign out
                </button>
              </div>
            ) : (
              <button
                onClick={() => handleMobileNav("/signin")}
                className="block w-full text-center rounded-lg bg-brand-600 px-4 py-2.5 text-sm font-medium text-white transition-colors hover:bg-brand-700"
              >
                Sign in
              </button>
            )}
          </div>
        </nav>
      </div>
    </header>
  );
}
