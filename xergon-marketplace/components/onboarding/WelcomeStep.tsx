"use client";

import { ShieldCheck, Server, ArrowLeftRight, Sparkles } from "lucide-react";

export type AccountType = "consumer" | "provider" | "both";

interface WelcomeStepProps {
  value: AccountType | null;
  onChange: (type: AccountType) => void;
}

const accountOptions: {
  type: AccountType;
  icon: React.ReactNode;
  title: string;
  description: string;
  benefits: string[];
}[] = [
  {
    type: "consumer",
    icon: <Sparkles className="h-6 w-6" />,
    title: "Consumer",
    description: "Browse and use AI models",
    benefits: [
      "Access 50+ open-source models",
      "Pay-per-token with ERG",
      "Playground for testing",
      "No vendor lock-in",
    ],
  },
  {
    type: "provider",
    icon: <Server className="h-6 w-6" />,
    title: "Provider",
    description: "Host and sell model inference",
    benefits: [
      "Earn ERG for compute",
      "Set your own pricing",
      "Provider analytics dashboard",
      "Decentralized reputation",
    ],
  },
  {
    type: "both",
    icon: <ArrowLeftRight className="h-6 w-6" />,
    title: "Both",
    description: "Use models and provide compute",
    benefits: [
      "Full marketplace access",
      "Consumer & provider tools",
      "Earnings management",
      "Complete Xergon experience",
    ],
  },
];

export default function WelcomeStep({ value, onChange }: WelcomeStepProps) {
  return (
    <div className="space-y-8">
      {/* Branding */}
      <div className="text-center space-y-3">
        <div className="mx-auto flex h-16 w-16 items-center justify-center rounded-2xl bg-gradient-to-br from-emerald-500 to-teal-600 shadow-lg shadow-emerald-500/20">
          <ShieldCheck className="h-8 w-8 text-white" />
        </div>
        <h1 className="text-2xl font-bold text-surface-900 dark:text-surface-0">
          Welcome to Xergon
        </h1>
        <p className="text-sm text-surface-800/60 dark:text-surface-300/60 max-w-md mx-auto">
          GPU-first AI inference marketplace powered by the Ergo blockchain.
          Choose how you want to participate to get started.
        </p>
      </div>

      {/* Account type cards */}
      <div className="grid grid-cols-1 sm:grid-cols-3 gap-4">
        {accountOptions.map((opt) => {
          const isSelected = value === opt.type;
          return (
            <button
              key={opt.type}
              type="button"
              onClick={() => onChange(opt.type)}
              className={`relative flex flex-col items-start gap-3 rounded-xl border-2 p-5 text-left transition-all duration-200 ${
                isSelected
                  ? "border-emerald-500 bg-emerald-50 dark:bg-emerald-950/30 shadow-md shadow-emerald-500/10"
                  : "border-surface-200 bg-surface-0 hover:border-surface-300 hover:shadow-sm dark:border-surface-700 dark:bg-surface-900 dark:hover:border-surface-600"
              }`}
            >
              {isSelected && (
                <div className="absolute top-3 right-3 h-5 w-5 rounded-full bg-emerald-500 flex items-center justify-center">
                  <svg className="h-3 w-3 text-white" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={3}>
                    <path strokeLinecap="round" strokeLinejoin="round" d="M5 13l4 4L19 7" />
                  </svg>
                </div>
              )}
              <div
                className={`flex h-10 w-10 items-center justify-center rounded-lg ${
                  isSelected
                    ? "bg-emerald-500 text-white"
                    : "bg-surface-100 text-surface-600 dark:bg-surface-800 dark:text-surface-400"
                }`}
              >
                {opt.icon}
              </div>
              <div>
                <h3 className="font-semibold text-surface-900 dark:text-surface-0">
                  {opt.title}
                </h3>
                <p className="text-xs text-surface-800/60 dark:text-surface-300/60 mt-0.5">
                  {opt.description}
                </p>
              </div>
              <ul className="space-y-1.5 w-full">
                {opt.benefits.map((b) => (
                  <li
                    key={b}
                    className="flex items-center gap-1.5 text-xs text-surface-700 dark:text-surface-400"
                  >
                    <span className={`h-1 w-1 rounded-full shrink-0 ${isSelected ? "bg-emerald-500" : "bg-surface-300 dark:bg-surface-600"}`} />
                    {b}
                  </li>
                ))}
              </ul>
            </button>
          );
        })}
      </div>
    </div>
  );
}
