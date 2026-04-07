"use client";

import { useState, useEffect, useCallback, useRef, type ReactNode } from "react";
import { useRouter } from "next/navigation";
import {
  ChevronLeft,
  ChevronRight,
  X,
  PartyPopper,
  Loader2,
  CheckCircle2,
} from "lucide-react";
import WelcomeStep, { type AccountType } from "./WelcomeStep";
import WalletStep from "./WalletStep";
import ProfileStep, { type Tag } from "./ProfileStep";
import ProviderSetupStep, { type PricingModel, type SlaTier } from "./ProviderSetupStep";
import PreferencesStep from "./PreferencesStep";
import type { Theme } from "@/lib/stores/theme";
import type { Locale } from "@/lib/i18n/config";

// ── Constants ──────────────────────────────────────────────────────────────

const STORAGE_KEY = "xergon_onboarding_progress";
const COMPLETED_KEY = "xergon_onboarding_completed";
const TOTAL_STEPS = 5;

// ── Types ──────────────────────────────────────────────────────────────────

interface OnboardingData {
  currentStep: number;
  accountType: AccountType | null;
  wallet: {
    connected: boolean;
    address: string | null;
    balance: number | null;
  };
  profile: {
    displayName: string;
    bio: string;
    avatarUrl: string;
    tags: Tag[];
    website: string;
    twitter: string;
    github: string;
  };
  provider: {
    endpointUrl: string;
    models: string[];
    pricingModel: PricingModel;
    slaTier: SlaTier;
    gpuType: string;
    vram: string;
    cpu: string;
    ram: string;
    region: string;
  };
  preferences: {
    defaultModel: string;
    notifications: {
      email: boolean;
      push: boolean;
      telegram: boolean;
    };
    theme: Theme;
    language: Locale;
    privacyProfile: boolean;
    privacyActivity: boolean;
  };
}

const DEFAULT_DATA: OnboardingData = {
  currentStep: 0,
  accountType: null,
  wallet: { connected: false, address: null, balance: null },
  profile: {
    displayName: "",
    bio: "",
    avatarUrl: "",
    tags: [],
    website: "",
    twitter: "",
    github: "",
  },
  provider: {
    endpointUrl: "",
    models: [],
    pricingModel: "per-token",
    slaTier: "standard",
    gpuType: "",
    vram: "",
    cpu: "",
    ram: "",
    region: "",
  },
  preferences: {
    defaultModel: "Auto (best available)",
    notifications: { email: false, push: true, telegram: false },
    theme: "system",
    language: "en",
    privacyProfile: true,
    privacyActivity: false,
  },
};

// ── Step definitions ───────────────────────────────────────────────────────

interface StepDef {
  title: string;
  canSkip: boolean;
  validate: (data: OnboardingData) => boolean;
}

const STEPS: StepDef[] = [
  {
    title: "Welcome",
    canSkip: false,
    validate: (d) => d.accountType !== null,
  },
  {
    title: "Wallet",
    canSkip: true,
    validate: (d) => d.wallet.connected,
  },
  {
    title: "Profile",
    canSkip: true,
    validate: (d) => d.profile.displayName.trim().length > 0,
  },
  {
    title: "Provider",
    canSkip: true,
    validate: (d) => {
      // Only required if accountType is provider or both
      if (d.accountType === "consumer") return true;
      return d.provider.endpointUrl.trim().length > 0 && d.provider.models.length > 0;
    },
  },
  {
    title: "Preferences",
    canSkip: true,
    validate: () => true, // Always valid — all fields have defaults
  },
];

// ── Main Component ─────────────────────────────────────────────────────────

