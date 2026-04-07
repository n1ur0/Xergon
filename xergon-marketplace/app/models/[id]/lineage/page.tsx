import type { Metadata } from "next";
import { ModelLineageGraph } from "@/components/lineage/ModelLineageGraph";

interface LineagePageProps {
  params: Promise<{ id: string }>;
}

export async function generateMetadata({ params }: LineagePageProps): Promise<Metadata> {
  const { id } = await params;
  return {
    title: `Model Lineage - ${id} | Xergon Network`,
    description: `View the lineage and evolution history for model ${id} on Xergon Network.`,
    openGraph: {
      title: `Model Lineage - ${id} | Xergon Network`,
      description: `View the lineage and evolution history for model ${id} on Xergon Network.`,
      url: `/models/${id}/lineage`,
      type: "website",
    },
  };
}

export default async function ModelLineagePage({ params }: LineagePageProps) {
  const { id } = await params;

  return (
    <div className="max-w-7xl mx-auto px-4 py-8">
      <div className="mb-6">
        <h1 className="text-2xl font-bold text-surface-900">Model Lineage</h1>
        <p className="text-sm text-surface-800/50 mt-0.5">
          Track how <span className="font-mono font-medium text-surface-900">{id}</span> was derived, fine-tuned, merged, and evolved
        </p>
      </div>
      <ModelLineageGraph modelId={id} />
    </div>
  );
}
