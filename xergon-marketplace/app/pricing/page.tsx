"use client";

import { useState, useEffect } from "react";
import { useAuthStore } from "@/lib/stores/auth";
import { endpoints } from "@/lib/api/client";
import type { CreditPack } from "@/lib/api/client";
import Link from "next/link";
import { toast } from "sonner";

export default function PricingPage() {
  const isAuthenticated = useAuthStore((s) => s.isAuthenticated);
  const refreshCredits = useAuthStore((s) => s.refreshCredits);
  const [packs, setPacks] = useState<CreditPack[]>([]);
  const [purchasing, setPurchasing] = useState<string | null>(null);
  const [error, setError] = useState("");

  useEffect(() => {
    endpoints
      .getPacks()
      .then((res) => setPacks(res.packs))
      .catch(() => {
        // Fallback packs if API is down
        setPacks([
          { id: "pack_5", amount_usd: 5.0, display_price: "$5.00", bonus_credits_usd: 0.0 },
          { id: "pack_10", amount_usd: 10.0, display_price: "$10.00", bonus_credits_usd: 1.0 },
          { id: "pack_25", amount_usd: 25.0, display_price: "$25.00", bonus_credits_usd: 5.0 },
        ]);
      });
  }, []);

  async function handlePurchase(pack: CreditPack) {
    if (!isAuthenticated) {
      setError("Please sign in to purchase credits");
      return;
    }

    setError("");
    setPurchasing(pack.id);

    try {
      const res = await endpoints.purchaseCredits(pack.id);
      // Redirect to Stripe checkout
      window.location.href = res.checkout_url;
    } catch (err) {
      setError(err instanceof Error ? err.message : "Purchase failed");
      setPurchasing(null);
    }
  }

  return (
    <div className="max-w-4xl mx-auto px-4 py-8">
      <h1 className="text-2xl font-bold mb-2">Pricing</h1>
      <p className="text-surface-800/60 mb-8">
        Pay-per-token with prepaid credits. No subscriptions, no lock-in.
      </p>

      {!isAuthenticated && (
        <div className="mb-6 rounded-lg border border-brand-500/30 bg-brand-500/10 px-4 py-3 text-sm text-brand-700">
          <Link href="/signin" className="font-medium hover:underline">
            Sign in
          </Link>{" "}
          to purchase credits and unlock higher rate limits.
        </div>
      )}

      {error && (
        <div className="mb-6 rounded-lg border border-danger-500/30 bg-danger-500/10 px-4 py-3 text-sm text-danger-600">
          {error}
        </div>
      )}

      <div className="grid gap-6 md:grid-cols-3">
        {packs.map((plan, idx) => {
          const totalCredits = plan.amount_usd + plan.bonus_credits_usd;
          const isHighlight = idx === 1;

          return (
            <div
              key={plan.id}
              className={`rounded-xl border p-6 ${
                isHighlight
                  ? "border-brand-500 ring-2 ring-brand-500/20"
                  : "border-surface-200"
              } bg-surface-0`}
            >
              <h2 className="text-lg font-semibold">
                {plan.amount_usd === 5 ? "Starter" : plan.amount_usd === 10 ? "Standard" : "Pro"}
              </h2>
              <p className="text-sm text-surface-800/50 mt-1">
                {plan.bonus_credits_usd > 0
                  ? `$${plan.bonus_credits_usd.toFixed(0)} bonus credits`
                  : "Try any model"}
              </p>
              <div className="mt-4">
                <span className="text-3xl font-bold">{plan.display_price}</span>
              </div>
              <p className="text-sm text-surface-800/60 mt-1">
                ${totalCredits.toFixed(2)} total credits
              </p>
              <button
                onClick={() => handlePurchase(plan)}
                disabled={purchasing === plan.id}
                className={`mt-6 w-full py-2 rounded-lg font-medium text-sm transition-colors disabled:opacity-50 ${
                  isHighlight
                    ? "bg-brand-600 text-white hover:bg-brand-700"
                    : "bg-surface-100 text-surface-900 hover:bg-surface-200"
                }`}
              >
                {purchasing === plan.id ? "Processing..." : isAuthenticated ? "Add Credits" : "Sign in to buy"}
              </button>
            </div>
          );
        })}
      </div>

      <div className="mt-10 text-sm text-surface-800/50 text-center">
        Credits never expire. Prices shown in USD. Free tier: 10 requests/day anonymous.
      </div>
    </div>
  );
}