export default function OnboardingWizard() {
  const router = useRouter();
  const [data, setData] = useState<OnboardingData>(DEFAULT_DATA);
  const [direction, setDirection] = useState<"forward" | "back">("forward");
  const [isCompleting, setIsCompleting] = useState(false);
  const [showSuccess, setShowSuccess] = useState(false);
  const initialized = useRef(false);

  // Load saved progress
  useEffect(() => {
    if (initialized.current) return;
    initialized.current = true;
    try {
      const saved = localStorage.getItem(STORAGE_KEY);
      if (saved) {
        const parsed = JSON.parse(saved) as Partial<OnboardingData>;
        setData((prev) => ({ ...prev, ...parsed }));
      }
    } catch {
      // Ignore corrupt data
    }
  }, []);

  // Auto-save on every change
  useEffect(() => {
    if (!initialized.current) return;
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(data));
    } catch {
      // Storage full or unavailable
    }
  }, [data]);

  // Show success state briefly, then redirect
  useEffect(() => {
    if (!showSuccess) return;
    const timer = setTimeout(() => {
      localStorage.setItem(COMPLETED_KEY, Date.now().toString());
      router.push("/dashboard?onboarding=complete");
    }, 2000);
    return () => clearTimeout(timer);
  }, [showSuccess, router]);

  // ── Navigation ──

  const canProceed = useCallback(() => {
    return STEPS[data.currentStep].validate(data);
  }, [data]);

  const goNext = useCallback(() => {
    if (!canProceed()) return;
    const nextStep = data.currentStep + 1;
    if (nextStep >= TOTAL_STEPS) {
      // Complete
      setIsCompleting(true);
      // Fire-and-forget API call
      fetch("/api/onboarding/complete", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(data),
      }).catch(() => {});
      setShowSuccess(true);
      return;
    }
    setDirection("forward");
    setData((prev) => ({ ...prev, currentStep: nextStep }));
  }, [data, canProceed]);

  const goBack = useCallback(() => {
    if (data.currentStep <= 0) return;
    setDirection("back");
    setData((prev) => ({ ...prev, currentStep: prev.currentStep - 1 }));
  }, [data]);

  const skipOnboarding = useCallback(() => {
    localStorage.setItem(COMPLETED_KEY, Date.now().toString());
    router.push("/dashboard");
  }, [router]);

  const skipStep = useCallback(() => {
    goNext();
  }, [goNext]);

  // ── Update helpers ──

  const updateWallet = useCallback(
    (connected: boolean, address: string | null, balance: number | null) => {
      setData((prev) => ({
        ...prev,
        wallet: { connected, address, balance },
      }));
    },
    []
  );

  const updateProfile = useCallback(
    (update: Partial<OnboardingData["profile"]>) => {
      setData((prev) => ({
        ...prev,
        profile: { ...prev.profile, ...update },
      }));
    },
    []
  );

  const updateProvider = useCallback(
    (update: Partial<OnboardingData["provider"]>) => {
      setData((prev) => ({
        ...prev,
        provider: { ...prev.provider, ...update },
      }));
    },
    []
  );

  const updatePreferences = useCallback(
    (update: Partial<OnboardingData["preferences"]>) => {
      setData((prev) => ({
        ...prev,
        preferences: { ...prev.preferences, ...update },
      }));
    },
    []
  );

  // ── Skip provider step for consumers ──

  const effectiveStep = (() => {
    // If consumer, skip the provider setup step
    if (data.accountType === "consumer" && data.currentStep >= 3) {
      return data.currentStep + 1;
    }
    return data.currentStep;
  })();

  // ── Render ──

  // Success screen
  if (showSuccess) {
    return (
      <div className="flex min-h-dvh items-center justify-center bg-surface-50 dark:bg-surface-950 px-4">
        <div className="text-center space-y-4 animate-fade-in">
          <div className="mx-auto flex h-20 w-20 items-center justify-center rounded-full bg-emerald-100 dark:bg-emerald-900/30">
            <PartyPopper className="h-10 w-10 text-emerald-500" />
          </div>
          <h1 className="text-2xl font-bold text-surface-900 dark:text-surface-0">
            Welcome to Xergon!
          </h1>
          <p className="text-sm text-surface-800/60 dark:text-surface-300/60">
            Your account is set up. Redirecting to the dashboard...
          </p>
          <Loader2 className="mx-auto h-5 w-5 animate-spin text-emerald-500" />
        </div>
      </div>
    );
  }

  const renderStep = (): ReactNode => {
    switch (data.currentStep) {
      case 0:
        return (
          <WelcomeStep
            value={data.accountType}
            onChange={(type) =>
              setData((prev) => ({ ...prev, accountType: type }))
            }
          />
        );
      case 1:
        return (
          <WalletStep value={data.wallet} onChange={updateWallet} />
        );
      case 2:
        return (
          <ProfileStep value={data.profile} onChange={updateProfile} />
        );
      case 3:
        return (
          <ProviderSetupStep
            value={data.provider}
            onChange={updateProvider}
          />
        );
      case 4:
        return (
          <PreferencesStep
            value={data.preferences}
            onChange={updatePreferences}
          />
        );
      default:
        return null;
    }
  };

  const progressPercent = ((data.currentStep + 1) / TOTAL_STEPS) * 100;
  const isLastStep = data.currentStep === TOTAL_STEPS - 1;
  const stepDef = STEPS[data.currentStep];

  return (
    <div className="flex min-h-dvh flex-col bg-surface-50 dark:bg-surface-950">
      {/* Top bar */}
      <div className="border-b border-surface-200 bg-surface-0 dark:border-surface-800 dark:bg-surface-900">
        <div className="mx-auto flex max-w-3xl items-center justify-between px-4 py-3">
          <div className="flex items-center gap-2">
            <span className="text-lg font-bold text-surface-900 dark:text-surface-0">
              Xergon
            </span>
          </div>
          <button
            type="button"
            onClick={skipOnboarding}
            className="flex items-center gap-1 rounded-lg px-3 py-1.5 text-sm text-surface-500 transition-colors hover:bg-surface-100 hover:text-surface-700 dark:hover:bg-surface-800 dark:hover:text-surface-300"
          >
            <X className="h-4 w-4" />
            Skip
          </button>
        </div>

        {/* Progress bar */}
        <div className="mx-auto max-w-3xl px-4 pb-3">
          <div className="flex items-center gap-2 mb-2">
            <div className="flex-1 h-1.5 rounded-full bg-surface-200 dark:bg-surface-700 overflow-hidden">
              <div
                className="h-full rounded-full bg-gradient-to-r from-emerald-500 to-teal-500 transition-all duration-500 ease-out"
                style={{ width: `${progressPercent}%` }}
              />
            </div>
            <span className="text-xs font-medium text-surface-500 tabular-nums">
              {data.currentStep + 1}/{TOTAL_STEPS}
            </span>
          </div>
          {/* Step indicators */}
          <div className="flex items-center justify-between">
            {STEPS.map((step, i) => {
              const isCompleted = i < data.currentStep;
              const isCurrent = i === data.currentStep;
              // Hide provider step for consumers
              if (i === 3 && data.accountType === "consumer") return null;
              return (
                <div key={step.title} className="flex flex-col items-center gap-1">
                  <div
                    className={`flex h-7 w-7 items-center justify-center rounded-full text-xs font-medium transition-all ${
                      isCompleted
                        ? "bg-emerald-500 text-white"
                        : isCurrent
                          ? "bg-emerald-500/20 text-emerald-600 ring-2 ring-emerald-500 dark:bg-emerald-500/20 dark:text-emerald-400"
                          : "bg-surface-200 text-surface-500 dark:bg-surface-700 dark:text-surface-400"
                    }`}
                  >
                    {isCompleted ? (
                      <CheckCircle2 className="h-4 w-4" />
                    ) : (
                      i + 1
                    )}
                  </div>
                  <span
                    className={`text-[10px] font-medium ${
                      isCurrent
                        ? "text-emerald-600 dark:text-emerald-400"
                        : "text-surface-400 dark:text-surface-500"
                    }`}
                  >
                    {step.title}
                  </span>
                </div>
              );
            })}
          </div>
        </div>
      </div>

      {/* Step content with animation */}
      <div className="flex-1 overflow-y-auto">
        <div className="mx-auto max-w-3xl px-4 py-8">
          <div
            key={data.currentStep}
            className={`animate-slide-${direction}`}
          >
            {renderStep()}
          </div>
        </div>
      </div>

      {/* Bottom navigation */}
      <div className="border-t border-surface-200 bg-surface-0 dark:border-surface-800 dark:bg-surface-900">
        <div className="mx-auto flex max-w-3xl items-center justify-between px-4 py-4">
          <button
            type="button"
            onClick={goBack}
            disabled={data.currentStep === 0}
            className="inline-flex items-center gap-1.5 rounded-lg border border-surface-300 px-4 py-2.5 text-sm font-medium text-surface-700 transition-colors hover:bg-surface-100 disabled:invisible dark:border-surface-600 dark:text-surface-300 dark:hover:bg-surface-800"
          >
            <ChevronLeft className="h-4 w-4" />
            Back
          </button>

          <div className="flex items-center gap-2">
            {stepDef.canSkip && (
              <button
                type="button"
                onClick={skipStep}
                className="rounded-lg px-4 py-2.5 text-sm font-medium text-surface-500 transition-colors hover:bg-surface-100 hover:text-surface-700 dark:hover:bg-surface-800 dark:hover:text-surface-300"
              >
                Skip this step
              </button>
            )}
            <button
              type="button"
              onClick={goNext}
              disabled={!canProceed() || isCompleting}
              className="inline-flex items-center gap-1.5 rounded-lg bg-emerald-600 px-5 py-2.5 text-sm font-medium text-white shadow-sm transition-colors hover:bg-emerald-700 disabled:opacity-50 disabled:cursor-not-allowed"
            >
              {isCompleting ? (
                <>
                  <Loader2 className="h-4 w-4 animate-spin" />
                  Finishing...
                </>
              ) : isLastStep ? (
                <>
                  <CheckCircle2 className="h-4 w-4" />
                  Complete
                </>
              ) : (
                <>
                  Next
                  <ChevronRight className="h-4 w-4" />
                </>
              )}
            </button>
          </div>
        </div>
      </div>

      {/* Inline animation keyframes */}
      <style jsx global>{`
        @keyframes slide-forward {
          from {
            opacity: 0;
            transform: translateX(24px);
          }
          to {
            opacity: 1;
            transform: translateX(0);
          }
        }
        @keyframes slide-back {
          from {
            opacity: 0;
            transform: translateX(-24px);
          }
          to {
            opacity: 1;
            transform: translateX(0);
          }
        }
        .animate-slide-forward {
          animation: slide-forward 0.3s ease-out;
        }
        .animate-slide-back {
          animation: slide-back 0.3s ease-out;
        }
        @keyframes fade-in {
          from { opacity: 0; transform: scale(0.95); }
          to { opacity: 1; transform: scale(1); }
        }
        .animate-fade-in {
          animation: fade-in 0.4s ease-out;
        }
      `}</style>
    </div>
  );
}
