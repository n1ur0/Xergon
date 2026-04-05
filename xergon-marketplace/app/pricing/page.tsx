"use client";

import { useState, useEffect } from "react";
import Link from "next/link";
import { fetchModels, nanoergToErg, type ChainModelInfo } from "@/lib/api/chain";

// ── Helpers ──

function formatErg(nanoerg: number): string {
  if (nanoerg <= 0) return "0";
  const erg = nanoerg / 1e9;
  return erg.toFixed(6).replace(/0+$/, "").replace(/\.$/, "");
}

function formatPricePer1M(nanoergPerToken: number): string {
  if (nanoergPerToken <= 0) return "Free";
  const nanoergPer1M = nanoergPerToken * 1_000_000;
  return `${formatErg(nanoergPer1M)} ERG`;
}

interface GpuPricing {
  gpu: string;
  vram: string;
  costPerHourErg: number;
}

const GPU_PRICES: GpuPricing[] = [
  { gpu: "RTX 4090", vram: "24 GB", costPerHourErg: 0.10 },
  { gpu: "RTX 4080", vram: "16 GB", costPerHourErg: 0.07 },
  { gpu: "RTX 3090", vram: "24 GB", costPerHourErg: 0.05 },
  { gpu: "A100 80GB", vram: "80 GB", costPerHourErg: 0.20 },
  { gpu: "RTX 6000 Ada", vram: "48 GB", costPerHourErg: 0.25 },
  { gpu: "H100 80GB", vram: "80 GB", costPerHourErg: 0.40 },
];

