import type { Metadata } from "next";
import OnboardingWizard from "@/components/onboarding/OnboardingWizard";
import { OnboardingGuard } from "./OnboardingGuard";

export const metadata: Metadata = {
  title: "Get Started",
  description:
    "Set up your Xergon account and preferences to start using the decentralized AI marketplace.",
};

export default function OnboardingPage() {
  return (
    <OnboardingGuard>
      <OnboardingWizard />
    </OnboardingGuard>
  );
}
