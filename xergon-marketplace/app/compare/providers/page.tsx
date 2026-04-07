import type { Metadata } from "next";
import ProviderComparisonTable from "@/components/providers/ProviderComparisonTable";

export const metadata: Metadata = {
  title: "Provider Comparison | Xergon",
  description:
    "Compare inference providers side-by-side on Xergon. See performance, pricing, features, and ratings.",
  openGraph: {
    title: "Provider Comparison | Xergon Network",
    description:
      "Compare inference providers across latency, throughput, reliability, cost, and features.",
    url: "/compare/providers",
    type: "website",
  },
  twitter: {
    card: "summary",
    title: "Provider Comparison | Xergon Network",
    description:
      "Compare inference providers across latency, throughput, reliability, cost, and features.",
  },
  alternates: { canonical: "/compare/providers" },
};

export default function ProviderComparisonPage() {
  return <ProviderComparisonTable />;
}
