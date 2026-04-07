import type { Metadata } from "next";
import { fetchModels } from "@/lib/api/chain";
import { FALLBACK_MODELS } from "@/lib/constants";
import { ModelDetailClient } from "./ModelDetailClient";

interface ModelPageProps {
  params: Promise<{ id: string }>;
}

export async function generateMetadata({ params }: ModelPageProps): Promise<Metadata> {
  const { id } = await params;
  const chainModels = await fetchModels();
  const model = chainModels.find((m) => m.id === id) ?? FALLBACK_MODELS.find((m) => m.id === id);

  if (!model) {
    return { title: "Model Not Found | Xergon Network" };
  }

  return {
    title: `${model.name} - Model Details | Xergon Network`,
    description: model.description ?? `Details for ${model.name} on Xergon Network. View benchmarks, pricing, and try it in the playground.`,
    openGraph: {
      title: `${model.name} | Xergon Network`,
      description: model.description ?? `AI model ${model.name} on Xergon Network.`,
      url: `/models/${id}`,
      type: "website",
    },
    twitter: {
      card: "summary",
      title: `${model.name} | Xergon Network`,
      description: model.description ?? `AI model ${model.name} on Xergon Network.`,
    },
  };
}

export default async function ModelDetailPage({ params }: ModelPageProps) {
  const { id } = await params;
  const chainModels = await fetchModels();
  const chainModel = chainModels.find((m) => m.id === id);
  const fallbackModel = FALLBACK_MODELS.find((m) => m.id === id);

  // Merge chain data with fallback data
  const model = {
    id,
    name: chainModel?.name ?? fallbackModel?.name ?? id,
    provider: chainModel?.provider ?? fallbackModel?.provider ?? "Unknown",
    tier: chainModel?.tier ?? fallbackModel?.tier ?? "standard",
    description: chainModel?.description ?? fallbackModel?.description,
    contextWindow: chainModel?.context_window ?? fallbackModel?.contextWindow,
    speed: chainModel?.speed ?? fallbackModel?.speed,
    tags: chainModel?.tags ?? fallbackModel?.tags ?? [],
    freeTier: chainModel?.free_tier ?? fallbackModel?.freeTier ?? false,
    pricePerInputTokenNanoerg: chainModel?.price_per_input_token_nanoerg ?? fallbackModel?.pricePerInputTokenNanoerg ?? 0,
    pricePerOutputTokenNanoerg: chainModel?.price_per_output_token_nanoerg ?? fallbackModel?.pricePerOutputTokenNanoerg ?? 0,
    effectivePriceNanoerg: chainModel?.effective_price_nanoerg ?? fallbackModel?.effectivePriceNanoerg,
    providerCount: chainModel?.provider_count ?? fallbackModel?.providerCount ?? 0,
    available: chainModel?.available ?? fallbackModel?.available ?? true,
  };

  return <ModelDetailClient model={model} />;
}
