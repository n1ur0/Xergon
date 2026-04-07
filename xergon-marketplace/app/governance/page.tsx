import type { Metadata } from "next";
import { GovernanceDashboard } from "@/components/governance/GovernanceDashboard";

export const metadata: Metadata = {
  title: "Governance | Xergon Network",
  description: "Participate in Xergon Network governance. Vote on proposals, create new proposals, and shape the future of the marketplace.",
  openGraph: {
    title: "Governance | Xergon Network",
    description: "Participate in Xergon Network governance. Vote on proposals and shape the marketplace.",
    url: "/governance",
    type: "website",
  },
};

export default function GovernancePage() {
  return <GovernanceDashboard />;
}
