import type { Metadata } from "next";

export const metadata: Metadata = {
  title: "Getting Started - Xergon Docs",
  description:
    "Get started with the Xergon AI marketplace in minutes. Connect your Ergo wallet, get an API key, and make your first inference request. SDK examples in TypeScript, Python, and cURL.",
  openGraph: {
    title: "Getting Started - Xergon Docs",
    description:
      "Get started with Xergon in minutes. Connect your wallet, get an API key, and make your first request.",
    url: "/docs/getting-started",
    type: "website",
  },
  twitter: {
    card: "summary",
    title: "Getting Started - Xergon Docs",
    description:
      "Get started with Xergon in minutes. Connect your wallet, get an API key, and make your first request.",
  },
  alternates: { canonical: "/docs/getting-started" },
};

export default function GettingStartedLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return children;
}
