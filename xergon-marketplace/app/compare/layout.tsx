import type { Metadata } from "next";

export const metadata: Metadata = {
  title: "Model Comparison - Xergon Network",
  description:
    "Compare AI models side-by-side on Xergon. See pricing, latency, throughput, availability, and context windows across providers.",
  openGraph: {
    title: "Model Comparison - Xergon Network",
    description:
      "Compare AI models side-by-side. See pricing, latency, throughput, and availability.",
    url: "/compare",
    type: "website",
  },
  twitter: {
    card: "summary",
    title: "Model Comparison - Xergon Network",
    description:
      "Compare AI models side-by-side. See pricing, latency, throughput, and availability.",
  },
  alternates: { canonical: "/compare" },
};

export default function CompareLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return children;
}
