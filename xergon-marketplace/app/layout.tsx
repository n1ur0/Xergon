import type { Metadata, Viewport } from "next";
import "./globals.css";
import { AppShell } from "@/components/AppShell";
import { Toaster } from "sonner";
import { ThemeProvider } from "@/components/ThemeProvider";
import { LocaleInit } from "@/components/LocaleInit";
import { ErrorBoundary } from "@/components/error/ErrorBoundary";
import { ServiceWorkerRegister } from "@/components/ServiceWorkerRegister";
import { ChatWidgetWrapper } from "@/components/chat/ChatWidgetWrapper";

export const viewport: Viewport = {
  width: "device-width",
  initialScale: 1,
  maximumScale: 1,
  viewportFit: "cover",
  themeColor: [
    { media: "(prefers-color-scheme: light)", color: "#ffffff" },
    { media: "(prefers-color-scheme: dark)", color: "#0f172a" },
  ],
};

const SITE_URL =
  process.env.NEXT_PUBLIC_SITE_URL || "https://xergon.network";

export const metadata: Metadata = {
  title: {
    default: "Xergon Marketplace",
    template: "%s | Xergon Network",
  },
  description:
    "GPU-first AI inference marketplace. Pay with ERG, powered by the Ergo blockchain. Access open-source AI models with transparent, pay-per-token pricing on a trustless decentralized network.",
  keywords: [
    "AI marketplace",
    "decentralized AI",
    "GPU inference",
    "Ergo blockchain",
    "ERG",
    "open-source AI models",
    "Llama",
    "Mistral",
    "Qwen",
    "DeepSeek",
    "pay-per-token",
    "Xergon",
  ],
  authors: [{ name: "Xergon Network" }],
  creator: "Xergon Network",
  metadataBase: new URL(SITE_URL),
  alternates: {
    canonical: "/",
    languages: {
      en: "/",
      ja: "/?lang=ja",
      zh: "/?lang=zh",
      es: "/?lang=es",
    },
  },
  openGraph: {
    type: "website",
    locale: "en_US",
    url: SITE_URL,
    siteName: "Xergon Network",
    title: "Xergon Network - Decentralized AI Compute Marketplace",
    description:
      "Access open-source AI models powered by the Ergo blockchain. No lock-in, no middlemen — transparent, pay-per-token inference on a trustless network.",
    images: [
      {
        url: "/og-image.png",
        width: 1200,
        height: 630,
        alt: "Xergon Network - Decentralized AI Compute Marketplace",
      },
    ],
  },
  twitter: {
    card: "summary_large_image",
    title: "Xergon Network - Decentralized AI Compute Marketplace",
    description:
      "Access open-source AI models powered by the Ergo blockchain. Pay-per-token inference, no lock-in.",
    images: ["/og-image.png"],
    creator: "@xergonnetwork",
  },
  robots: {
    index: true,
    follow: true,
    googleBot: {
      index: true,
      follow: true,
      "max-video-preview": -1,
      "max-image-preview": "large",
      "max-snippet": -1,
    },
  },
  appleWebApp: {
    capable: true,
    statusBarStyle: "default",
    title: "Xergon",
  },
  formatDetection: {
    telephone: false,
  },
};

/**
 * Inline script injected into <head> to set the theme class on <html>
 * BEFORE React hydrates, preventing flash-of-wrong-theme (FOUC).
 * Reads the persisted theme from localStorage and resolves it.
 * Also sets the lang attribute from persisted locale.
 */
const themeScript = `
(function(){
  try {
    // Theme
    var stored = JSON.parse(localStorage.getItem('xergon-theme'));
    var theme = (stored && stored.state && stored.state.theme) || 'system';
    var resolved = theme;
    if (theme === 'system') {
      resolved = window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
    }
    document.documentElement.classList.add(resolved);

    // Locale
    var localeStored = JSON.parse(localStorage.getItem('xergon-locale'));
    var lang = (localeStored && localeStored.state && localeStored.state.locale) || 'en';
    if (['en','ja','zh','es'].indexOf(lang) === -1) lang = 'en';
    document.documentElement.lang = lang;
  } catch(e) {
    document.documentElement.classList.add('dark');
  }
})();
`;

export default function RootLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return (
    <html lang="en" suppressHydrationWarning>
      <head>
        <script dangerouslySetInnerHTML={{ __html: themeScript }} />
        <link rel="manifest" href="/manifest.json" />
        <meta name="apple-mobile-web-app-capable" content="yes" />
        <meta name="apple-mobile-web-app-status-bar-style" content="default" />
        <link rel="apple-touch-icon" href="/icons/icon-192.png" />
        <ServiceWorkerRegister />
      </head>
      <body className="min-h-dvh min-h-[100dvh] flex flex-col overscroll-none" style={{ WebkitOverflowScrolling: "touch" }}>
        <ThemeProvider>
          <ErrorBoundary>
            <LocaleInit />
            <AppShell>{children}</AppShell>
          </ErrorBoundary>
        </ThemeProvider>
        <Toaster position="bottom-center" richColors />
        <ChatWidgetWrapper />
      </body>
    </html>
  );
}
