import type { Metadata } from "next";
import { notFound } from "next/navigation";
import { fetchProviders } from "@/lib/api/chain";
import { ProviderDetailClient } from "./ProviderDetailClient";

interface ProviderPageProps {
  params: Promise<{ id: string }>;
}

export async function generateMetadata({ params }: ProviderPageProps): Promise<Metadata> {
  const { id } = await params;
  const providers = await fetchProviders();
  const provider = providers.find((p) => p.provider_id === id);

  if (!provider) {
    return { title: "Provider Not Found | Xergon Network" };
  }

  const displayName = id.length > 16 ? `${id.slice(0, 8)}...${id.slice(-6)}` : id;

  return {
    title: `${displayName} - Provider | Xergon Network`,
    description: `View details for provider ${displayName} on Xergon Network. Models offered: ${provider.models.join(", ")}. Region: ${provider.region}.`,
    openGraph: {
      title: `${displayName} - Provider | Xergon Network`,
      description: `Provider ${displayName} serving ${provider.models.length} models on Xergon Network.`,
      url: `/providers/${id}`,
      type: "website",
    },
    twitter: {
      card: "summary",
      title: `${displayName} - Provider | Xergon Network`,
      description: `Provider ${displayName} on Xergon Network.`,
    },
  };
}

export default async function ProviderPage({ params }: ProviderPageProps) {
  const { id } = await params;
  const providers = await fetchProviders();
  const provider = providers.find((p) => p.provider_id === id);

  if (!provider) {
    notFound();
  }

  return <ProviderDetailClient provider={provider} />;
}
