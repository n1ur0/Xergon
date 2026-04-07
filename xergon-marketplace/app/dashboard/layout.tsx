import type { Metadata } from "next";

export const metadata: Metadata = {
  title: "Dashboard - Xergon Network",
  description:
    "View your Xergon usage analytics, spend tracking, API key management, and model usage statistics.",
  robots: { index: false, follow: false },
};

export default function DashboardLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return children;
}
