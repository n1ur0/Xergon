import { Metadata } from "next";
import ProviderInsights from "@/components/insights/ProviderInsights";

export const metadata: Metadata = {
  title: "Provider Insights | Xergon",
  description: "Market intelligence and provider insights",
};

export default function Page() {
  return <ProviderInsights />;
}
