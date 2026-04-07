import type { Metadata } from "next";
import { OperatorShell } from "@/components/operator/OperatorShell";

export const metadata: Metadata = {
  title: "Operator Dashboard - Xergon Network",
  description:
    "Manage GPU nodes, monitor providers, view events and alerts on the Xergon operator dashboard.",
  robots: { index: false, follow: false },
};

export default function OperatorLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return <OperatorShell>{children}</OperatorShell>;
}
