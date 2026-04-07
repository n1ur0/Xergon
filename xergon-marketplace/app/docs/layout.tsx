"use client";

import { useState, useCallback, useRef, useEffect } from "react";
import Link from "next/link";
import { usePathname } from "next/navigation";

const DOC_NAV = [
  {
    category: "Getting Started",
    links: [
      { href: "/docs/getting-started", label: "Quick Start" },
      { href: "/docs/concepts", label: "Key Concepts" },
    ],
  },
  {
    category: "API",
    links: [
      { href: "/docs/api-reference", label: "API Reference" },
      { href: "/docs/models", label: "Model Catalog" },
    ],
  },
  {
    category: "SDK",
    links: [{ href: "/docs/sdk", label: "SDK Documentation" }],
  },
];

function Breadcrumbs() {
  const pathname = usePathname();
  const segments = pathname.split("/").filter(Boolean);

  return (
    <nav aria-label="Breadcrumb" className="mb-6">
      <ol className="flex items-center gap-1.5 text-sm text-surface-800/50">
        <li>
          <Link href="/" className="hover:text-surface-900 transition-colors">
            Home
          </Link>
        </li>
        {segments.map((seg, i) => {
          const href = "/" + segments.slice(0, i + 1).join("/");
          const isLast = i === segments.length - 1;
          return (
            <li key={href} className="flex items-center gap-1.5">
              <span aria-hidden="true">/</span>
              {isLast ? (
                <span className="text-surface-900 font-medium capitalize">
                  {seg.replace(/-/g, " ")}
                </span>
              ) : (
                <Link
                  href={href}
                  className="hover:text-surface-900 transition-colors capitalize"
                >
                  {seg.replace(/-/g, " ")}
                </Link>
              )}
            </li>
          );
        })}
      </ol>
    </nav>
  );
}

export default function DocsLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  const pathname = usePathname();
  const [sidebarOpen, setSidebarOpen] = useState(false);
  const sidebarRef = useRef<HTMLDivElement>(null);

  const closeSidebar = useCallback(() => setSidebarOpen(false), []);

  // Close sidebar on route change
  useEffect(() => {
    setSidebarOpen(false);
  }, [pathname]);

  // Close sidebar on outside click
  useEffect(() => {
    function handleClick(e: MouseEvent) {
      if (
        sidebarOpen &&
        sidebarRef.current &&
        !sidebarRef.current.contains(e.target as Node)
      ) {
        setSidebarOpen(false);
      }
    }
    document.addEventListener("mousedown", handleClick);
    return () => document.removeEventListener("mousedown", handleClick);
  }, [sidebarOpen]);

  // Lock body scroll when sidebar open on mobile
  useEffect(() => {
    if (sidebarOpen) {
      document.body.style.overflow = "hidden";
    } else {
      document.body.style.overflow = "";
    }
    return () => {
      document.body.style.overflow = "";
    };
  }, [sidebarOpen]);

  return (
    <div className="min-h-[calc(100dvh-3.5rem)]">
      {/* Mobile sidebar toggle */}
      <div className="lg:hidden sticky top-14 z-30 border-b border-surface-200 bg-surface-0/80 backdrop-blur-md">
        <div className="mx-auto max-w-6xl px-4 flex items-center h-11 gap-3">
          <button
            onClick={() => setSidebarOpen(true)}
            className="inline-flex items-center gap-2 text-sm text-surface-800/70 hover:text-surface-900 transition-colors"
            aria-label="Open docs navigation"
          >
            <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <line x1="3" y1="6" x2="21" y2="6" />
              <line x1="3" y1="12" x2="21" y2="12" />
              <line x1="3" y1="18" x2="21" y2="18" />
            </svg>
            Docs Menu
          </button>
        </div>
      </div>

      <div className="mx-auto max-w-6xl flex">
        {/* Sidebar backdrop */}
        <div
          className={`fixed inset-0 z-40 bg-black/30 backdrop-blur-sm lg:hidden transition-opacity duration-200 ${
            sidebarOpen ? "opacity-100" : "opacity-0 pointer-events-none"
          }`}
          onClick={closeSidebar}
          aria-hidden="true"
        />

        {/* Sidebar */}
        <aside
          ref={sidebarRef}
          className={`fixed lg:sticky top-14 left-0 z-50 lg:z-10 h-[calc(100dvh-3.5rem)] w-64 shrink-0 border-r border-surface-200 bg-surface-0 overflow-y-auto transition-transform duration-200 lg:translate-x-0 ${
            sidebarOpen ? "translate-x-0" : "-translate-x-full"
          }`}
          style={{ paddingTop: "0.5rem" }}
        >
          <div className="px-4 py-4 space-y-6">
            {DOC_NAV.map((group) => (
              <div key={group.category}>
                <h3 className="text-xs font-semibold uppercase tracking-wider text-surface-800/40 mb-2">
                  {group.category}
                </h3>
                <ul className="space-y-1">
                  {group.links.map((link) => {
                    const isActive = pathname === link.href;
                    return (
                      <li key={link.href}>
                        <Link
                          href={link.href}
                          onClick={closeSidebar}
                          className={`block px-3 py-2 text-sm rounded-lg transition-colors ${
                            isActive
                              ? "bg-brand-50 text-brand-700 font-medium dark:bg-brand-950/40 dark:text-brand-300"
                              : "text-surface-800/70 hover:text-surface-900 hover:bg-surface-100"
                          }`}
                        >
                          {link.label}
                        </Link>
                      </li>
                    );
                  })}
                </ul>
              </div>
            ))}
          </div>
        </aside>

        {/* Main content */}
        <main className="flex-1 min-w-0 px-4 sm:px-8 py-8">
          <Breadcrumbs />
          <div className="max-w-4xl">{children}</div>
        </main>
      </div>
    </div>
  );
}
