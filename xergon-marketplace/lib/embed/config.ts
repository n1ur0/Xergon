/**
 * Widget configuration types, defaults, and security utilities.
 */

// ── Types ──

export interface WidgetConfig {
  /** Model ID to use (e.g. "llama-3.3-70b") */
  model?: string;
  /** Welcome message shown in empty chat */
  welcomeMessage?: string;
  /** Primary brand color (hex, e.g. "#6366f1") */
  primaryColor?: string;
  /** Position of the floating button */
  position?: "bottom-right" | "bottom-left";
  /** Title shown in the chat header */
  title?: string;
  /** Public key for X-Wallet-PK auth */
  publicKey?: string;
}

export type WidgetPosition = NonNullable<WidgetConfig["position"]>;

// ── Defaults ──

export const WIDGET_DEFAULTS: Required<WidgetConfig> = {
  model: "",
  welcomeMessage: "Hello! How can I help you today?",
  primaryColor: "#6366f1",
  position: "bottom-right",
  title: "Xergon Chat",
  publicKey: "",
};

// ── Validation ──

const HEX_COLOR_RE = /^#([0-9a-f]{3}|[0-9a-f]{6})$/i;

export function isValidHexColor(color: string): boolean {
  return HEX_COLOR_RE.test(color);
}

export function sanitizeColor(color: string | undefined): string {
  if (!color) return WIDGET_DEFAULTS.primaryColor;
  return isValidHexColor(color) ? color : WIDGET_DEFAULTS.primaryColor;
}

export function sanitizePosition(
  position: string | undefined,
): WidgetPosition {
  if (position === "bottom-left") return "bottom-left";
  return "bottom-right";
}

// ── CSP ──

export function generateCSP(allowedOrigins: string = "*"): string {
  const directives = [
    "default-src 'none'",
    `frame-ancestors ${allowedOrigins === "*" ? "*" : allowedOrigins.split(",").join(" ")}`,
    "script-src 'self' 'unsafe-inline' 'unsafe-eval'",
    "style-src 'self' 'unsafe-inline'",
    "img-src 'self' data: https:",
    "font-src 'self' data:",
    "connect-src 'self'",
  ];
  return directives.join("; ");
}

// ── Allowed origins ──

export const ALLOWED_ORIGINS = "*";

// ── Helpers ──

export function mergeConfig(query: Record<string, string | undefined>): Required<WidgetConfig> {
  return {
    model: query.model || WIDGET_DEFAULTS.model,
    welcomeMessage: query.welcome || WIDGET_DEFAULTS.welcomeMessage,
    primaryColor: sanitizeColor(query.color),
    position: sanitizePosition(query.position),
    title: query.title || WIDGET_DEFAULTS.title,
    publicKey: query.pk || WIDGET_DEFAULTS.publicKey,
  };
}
