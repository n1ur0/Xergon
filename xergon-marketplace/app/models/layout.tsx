import type { Metadata } from "next";

export const metadata: Metadata = {
  title: "AI Models - Xergon Network",
  description:
    "Browse available AI models on the Xergon marketplace. Compare Llama, Qwen, Mistral, DeepSeek and more. Filter by speed, capability, and pricing. Try any model instantly.",
  openGraph: {
    title: "AI Models - Xergon Network",
    description:
      "Browse available AI models on the Xergon marketplace. Filter by speed, capability, and pricing. Try any model instantly.",
    url: "/models",
    type: "website",
    images: [{ url: "/og-models.png", width: 1200, height: 630, alt: "Xergon AI Models" }],
  },
  twitter: {
    card: "summary_large_image",
    title: "AI Models - Xergon Network",
    description:
      "Browse available AI models on the Xergon marketplace. Filter by speed, capability, and pricing.",
  },
  alternates: { canonical: "/models" },
};

export default function ModelsLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return children;
}
