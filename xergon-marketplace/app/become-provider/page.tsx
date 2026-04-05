"use client";

import { useAuthStore } from "@/lib/stores/auth";
import { useRouter } from "next/navigation";
import Link from "next/link";

export default function BecomeProviderPage() {
  const isAuthenticated = useAuthStore((s) => s.isAuthenticated);
  const user = useAuthStore((s) => s.user);
  const router = useRouter();

  const hasWallet = !!(user?.ergoAddress);

  function handleGetStarted() {
    if (!isAuthenticated) {
      router.push("/signin");
    } else if (!hasWallet) {
      router.push("/settings");
    } else {
      router.push("/provider");
    }
  }

  const steps = [
    {
      number: "1",
      title: "Link Your Wallet",
      description:
        "Link an Ergo wallet in Settings to receive payments for inference work.",
    },
    {
      number: "2",
      title: "Run Xergon Agent",
      description:
        "Download and run xergon-agent on your hardware with an LLM backend of your choice.",
    },
    {
      number: "3",
      title: "Earn ERG",
      description:
        "Automatically receive payments for every inference request served through the network.",
    },
  ];

  const requirements = [
    { icon: "👛", label: "Ergo wallet", detail: "for payments" },
    {
      icon: "🖥️",
      label: "NVIDIA/AMD GPU or Apple Silicon",
      detail: "for inference",
    },
    { icon: "💾", label: "At least 8 GB VRAM", detail: "recommended" },
    {
      icon: "🌐",
      label: "Internet connection",
      detail: "with stable uptime",
    },
  ];

  return (
    <div className="min-h-screen bg-surface-950 text-surface-200">
      {/* Hero */}
      <section className="mx-auto max-w-3xl px-4 pt-20 pb-16 text-center">
        <h1 className="text-4xl font-bold tracking-tight sm:text-5xl">
          Become a Compute Provider
        </h1>
        <p className="mt-4 text-lg text-surface-200/70">
          Earn ERG by providing AI inference capacity to the Xergon Network.
          Turn your idle GPU into a revenue stream.
        </p>
      </section>

      {/* How it works */}
      <section className="mx-auto max-w-4xl px-4 pb-20">
        <h2 className="mb-10 text-center text-2xl font-semibold">
          How It Works
        </h2>

        <div className="grid gap-6 md:grid-cols-3">
          {steps.map((step) => (
            <div
              key={step.number}
              className="rounded-xl border border-surface-800 bg-surface-800 p-6"
            >
              <span className="flex h-10 w-10 items-center justify-center rounded-full bg-brand-600 text-sm font-bold text-white">
                {step.number}
              </span>
              <h3 className="mt-4 text-lg font-semibold">{step.title}</h3>
              <p className="mt-2 text-sm text-surface-200/70">
                {step.description}
              </p>
            </div>
          ))}
        </div>
      </section>

      {/* Requirements */}
      <section className="mx-auto max-w-3xl px-4 pb-20">
        <h2 className="mb-8 text-center text-2xl font-semibold">
          Requirements
        </h2>

        <div className="rounded-xl border border-surface-800 bg-surface-800 p-6">
          <ul className="space-y-4">
            {requirements.map((req) => (
              <li key={req.label} className="flex items-start gap-3">
                <span className="mt-0.5 text-xl leading-none">{req.icon}</span>
                <div>
                  <span className="font-medium">{req.label}</span>
                  <span className="text-surface-200/60"> — {req.detail}</span>
                </div>
              </li>
            ))}
          </ul>
        </div>
      </section>

      {/* CTA */}
      <section className="mx-auto max-w-3xl px-4 pb-12 text-center">
        <button
          onClick={handleGetStarted}
          className="inline-block rounded-lg bg-brand-600 px-8 py-3 text-base font-semibold text-white transition-colors hover:bg-brand-700"
        >
          Get Started
        </button>
        <p className="mt-3 text-sm text-surface-200/50">
          {!isAuthenticated
            ? "You'll need to connect your wallet first."
            : !hasWallet
              ? "Your wallet is connected — head to the Provider Dashboard."
              : "Your wallet is linked — head to the Provider Dashboard."}
        </p>
      </section>

      {/* GitHub link */}
      <section className="mx-auto max-w-3xl px-4 pb-20 text-center">
        <p className="text-sm text-surface-200/50">
          Need help setting up your agent? Check out the{" "}
          <Link
            href="https://github.com/n1ur0/Xergon-Network"
            target="_blank"
            rel="noopener noreferrer"
            className="text-brand-600 hover:text-brand-700 underline underline-offset-2 transition-colors"
          >
            Xergon Network GitHub repo
          </Link>{" "}
          for full setup documentation.
        </p>
      </section>
    </div>
  );
}
