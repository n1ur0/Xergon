import type { Metadata } from "next";
import { fetchProviders } from "@/lib/api/chain";
import { ProvidersDirectoryClient } from "./ProvidersDirectoryClient";

export const metadata: Metadata = {
  title: "Providers - Xergon Network",
  description:
    "Browse compute providers on the Xergon marketplace. Filter by verification, tier, region, and uptime. Find the best provider for your AI inference needs.",
  openGraph: {
    title: "Providers - Xergon Network",
    description:
      "Browse compute providers on the Xergon marketplace. Filter by verification, tier, region, and uptime.",
    url: "/providers",
    type: "website",
  },
  twitter: {
    card: "summary_large_image",
    title: "Providers - Xergon Network",
    description: "Browse compute providers on the Xergon marketplace.",
  },
  alternates: { canonical: "/providers" },
};

export default async function ProvidersPage() {
  const providers = await fetchProviders();

  return (
    <div className="max-w-6xl mx-auto px-4 py-8">
      <div className="mb-8">
        <h1 className="text-2xl font-bold mb-2">Providers</h1>
        <p className="text-surface-800/60">
          Browse compute providers on the Xergon network. Filter by status, region, and models offered.
        </p>
      </div>
      <ProvidersDirectoryClient providers={providers} />
    </div>
  );
}