export default function PricingPage() {
  const [models, setModels] = useState<ChainModelInfo[]>([]);
  const [isLoading, setIsLoading] = useState(true);

  useEffect(() => {
    fetchModels()
      .then((data) => {
        setModels(data);
        setIsLoading(false);
      })
      .catch(() => {
        setIsLoading(false);
      });
  }, []);

  return (
    <div className="max-w-4xl mx-auto px-4 py-8">
      <h1 className="text-2xl font-bold mb-2">Pricing</h1>
      <p className="text-surface-800/60 mb-2">
        No subscriptions. Pay per use with ERG.
      </p>
      <p className="text-sm text-surface-800/40 mb-8">
        All prices are live from the marketplace. Prices are
        determined by the open market — providers set their own rates.
      </p>

      {/* Inference pricing — dynamic from API */}
      <section className="mb-10">
        <h2 className="text-lg font-semibold mb-4 flex items-center gap-2">
          <span>&#x1F916;</span> Inference Pricing
        </h2>
        <p className="text-sm text-surface-800/50 mb-4">
          Cost per 1M tokens (input + output)
        </p>

        {isLoading ? (
          <div className="grid gap-4 sm:grid-cols-2">
            {[1, 2, 3, 4].map((i) => (
              <div
                key={i}
                className="rounded-xl border border-surface-200 bg-surface-0 p-5 animate-pulse"
              >
                <div className="h-5 w-32 rounded bg-surface-100 mb-2" />
                <div className="h-3 w-20 rounded bg-surface-100 mb-4" />
                <div className="h-4 w-16 rounded bg-surface-100" />
              </div>
            ))}
          </div>
        ) : models.length > 0 ? (
          <div className="grid gap-4 sm:grid-cols-2">
            {models.map((model) => {
              const avgPriceNanoerg =
                (model.price_per_input_token_nanoerg +
                  model.price_per_output_token_nanoerg) /
                2;
              const isFree =
                model.price_per_input_token_nanoerg <= 0 &&
                model.price_per_output_token_nanoerg <= 0;

              return (
                <div
                  key={model.id}
                  className="rounded-xl border border-surface-200 bg-surface-0 p-5"
                >
                  <div className="flex items-start justify-between">
                    <div>
                      <h3 className="font-semibold text-surface-900">
                        {model.name}
                      </h3>
                      <p className="text-xs text-surface-800/40 mt-0.5">
                        {model.tier}
                        {model.provider_count != null && model.provider_count > 0
                          ? ` · ${model.provider_count} provider${model.provider_count !== 1 ? "s" : ""}`
                          : ""}
                      </p>
                    </div>
                    <div className="text-right">
                      {isFree ? (
                        <span className="inline-block rounded-full bg-emerald-100 px-2.5 py-0.5 text-sm font-bold text-emerald-700">
                          FREE
                        </span>
                      ) : (
                        <>
                          <span className="text-lg font-bold text-brand-600">
                            {formatPricePer1M(avgPriceNanoerg)}
                          </span>
                          {model.effective_price_nanoerg != null &&
                            model.effective_price_nanoerg !== avgPriceNanoerg && (
                              <div className="text-xs text-surface-800/30 mt-0.5">
                                effective: {formatPricePer1M(model.effective_price_nanoerg)}
                              </div>
                            )}
                        </>
                      )}
                    </div>
                  </div>
                  <div className="mt-3 flex items-center justify-between text-sm">
                    <span className="text-surface-800/50">1M tokens</span>
                    {model.description && (
                      <span className="text-xs text-surface-800/40 line-clamp-1 max-w-[200px]">
                        {model.description}
                      </span>
                    )}
                  </div>
                </div>
              );
            })}
          </div>
        ) : (
          <div className="rounded-xl border border-surface-200 bg-surface-0 p-8 text-center">
            <p className="text-sm text-surface-800/50">
              No models available right now. Check back soon.
            </p>
          </div>
        )}
      </section>

      {/* GPU rental pricing */}
      <section className="mb-10">
        <h2 className="text-lg font-semibold mb-4 flex items-center gap-2">
          <span>&#x1F4BB;</span> GPU Rental Pricing
        </h2>
        <p className="text-sm text-surface-800/50 mb-4">
          Cost per hour — on-chain settlement via Ergo
        </p>
        <div className="overflow-x-auto rounded-xl border border-surface-200">
          <table className="w-full text-sm">
            <thead>
              <tr className="text-left text-xs text-surface-800/40 border-b border-surface-100 bg-surface-50">
                <th className="px-5 py-3 font-medium">GPU</th>
                <th className="px-3 py-3 font-medium">VRAM</th>
                <th className="px-5 py-3 font-medium text-right">
                  Price / Hour
                </th>
                <th className="px-5 py-3 font-medium text-right">
                  Price / Day
                </th>
              </tr>
            </thead>
            <tbody className="divide-y divide-surface-50">
              {GPU_PRICES.map((item) => (
                <tr
                  key={item.gpu}
                  className="hover:bg-surface-50 transition-colors"
                >
                  <td className="px-5 py-3 font-medium text-surface-900">
                    {item.gpu}
                  </td>
                  <td className="px-3 py-3 text-surface-800/60">
                    {item.vram}
                  </td>
                  <td className="px-5 py-3 text-right font-mono text-surface-800/70">
                    {item.costPerHourErg.toFixed(4)} ERG
                  </td>
                  <td className="px-5 py-3 text-right font-mono text-surface-800/70">
                    {(item.costPerHourErg * 24).toFixed(4)} ERG
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </section>

      {/* Provider rewards */}
      <section className="mb-10">
        <h2 className="text-lg font-semibold mb-4 flex items-center gap-2">
          <span>&#x1F3AF;</span> Provider Rewards (PoNW)
        </h2>
        <div className="rounded-xl border border-surface-200 bg-surface-0 p-6 space-y-3">
          <p className="text-sm text-surface-800/60">
            Xergon uses <strong>Proof of Network Work (PoNW)</strong> to reward
            compute providers. Your score is based on:
          </p>
          <ul className="space-y-2 text-sm text-surface-800/60">
            <li className="flex items-start gap-2">
              <span className="text-brand-600 mt-0.5">&#x2022;</span>
              <span>
                <strong>AI Points</strong> — earned per inference request served,
                weighted by model difficulty
              </span>
            </li>
            <li className="flex items-start gap-2">
              <span className="text-brand-600 mt-0.5">&#x2022;</span>
              <span>
                <strong>Uptime</strong> — continuous availability increases your
                composite score
              </span>
            </li>
            <li className="flex items-start gap-2">
              <span className="text-brand-600 mt-0.5">&#x2022;</span>
              <span>
                <strong>Hardware</strong> — GPU type and VRAM size affect
                difficulty multipliers (2x for GPU, 1x for CPU)
              </span>
            </li>
          </ul>
          <p className="text-sm text-surface-800/60">
            Higher PoNW scores lead to more inference routing and higher ERG
            earnings. Settlements are sent directly to your linked Ergo wallet.
          </p>
        </div>
      </section>

      {/* Get ERG */}
      <section className="mb-10">
        <h2 className="text-lg font-semibold mb-4 flex items-center gap-2">
          <span>&#x1F4B0;</span> Get ERG
        </h2>
        <p className="text-sm text-surface-800/50 mb-4">
          You need ERG to pay for inference and GPU rentals. Buy ERG on any of
          these exchanges:
        </p>
        <div className="grid grid-cols-2 sm:grid-cols-3 gap-3">
          {[
            { name: "CoinEx", url: "https://www.coinex.com/exchange/erg-btc" },
            { name: "KuCoin", url: "https://www.kucoin.com/trade/ERG-USDT" },
            { name: "TradeOgre", url: "https://tradeogre.com/exchange/ERG-BTC" },
            { name: "Gate.io", url: "https://www.gate.io/trade/ERG_USDT" },
            { name: "SushiSwap", url: "https://app.sushi.com/swap" },
            { name: "CoinJar", url: "https://www.coinjar.com/" },
          ].map((exchange) => (
            <a
              key={exchange.name}
              href={exchange.url}
              target="_blank"
              rel="noopener noreferrer"
              className="rounded-lg border border-surface-200 bg-surface-0 px-4 py-3 text-sm font-medium text-surface-900 hover:bg-surface-50 transition-colors text-center"
            >
              {exchange.name}
            </a>
          ))}
        </div>
      </section>

      {/* Free tier note */}
      <div className="rounded-xl border border-brand-500/30 bg-brand-500/5 p-6 mb-8">
        <h3 className="font-semibold text-sm text-brand-700 mb-1">
          Free Tier
        </h3>
        <p className="text-sm text-surface-800/60">
          Some models are available for free (no wallet required). Free tier
          users get 10 requests per day. Connect your wallet to unlock
          unlimited access and higher rate limits.
        </p>
      </div>

      <div className="text-center">
        <Link
          href="/signin"
          className="inline-block rounded-lg bg-brand-600 px-6 py-2.5 text-sm font-medium text-white transition-colors hover:bg-brand-700"
        >
          Get Started
        </Link>
      </div>

      <div className="mt-8 text-sm text-surface-800/40 text-center">
        Prices are live from the marketplace and set by individual providers.
        All payments are settled on the Ergo blockchain.
      </div>
    </div>
  );
}
