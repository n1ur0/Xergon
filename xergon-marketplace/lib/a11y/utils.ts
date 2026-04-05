/**
 * Accessibility utilities for Xergon Marketplace.
 *
 * Provides React hooks and helper functions for common a11y patterns:
 *  - Focus trapping for modals / drawers
 *  - Keyboard navigation helpers
 *  - Screen reader announcements via aria-live regions
 *  - Shared ARIA prop generators
 */

"use client";

import { useEffect, useRef, useCallback } from "react";

// ---------------------------------------------------------------------------
// Screen-reader only CSS class
// ---------------------------------------------------------------------------

/** Tailwind-compatible class name for visually hidden content (still read by SRs). */
export const srOnly = "sr-only";

// ---------------------------------------------------------------------------
// announceToScreenReader
// ---------------------------------------------------------------------------

let liveRegion: HTMLElement | null = null;

/**
 * Announce a message to screen readers by injecting text into an
 * aria-live="polite" region that is visually hidden.
 *
 * If the region does not exist yet it is created and appended to `<body>`.
 */
export function announceToScreenReader(message: string): void {
  if (typeof document === "undefined") return;

  if (!liveRegion) {
    liveRegion = document.createElement("div");
    liveRegion.setAttribute("aria-live", "polite");
    liveRegion.setAttribute("aria-atomic", "true");
    // Visually hidden but accessible to assistive tech
    liveRegion.style.cssText =
      "position:absolute;width:1px;height:1px;padding:0;margin:-1px;overflow:hidden;clip:rect(0,0,0,0);white-space:nowrap;border-width:0;";
    document.body.appendChild(liveRegion);
  }

  // Clear and re-set so repeated messages still trigger an announcement
  liveRegion.textContent = "";
  // Use requestAnimationFrame to ensure the browser processes the clearing first
  requestAnimationFrame(() => {
    if (liveRegion) {
      liveRegion.textContent = message;
    }
  });
}

// ---------------------------------------------------------------------------
// useFocusTrap
// ---------------------------------------------------------------------------

/**
 * Traps keyboard focus within the given container element.
 *
 * Tab / Shift+Tab cycle through focusable descendants.
 * Escape key is forwarded to the optional `onEscape` callback.
 *
 * The trap is only active when `active` is `true`.
 */
export function useFocusTrap<T extends HTMLElement = HTMLElement>(
  active: boolean,
  onEscape?: () => void
): React.RefObject<T | null> {
  const containerRef = useRef<T | null>(null);

  useEffect(() => {
    if (!active || !containerRef.current) return;

    const container = containerRef.current;

    function getFocusableElements(): HTMLElement[] {
      const selectors = [
        "a[href]",
        "button:not([disabled])",
        "input:not([disabled])",
        "select:not([disabled])",
        "textarea:not([disabled])",
        '[tabindex]:not([tabindex="-1"])',
        "[contenteditable]",
      ];
      return Array.from(
        container.querySelectorAll<HTMLElement>(selectors.join(","))
      ).filter(
        (el) =>
          !el.hasAttribute("aria-hidden") &&
          el.offsetParent !== null // visible
      );
    }

    function handleKeyDown(e: KeyboardEvent) {
      if (e.key === "Escape") {
        e.preventDefault();
        onEscape?.();
        return;
      }

      if (e.key !== "Tab") return;

      const focusable = getFocusableElements();
      if (focusable.length === 0) return;

      const first = focusable[0];
      const last = focusable[focusable.length - 1];

      if (e.shiftKey) {
        // Shift+Tab: if focus is on first element, wrap to last
        if (document.activeElement === first) {
          e.preventDefault();
          last.focus();
        }
      } else {
        // Tab: if focus is on last element, wrap to first
        if (document.activeElement === last) {
          e.preventDefault();
          first.focus();
        }
      }
    }

    // Move focus into the trap
    const focusable = getFocusableElements();
    if (focusable.length > 0) {
      // Delay one frame so the transition has started
      requestAnimationFrame(() => {
        focusable[0].focus();
      });
    }

    container.addEventListener("keydown", handleKeyDown);
    return () => container.removeEventListener("keydown", handleKeyDown);
  }, [active, onEscape]);

  return containerRef;
}

// ---------------------------------------------------------------------------
// useKeyboardNavigation
// ---------------------------------------------------------------------------

