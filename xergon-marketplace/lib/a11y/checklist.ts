/**
 * Accessibility Checklist for Xergon Marketplace
 *
 * This is a reference checklist documenting all accessibility fixes applied.
 * It is NOT a runtime test suite, but a human-readable record of what was
 * audited and fixed, organized by WCAG category.
 *
 * Each item has a status: DONE | PARTIAL | N/A
 */

export const a11yChecklist = {
  // -----------------------------------------------------------------------
  // 1. ARIA Labels & Roles
  // -----------------------------------------------------------------------
  ariaLabelsAndRoles: {
    items: [
      {
        id: "aria-001",
        rule: "All interactive elements (buttons, links, inputs) have accessible names",
        status: "DONE",
        details:
          "Hamburger button: aria-label + aria-expanded. ThemeToggle: aria-label. LanguageSwitcher: aria-label + aria-expanded + aria-haspopup. Disconnect button: visible text label. Icon-only close buttons: aria-label='Close'. Copy button: aria-label. Sort toggle: aria-label.",
        files: [
          "components/Navbar.tsx",
          "components/ThemeToggle.tsx",
          "components/LanguageSwitcher.tsx",
          "components/auth/ErgoAuthModal.tsx",
          "components/ergopay/ErgoPayModal.tsx",
          "components/explorer/ProviderFilters.tsx",
        ],
      },
      {
        id: "aria-002",
        rule: "Modal dialogs have role='dialog', aria-modal, aria-labelledby, focus trap",
        status: "DONE",
        details:
          "ErgoAuthModal: role='dialog', aria-modal='true', aria-labelledby='ergoauth-title', focus trap via useFocusTrap, Escape closes. ErgoPayModal: role='dialog', aria-modal='true', aria-labelledby='ergopay-title', focus trap via useFocusTrap, Escape closes.",
        files: [
          "components/auth/ErgoAuthModal.tsx",
          "components/ergopay/ErgoPayModal.tsx",
        ],
      },
      {
        id: "aria-003",
        rule: "Loading states use aria-live or aria-busy",
        status: "DONE",
        details:
          "StakingBalanceBadge loading spinner: aria-live='polite' wrapper. TxTable loading: aria-busy on container. Service status: announced via status text (not just color).",
        files: [
          "components/Navbar.tsx",
          "components/transactions/TxTable.tsx",
        ],
      },
      {
        id: "aria-004",
        rule: "Status indicators have aria-label (not just color dots)",
        status: "DONE",
        details:
          "ServiceCard: status badge has text label alongside color dot. ProviderCard: status dot has aria-label. TxTable: status badges have text label. Navbar health indicator: aria-label on status dot.",
        files: [
          "components/health/ServiceCard.tsx",
          "components/explorer/ProviderCard.tsx",
          "components/transactions/TxTable.tsx",
          "components/Navbar.tsx",
        ],
      },
      {
        id: "aria-005",
        rule: "Charts/SVG have aria-label describing the data",
        status: "DONE",
        details:
          "RequestsChart SVG: role='img' + descriptive aria-label with data summary. QR code SVG in ErgoAuthModal: aria-label describing purpose.",
        files: [
          "components/analytics/RequestsChart.tsx",
          "components/auth/ErgoAuthModal.tsx",
        ],
      },
      {
        id: "aria-006",
        rule: "Icon-only buttons have aria-label",
        status: "DONE",
        details:
          "All close buttons, copy buttons, sort toggles, theme toggle, language switcher have explicit aria-label attributes.",
        files: [
          "components/auth/ErgoAuthModal.tsx",
          "components/ergopay/ErgoPayModal.tsx",
          "components/ThemeToggle.tsx",
          "components/LanguageSwitcher.tsx",
          "components/explorer/ProviderFilters.tsx",
        ],
      },
    ],
  },

  // -----------------------------------------------------------------------
  // 2. Keyboard Navigation
  // -----------------------------------------------------------------------
  keyboardNavigation: {
    items: [
      {
        id: "kb-001",
        rule: "All interactive elements are focusable",
        status: "DONE",
        details:
          "All clickable elements use <button> or <a> (not <div>). ProviderCard expand uses <button>. ServiceCard expand uses <button>. TopModelsTable sort headers use <button>.",
        files: [
          "components/explorer/ProviderCard.tsx",
          "components/health/ServiceCard.tsx",
          "components/analytics/TopModelsTable.tsx",
          "components/transactions/TxTable.tsx",
        ],
      },
      {
        id: "kb-002",
        rule: "Modals trap focus within",
        status: "DONE",
        details:
          "ErgoAuthModal and ErgoPayModal use useFocusTrap hook which cycles Tab/Shift+Tab through focusable children and prevents focus from escaping.",
        files: [
          "lib/a11y/utils.ts",
          "components/auth/ErgoAuthModal.tsx",
          "components/ergopay/ErgoPayModal.tsx",
        ],
      },
      {
        id: "kb-003",
        rule: "Escape key closes modals",
        status: "DONE",
        details:
          "Both ErgoAuthModal and ErgoPayModal close on Escape. The mobile drawer in Navbar closes on Escape via focus trap.",
        files: [
          "components/auth/ErgoAuthModal.tsx",
          "components/ergopay/ErgoPayModal.tsx",
          "components/Navbar.tsx",
        ],
      },
      {
        id: "kb-004",
        rule: "Tab order is logical",
        status: "DONE",
        details:
          "Components use natural DOM order. No positive tabindex values. SkipToContent link is first focusable element.",
        files: ["components/AppShell.tsx", "components/a11y/SkipToContent.tsx"],
      },
      {
        id: "kb-005",
        rule: "Skip-to-content link at page top",
        status: "DONE",
        details:
          "SkipToContent component renders a link that is sr-only by default, visible on focus. Placed at top of AppShell. Links to #main-content.",
        files: [
          "components/a11y/SkipToContent.tsx",
          "components/AppShell.tsx",
        ],
      },
      {
        id: "kb-006",
        rule: "Focus visible styles (ring/outline)",
        status: "DONE",
        details:
          "SkipToContent link has explicit focus-visible styles. Interactive elements in the app use Tailwind focus:ring utilities. No outline:none without replacement on focusable elements.",
        files: ["components/a11y/SkipToContent.tsx"],
      },
    ],
  },

  // -----------------------------------------------------------------------
  // 3. Color Contrast
  // -----------------------------------------------------------------------
  colorContrast: {
    items: [
      {
        id: "cc-001",
        rule: "Status indicators don't rely solely on color",
        status: "DONE",
        details:
          "All status indicators (ServiceCard, ProviderCard, TxTable) include text labels alongside colored dots/badges. Status text like 'Operational', 'Degraded', 'Down' is always visible.",
        files: [
          "components/health/ServiceCard.tsx",
          "components/explorer/ProviderCard.tsx",
          "components/transactions/TxTable.tsx",
        ],
      },
      {
        id: "cc-002",
        rule: "Error states have text + icon, not just red",
        status: "DONE",
        details:
          "ErgoAuthModal error state: error icon (circle-x) + error text. ErgoPayModal expired: warning icon + text. Form validation: error message text below input.",
        files: [
          "components/auth/ErgoAuthModal.tsx",
          "components/ergopay/ErgoPayModal.tsx",
        ],
      },
      {
        id: "cc-003",
        rule: "Links are distinguishable from text",
        status: "DONE",
        details:
          "Navigation links use hover:bg-surface-100 for visual distinction. Transaction ID links use brand color + hover:underline. Connect Wallet uses bg-brand-600 button.",
        files: [
          "components/Navbar.tsx",
          "components/transactions/TxTable.tsx",
        ],
      },
    ],
  },

  // -----------------------------------------------------------------------
  // 4. Screen Reader Support
  // -----------------------------------------------------------------------
  screenReaderSupport: {
    items: [
      {
        id: "sr-001",
        rule: "Images need alt text",
        status: "N/A",
        details:
          "No <img> elements found in audited components. SVGs used for icons are decorative (aria-hidden or within labeled buttons).",
        files: [],
      },
      {
        id: "sr-002",
        rule: "Tables have proper thead/tbody structure",
        status: "DONE",
        details:
          "TopModelsTable: thead with th elements, tbody with td. TxTable: thead with th elements (some using SortableHeader buttons), tbody with td. Both have proper column headers.",
        files: [
          "components/analytics/TopModelsTable.tsx",
          "components/transactions/TxTable.tsx",
        ],
      },
      {
        id: "sr-003",
        rule: "Form inputs have associated labels",
        status: "DONE",
        details:
          "ErgoAuthModal: <label htmlFor='ergo-address'> associated with input. ProviderFilters: search input has aria-label, selects have aria-label.",
        files: [
          "components/auth/ErgoAuthModal.tsx",
          "components/explorer/ProviderFilters.tsx",
        ],
      },
      {
        id: "sr-004",
        rule: "Dynamic content updates have aria-live regions",
        status: "DONE",
        details:
          "announceToScreenReader() utility created for dynamic announcements. Used for toast-like notifications and status changes.",
        files: ["lib/a11y/utils.ts"],
      },
    ],
  },

  // -----------------------------------------------------------------------
  // 5. Semantic HTML
  // -----------------------------------------------------------------------
  semanticHTML: {
    items: [
      {
        id: "sh-001",
        rule: "Proper heading hierarchy",
        status: "DONE",
        details:
          "AppShell wraps content in <main id='main-content'>. StatsHero metric cards use div (not heading). TopModelsTable uses h2 for section title. RequestsChart uses h2 for section title. ServiceCard uses span for name (appropriate within card context). ProviderCard uses h3 for provider name.",
        files: [
          "components/AppShell.tsx",
          "components/analytics/StatsHero.tsx",
          "components/analytics/TopModelsTable.tsx",
          "components/analytics/RequestsChart.tsx",
          "components/health/ServiceCard.tsx",
          "components/explorer/ProviderCard.tsx",
        ],
      },
      {
        id: "sh-002",
        rule: "Use nav, main, section, article, footer elements",
        status: "DONE",
        details:
          "<header> in Navbar. <main id='main-content'> in AppShell. <nav> for desktop and mobile navigation links.",
        files: [
          "components/Navbar.tsx",
          "components/AppShell.tsx",
        ],
      },
      {
        id: "sh-003",
        rule: "Use button for actions, not div",
        status: "DONE",
        details:
          "ProviderCard: expand/collapse is <button>. ServiceCard: expand/collapse is <button>. TopModelsTable sort headers: <button> (were <th onClick>). TxTable sort headers: already using SortableHeader button. LanguageSwitcher items: <button role='option'>.",
        files: [
          "components/explorer/ProviderCard.tsx",
          "components/health/ServiceCard.tsx",
          "components/analytics/TopModelsTable.tsx",
          "components/transactions/TxTable.tsx",
        ],
      },
      {
        id: "sh-004",
        rule: "Use proper list elements",
        status: "N/A",
        details:
          "Navigation links are rendered as individual buttons/links (acceptable for nav landmarks). No list-based UIs found that need ul/ol/li.",
        files: [],
      },
    ],
  },

  // -----------------------------------------------------------------------
  // 6. Layout & Infrastructure
  // -----------------------------------------------------------------------
  layout: {
    items: [
      {
        id: "ly-001",
        rule: "SkipToContent component at top of page",
        status: "DONE",
        details:
          "SkipToContent added to AppShell, rendered before Navbar. Links to #main-content which is the <main> element.",
        files: ["components/AppShell.tsx", "components/a11y/SkipToContent.tsx"],
      },
      {
        id: "ly-002",
        rule: "lang attribute on html element",
        status: "ALREADY_EXISTS",
        details:
          "layout.tsx already sets <html lang='en'> with a themeScript that dynamically updates lang from persisted locale.",
        files: ["app/layout.tsx"],
      },
      {
        id: "ly-003",
        rule: "main landmark exists",
        status: "DONE",
        details:
          "AppShell wraps children in <main id='main-content' role='main'> for explicit landmark.",
        files: ["components/AppShell.tsx"],
      },
    ],
  },
} as const;

export type A11yChecklist = typeof a11yChecklist;

/** Get a summary of checklist items by status. */
export function getChecklistSummary() {
  const categories = Object.values(a11yChecklist);
  const allItems = categories.flatMap((cat) => [...cat.items]);
  const summary: Record<string, number> = {
    DONE: 0,
    PARTIAL: 0,
    "N/A": 0,
    ALREADY_EXISTS: 0,
  };

  for (const item of allItems) {
    const status = (item as { status: string }).status;
    if (status in summary) {
      summary[status]++;
    }
  }

  return { total: allItems.length, ...summary };
}
