import { Metadata } from "next";
import RequestAnalytics from "@/components/analytics/RequestAnalytics";

export const metadata: Metadata = {
  title: "Request Analytics | Xergon",
  description: "Monitor inference request analytics",
};

export default function Page() {
  return <RequestAnalytics />;
}