interface KeyboardNavOptions {
  /** Called when Escape is pressed */
  onEscape?: () => void;
  /** Called when Enter or Space is pressed (for custom widgets) */
  onActivate?: () => void;
  /** Called when ArrowDown is pressed */
  onArrowDown?: () => void;
  /** Called when ArrowUp is pressed */
  onArrowUp?: () => void;
  /** Called when ArrowRight is pressed */
  onArrowRight?: () => void;
  /** Called when ArrowLeft is pressed */
  onArrowLeft?: () => void;
  /** Called when Home is pressed */
  onHome?: () => void;
  /** Called when End is pressed */
  onEnd?: () => void;
  /** Whether the handler is active */
  active?: boolean;
}

/**
 * Attaches keyboard event listeners for common navigation patterns.
 * Useful for custom interactive widgets (comboboxes, menus, etc.).
 */
export function useKeyboardNavigation(options: KeyboardNavOptions): {
  handleKeyDown: (e: React.KeyboardEvent) => void;
} {
  const {
    onEscape,
    onActivate,
    onArrowDown,
    onArrowUp,
    onArrowRight,
    onArrowLeft,
    onHome,
    onEnd,
    active = true,
  } = options;

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (!active) return;

      switch (e.key) {
        case "Escape":
          e.preventDefault();
          onEscape?.();
          break;
        case "Enter":
        case " ":
          if (onActivate) {
            e.preventDefault();
            onActivate();
          }
          break;
        case "ArrowDown":
          e.preventDefault();
          onArrowDown?.();
          break;
        case "ArrowUp":
          e.preventDefault();
          onArrowUp?.();
          break;
        case "ArrowRight":
          e.preventDefault();
          onArrowRight?.();
          break;
        case "ArrowLeft":
          e.preventDefault();
          onArrowLeft?.();
          break;
        case "Home":
          e.preventDefault();
          onHome?.();
          break;
        case "End":
          e.preventDefault();
          onEnd?.();
          break;
      }
    },
    [active, onEscape, onActivate, onArrowDown, onArrowUp, onArrowRight, onArrowLeft, onHome, onEnd]
  );

  return { handleKeyDown };
}

// ---------------------------------------------------------------------------
// getAriaProps
// ---------------------------------------------------------------------------

interface AriaPropsOptions {
  /** For elements with expandable content (accordion, combobox) */
  expanded?: boolean;
  /** For elements that control another element */
  controlsId?: string;
  /** For modal / dialog containers */
  modal?: boolean;
  /** For live regions */
  live?: "polite" | "assertive" | "off";
  /** For busy states */
  busy?: boolean;
  /** Accessible name (overrides aria-label) */
  labelledBy?: string;
  /** Accessible description */
  describedBy?: string;
  /** Current value (for sliders, progress, etc.) */
  valueNow?: number;
  /** Min value */
  valueMin?: number;
  /** Max value */
  valueMax?: number;
  /** For sort indicators */
  sortDirection?: "ascending" | "descending" | "none";
  /** Role override */
  role?: string;
  /** Whether the element is selected (listbox options, tabs) */
  selected?: boolean;
}

/**
 * Returns a flat object of ARIA attributes based on common patterns.
 * Spread the result onto a JSX element.
 *
 * @example
 * ```tsx
 * <th {...getAriaProps({ sortDirection: "ascending" })}>Name</th>
 * <div {...getAriaProps({ modal: true, labelledBy: "dialog-title" })}>
 * ```
 */
export function getAriaProps(options: AriaPropsOptions): Record<string, string | boolean | number | undefined> {
  const props: Record<string, string | boolean | number | undefined> = {};

  if (options.role) props["role"] = options.role;
  if (options.expanded !== undefined) props["aria-expanded"] = options.expanded;
  if (options.controlsId) props["aria-controls"] = options.controlsId;
  if (options.modal !== undefined) props["aria-modal"] = options.modal;
  if (options.live) props["aria-live"] = options.live;
  if (options.busy !== undefined) props["aria-busy"] = options.busy;
  if (options.labelledBy) props["aria-labelledby"] = options.labelledBy;
  if (options.describedBy) props["aria-describedby"] = options.describedBy;
  if (options.valueNow !== undefined) props["aria-valuenow"] = options.valueNow;
  if (options.valueMin !== undefined) props["aria-valuemin"] = options.valueMin;
  if (options.valueMax !== undefined) props["aria-valuemax"] = options.valueMax;
  if (options.sortDirection) props["aria-sort"] = options.sortDirection;
  if (options.selected !== undefined) props["aria-selected"] = options.selected;

  return props;
}
