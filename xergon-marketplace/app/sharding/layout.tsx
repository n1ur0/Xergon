import type { Metadata } from "next";

export const metadata: Metadata = {
  title: "Model Sharding Visualizer | Xergon Marketplace",
  description: "Visualize model sharding across GPU providers with pipeline and tensor parallel views.",
};

export default function ShardingLayout({ children }: { children: React.ReactNode }) {
  return (
    <div className="min-h-screen bg-white">
      {children}
    </div>
  );
}
