import type { Metadata } from "next";

export const metadata: Metadata = {
  title: "Pricing - Xergon Network",
  description:
    "No subscriptions. Pay per use with ERG. View live inference pricing, GPU rental costs, and provider rewards on the Xergon decentralized AI marketplace.",
  openGraph: {
    title: "Pricing - Xergon Network",
    description:
      "No subscriptions. Pay per use with ERG. Live inference pricing and GPU rental costs.",
    url: "/pricing",
    type: "website",
  },
  twitter: {
    card: "summary",
    title: "Pricing - Xergon Network",
    description:
      "No subscriptions. Pay per use with ERG. Live inference pricing and GPU rental costs.",
  },
  alternates: { canonical: "/pricing" },
};

export default function PricingLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return children;
}
