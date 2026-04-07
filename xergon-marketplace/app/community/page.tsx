import type { Metadata } from "next";
import { CommunityForumClient } from "./CommunityForumClient";

export const metadata: Metadata = {
  title: "Community Forum - Xergon Network",
  description:
    "Join the Xergon community. Discuss AI models, providers, feature requests, and get support from the community.",
  openGraph: {
    title: "Community Forum - Xergon Network",
    description: "Join the Xergon community forum for discussions, support, and feature requests.",
    url: "/community",
    type: "website",
  },
  twitter: {
    card: "summary_large_image",
    title: "Community Forum - Xergon Network",
    description: "Join the Xergon community forum.",
  },
  alternates: { canonical: "/community" },
};

export default function CommunityPage() {
  return (
    <div className="max-w-4xl mx-auto px-4 py-8">
      <div className="mb-8">
        <h1 className="text-2xl font-bold mb-2">Community Forum</h1>
        <p className="text-surface-800/60">
          Discuss AI models, providers, feature requests, and get support from the community.
        </p>
      </div>
      <CommunityForumClient />
    </div>
  );
}
