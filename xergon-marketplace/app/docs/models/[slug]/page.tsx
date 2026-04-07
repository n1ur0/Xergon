import { Metadata } from "next";
import ModelDocumentation from "@/components/docs/ModelDocumentation";

export async function generateMetadata({
  params,
}: {
  params: Promise<{ slug: string }>;
}): Promise<Metadata> {
  const { slug } = await params;
  const modelName = slug
    .split("-")
    .map((w) => w.charAt(0).toUpperCase() + w.slice(1))
    .join(" ");
  return {
    title: `${modelName} | Model Docs | Xergon`,
    description: `Documentation, benchmarks, and usage examples for ${modelName} on Xergon Marketplace.`,
  };
}

export default async function ModelDocPage({
  params,
}: {
  params: Promise<{ slug: string }>;
}) {
  const { slug } = await params;
  return <ModelDocumentation slug={slug} />;
}
