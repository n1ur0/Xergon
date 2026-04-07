import type { Metadata } from "next";
import { BillingDashboard } from "@/components/billing/BillingDashboard";

export const metadata: Metadata = {
  title: "Billing Dashboard | Xergon Network",
  description: "Track spending, manage invoices, and monitor usage across the Xergon marketplace.",
  openGraph: {
    title: "Billing Dashboard | Xergon Network",
    description: "Track spending, manage invoices, and monitor usage across the Xergon marketplace.",
    url: "/billing",
    type: "website",
  },
};

export default function BillingPage() {
  return <BillingDashboard />;
}
