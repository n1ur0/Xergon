import type { Metadata } from "next";
import { fetchProviders } from "@/lib/api/chain";
import { ProviderPortfolioComponent } from "@/components/portfolio/ProviderPortfolio";

interface ProviderPortfolioPageProps {
  params: Promise<{ id: string }>;
}

export async function generateMetadata({ params }: ProviderPortfolioPageProps): Promise<Metadata> {
  const { id } = await params;
  const providers = await fetchProviders();
  const provider = providers.find(p => p.provider_id === id);

  if (!provider) {
    return { title: "Provider Not Found | Xergon Network" };
  }

  const displayName = id.length > 16 ? `${id.slice(0, 8)}...${id.slice(-6)}` : id;

  return {
    title: `${displayName} - Portfolio | Xergon Network`,
    description: `View the portfolio for provider ${displayName} on Xergon Network. Models, performance metrics, reviews, and more.`,
    openGraph: {
      title: `${displayName} - Portfolio | Xergon Network`,
      description: `Provider ${displayName} portfolio on Xergon Network. ${provider.models.length} models available.`,
      url: `/providers/${id}/portfolio`,
      type: "website",
    },
    twitter: {
      card: "summary",
      title: `${displayName} - Portfolio | Xergon Network`,
      description: `Provider ${displayName} portfolio on Xergon Network.`,
    },
  };
}

export default async function ProviderPortfolioPage({ params }: ProviderPortfolioPageProps) {
  const { id } = await params;
  const providers = await fetchProviders();
  const provider = providers.find(p => p.provider_id === id);

  if (!provider) {
    return (
      <div className="max-w-6xl mx-auto px-4 py-8 text-center">
        <h1 className="text-2xl font-bold mb-4">Provider Not Found</h1>
        <p className="text-surface-800/50 mb-6">The provider you are looking for does not exist.</p>
        <a href="/providers" className="text-brand-600 hover:text-brand-700">
          Browse all providers
        </a>
      </div>
    );
  }

  return (
    <ProviderPortfolioComponent
      providerId={id}
      // In production, determine ownership from auth context
      isOwner={false}
    />
  );
}
