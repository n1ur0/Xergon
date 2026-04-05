"use client";

import { type GpuPricing } from "@/lib/api/gpu";
import { TrendingUp } from "lucide-react";

interface PricingOverviewProps {
  pricing: GpuPricing[];
}

export function PricingOverview({ pricing }: PricingOverviewProps) {
  if (pricing.length === 0) return null;

  return (
    <div className="rounded-xl border border-surface-200 bg-surface-0 overflow-hidden">
      <div className="px-5 py-3 border-b border-surface-100 flex items-center gap-2">
        <TrendingUp className="w-4 h-4 text-brand-500" />
        <h3 className="font-semibold text-surface-900 text-sm">Market Pricing</h3>
        <span className="text-xs text-surface-800/40 ml-auto">Prices in ERG/hour</span>
      </div>

      <div className="overflow-x-auto">
        <table className="w-full text-sm">
          <thead>
            <tr className="text-left text-xs text-surface-800/40 border-b border-surface-100">
              <th className="px-5 py-2 font-medium">GPU Type</th>
              <th className="px-3 py-2 font-medium text-right">Avg Price</th>
              <th className="px-3 py-2 font-medium text-right">Min</th>
              <th className="px-3 py-2 font-medium text-right">Max</th>
              <th className="px-5 py-2 font-medium text-right">Listings</th>
            </tr>
          </thead>
          <tbody>
            {pricing.map((p) => (
              <tr
                key={p.gpu_type}
                className="border-b border-surface-50 last:border-b-0 hover:bg-surface-50 transition-colors"
              >
                <td className="px-5 py-2.5 font-medium text-surface-900">{p.gpu_type}</td>
                <td className="px-3 py-2.5 text-right font-mono text-surface-800/70">
                  {p.avg_price_per_hour_erg.toFixed(4)}
                </td>
                <td className="px-3 py-2.5 text-right font-mono text-emerald-600">
                  {p.min_price_per_hour_erg.toFixed(4)}
                </td>
                <td className="px-3 py-2.5 text-right font-mono text-surface-800/40">
                  {p.max_price_per_hour_erg.toFixed(4)}
                </td>
                <td className="px-5 py-2.5 text-right text-surface-800/50">
                  {p.listing_count}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}
