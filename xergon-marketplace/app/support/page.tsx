import type { Metadata } from "next";
import SupportCenter from "@/components/support/SupportCenter";

export const metadata: Metadata = {
  title: "Support Center | Xergon",
  description:
    "Get help with Xergon Network. Browse FAQs, knowledge base articles, or submit a support ticket.",
  openGraph: {
    title: "Support Center | Xergon Network",
    description:
      "Find answers, browse articles, or get help from our team.",
    url: "/support",
    type: "website",
  },
  twitter: {
    card: "summary",
    title: "Support Center | Xergon Network",
    description: "Find answers, browse articles, or get help from our team.",
  },
  alternates: { canonical: "/support" },
};

export default function SupportPage() {
  return <SupportCenter />;
}
