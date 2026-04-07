"use client";

import { useState } from "react";
import { Server, Plus, X, Cpu, HardDrive, Globe } from "lucide-react";

export type PricingModel = "per-token" | "per-request" | "subscription";
export type SlaTier = "basic" | "standard" | "premium";

interface ProviderSetupStepProps {
  value: {
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
  onChange: (update: Partial<ProviderSetupStepProps["value"]>) => void;
}

const PRICING_MODELS: { value: PricingModel; label: string; description: string }[] = [
  { value: "per-token", label: "Per-Token", description: "Charged per input/output token" },
  { value: "per-request", label: "Per-Request", description: "Fixed price per API call" },
  { value: "subscription", label: "Subscription", description: "Monthly flat rate" },
];

const SLA_TIERS: { value: SlaTier; label: string; description: string; uptime: string }[] = [
  { value: "basic", label: "Basic", description: "Best effort", uptime: "99%" },
  { value: "standard", label: "Standard", description: "Response time SLA", uptime: "99.5%" },
  { value: "premium", label: "Premium", description: "Guaranteed uptime & support", uptime: "99.9%" },
];

const REGIONS = [
  "US East", "US West", "EU West", "EU Central", "Asia Pacific",
  "South America", "Africa", "Middle East", "Oceania",
];

const SUGGESTED_MODELS = [
  "llama-3.3-70b",
  "qwen3.5-4b-f16.gguf",
  "mistral-small-24b",
  "llama-3.1-8b",
  "deepseek-coder-33b",
  "phi-3-medium",
  "gemma-2-27b",
  "codestral-22b",
];

export default function ProviderSetupStep({ value, onChange }: ProviderSetupStepProps) {
  const [newModel, setNewModel] = useState("");

  const addModel = () => {
    const trimmed = newModel.trim().toLowerCase();
    if (trimmed && !value.models.includes(trimmed)) {
      onChange({ models: [...value.models, trimmed] });
      setNewModel("");
    }
  };

  const removeModel = (model: string) => {
    onChange({ models: value.models.filter((m) => m !== model) });
  };

  const addSuggestedModel = (model: string) => {
    if (!value.models.includes(model)) {
      onChange({ models: [...value.models, model] });
    }
  };

  return (
    <div className="space-y-6">
      <div className="text-center space-y-3">
        <div className="mx-auto flex h-16 w-16 items-center justify-center rounded-2xl bg-gradient-to-br from-orange-500 to-red-600 shadow-lg shadow-orange-500/20">
          <Server className="h-8 w-8 text-white" />
        </div>
        <h2 className="text-xl font-bold text-surface-900 dark:text-surface-0">
          Provider Setup
        </h2>
        <p className="text-sm text-surface-800/60 dark:text-surface-300/60 max-w-md mx-auto">
          Configure your inference endpoint, supported models, and pricing.
        </p>
      </div>

      <div className="mx-auto max-w-lg space-y-5">
        {/* Endpoint URL */}
        <div>
          <label className="block text-sm font-medium text-surface-700 dark:text-surface-300 mb-1.5">
            Inference Endpoint URL
          </label>
          <div className="relative">
            <Globe className="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-surface-400" />
            <input
              type="url"
              value={value.endpointUrl}
              onChange={(e) => onChange({ endpointUrl: e.target.value })}
              placeholder="https://your-node.example.com:11434"
              className="block w-full rounded-lg border border-surface-300 bg-surface-0 py-2.5 pl-10 pr-3 text-sm placeholder:text-surface-400 focus:border-emerald-500 focus:outline-none focus:ring-1 focus:ring-emerald-500 dark:border-surface-600 dark:bg-surface-900 dark:placeholder:text-surface-500"
            />
          </div>
        </div>

        {/* Supported Models */}
        <div>
          <label className="block text-sm font-medium text-surface-700 dark:text-surface-300 mb-1.5">
            Supported Models
          </label>
          {/* Current models */}
          {value.models.length > 0 && (
            <div className="flex flex-wrap gap-2 mb-2">
              {value.models.map((model) => (
                <span
                  key={model}
                  className="inline-flex items-center gap-1 rounded-full bg-surface-100 px-2.5 py-1 text-xs font-medium text-surface-700 dark:bg-surface-800 dark:text-surface-300"
                >
                  {model}
                  <button
                    type="button"
                    onClick={() => removeModel(model)}
                    className="ml-0.5 rounded-full hover:bg-surface-200 dark:hover:bg-surface-700 p-0.5"
                  >
                    <X className="h-3 w-3" />
                  </button>
                </span>
              ))}
            </div>
          )}
          {/* Add model input */}
          <div className="flex gap-2">
            <input
              type="text"
              value={newModel}
              onChange={(e) => setNewModel(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && (e.preventDefault(), addModel())}
              placeholder="Add model ID..."
              className="flex-1 rounded-lg border border-surface-300 bg-surface-0 px-3 py-2 text-sm placeholder:text-surface-400 focus:border-emerald-500 focus:outline-none focus:ring-1 focus:ring-emerald-500 dark:border-surface-600 dark:bg-surface-900 dark:placeholder:text-surface-500"
            />
            <button
              type="button"
              onClick={addModel}
              disabled={!newModel.trim()}
              className="flex items-center gap-1 rounded-lg bg-emerald-600 px-3 py-2 text-sm font-medium text-white transition-colors hover:bg-emerald-700 disabled:opacity-50"
            >
              <Plus className="h-4 w-4" />
              Add
            </button>
          </div>
          {/* Quick-add suggested models */}
          <div className="mt-2">
            <p className="text-xs text-surface-800/50 dark:text-surface-300/50 mb-1.5">
              Quick add:
            </p>
            <div className="flex flex-wrap gap-1.5">
              {SUGGESTED_MODELS.filter((m) => !value.models.includes(m)).map((model) => (
                <button
                  key={model}
                  type="button"
                  onClick={() => addSuggestedModel(model)}
                  className="rounded-full border border-dashed border-surface-300 px-2 py-0.5 text-xs text-surface-500 transition-colors hover:border-emerald-400 hover:text-emerald-600 dark:border-surface-600 dark:text-surface-400 dark:hover:border-emerald-600 dark:hover:text-emerald-400"
                >
                  + {model}
                </button>
              ))}
            </div>
          </div>
        </div>

        {/* Pricing Model */}
        <div>
          <label className="block text-sm font-medium text-surface-700 dark:text-surface-300 mb-2">
            Pricing Model
          </label>
          <div className="grid grid-cols-3 gap-2">
            {PRICING_MODELS.map((pm) => (
              <button
                key={pm.value}
                type="button"
                onClick={() => onChange({ pricingModel: pm.value })}
                className={`rounded-lg border-2 p-3 text-left transition-all ${
                  value.pricingModel === pm.value
                    ? "border-emerald-500 bg-emerald-50 dark:bg-emerald-950/30"
                    : "border-surface-200 hover:border-surface-300 dark:border-surface-700 dark:hover:border-surface-600"
                }`}
              >
                <p className={`text-xs font-semibold ${
                  value.pricingModel === pm.value
                    ? "text-emerald-700 dark:text-emerald-300"
                    : "text-surface-700 dark:text-surface-300"
                }`}>
                  {pm.label}
                </p>
                <p className="text-[10px] text-surface-800/50 dark:text-surface-300/50 mt-0.5">
                  {pm.description}
                </p>
              </button>
            ))}
          </div>
        </div>

        {/* SLA Tier */}
        <div>
          <label className="block text-sm font-medium text-surface-700 dark:text-surface-300 mb-2">
            SLA Tier
          </label>
          <div className="grid grid-cols-3 gap-2">
            {SLA_TIERS.map((tier) => (
              <button
                key={tier.value}
                type="button"
                onClick={() => onChange({ slaTier: tier.value })}
                className={`rounded-lg border-2 p-3 text-left transition-all ${
                  value.slaTier === tier.value
                    ? "border-emerald-500 bg-emerald-50 dark:bg-emerald-950/30"
                    : "border-surface-200 hover:border-surface-300 dark:border-surface-700 dark:hover:border-surface-600"
                }`}
              >
                <p className={`text-xs font-semibold ${
                  value.slaTier === tier.value
                    ? "text-emerald-700 dark:text-emerald-300"
                    : "text-surface-700 dark:text-surface-300"
                }`}>
                  {tier.label}
                </p>
                <p className="text-[10px] text-surface-800/50 dark:text-surface-300/50">
                  {tier.description}
                </p>
                <p className="text-[10px] font-mono text-surface-800/40 dark:text-surface-300/40 mt-0.5">
                  {tier.uptime}
                </p>
              </button>
            ))}
          </div>
        </div>

        {/* Hardware Specs */}
        <div className="space-y-3">
          <p className="text-sm font-medium text-surface-700 dark:text-surface-300">
            Hardware Specifications
          </p>
          <div className="grid grid-cols-2 gap-3">
            <div>
              <label className="block text-xs text-surface-800/60 dark:text-surface-300/60 mb-1">
                <Cpu className="inline h-3 w-3 mr-1" />
                GPU Type
              </label>
              <input
                type="text"
                value={value.gpuType}
                onChange={(e) => onChange({ gpuType: e.target.value })}
                placeholder="e.g. RTX 4090"
                className="block w-full rounded-lg border border-surface-300 bg-surface-0 px-3 py-2 text-sm placeholder:text-surface-400 focus:border-emerald-500 focus:outline-none focus:ring-1 focus:ring-emerald-500 dark:border-surface-600 dark:bg-surface-900 dark:placeholder:text-surface-500"
              />
            </div>
            <div>
              <label className="block text-xs text-surface-800/60 dark:text-surface-300/60 mb-1">
                <HardDrive className="inline h-3 w-3 mr-1" />
                VRAM
              </label>
              <input
                type="text"
                value={value.vram}
                onChange={(e) => onChange({ vram: e.target.value })}
                placeholder="e.g. 24 GB"
                className="block w-full rounded-lg border border-surface-300 bg-surface-0 px-3 py-2 text-sm placeholder:text-surface-400 focus:border-emerald-500 focus:outline-none focus:ring-1 focus:ring-emerald-500 dark:border-surface-600 dark:bg-surface-900 dark:placeholder:text-surface-500"
              />
            </div>
            <div>
              <label className="block text-xs text-surface-800/60 dark:text-surface-300/60 mb-1">
                CPU
              </label>
              <input
                type="text"
                value={value.cpu}
                onChange={(e) => onChange({ cpu: e.target.value })}
                placeholder="e.g. Ryzen 9 7950X"
                className="block w-full rounded-lg border border-surface-300 bg-surface-0 px-3 py-2 text-sm placeholder:text-surface-400 focus:border-emerald-500 focus:outline-none focus:ring-1 focus:ring-emerald-500 dark:border-surface-600 dark:bg-surface-900 dark:placeholder:text-surface-500"
              />
            </div>
            <div>
              <label className="block text-xs text-surface-800/60 dark:text-surface-300/60 mb-1">
                RAM
              </label>
              <input
                type="text"
                value={value.ram}
                onChange={(e) => onChange({ ram: e.target.value })}
                placeholder="e.g. 64 GB"
                className="block w-full rounded-lg border border-surface-300 bg-surface-0 px-3 py-2 text-sm placeholder:text-surface-400 focus:border-emerald-500 focus:outline-none focus:ring-1 focus:ring-emerald-500 dark:border-surface-600 dark:bg-surface-900 dark:placeholder:text-surface-500"
              />
            </div>
          </div>
        </div>

        {/* Region */}
        <div>
          <label className="block text-sm font-medium text-surface-700 dark:text-surface-300 mb-1.5">
            Region
          </label>
          <select
            value={value.region}
            onChange={(e) => onChange({ region: e.target.value })}
            className="block w-full rounded-lg border border-surface-300 bg-surface-0 px-3 py-2.5 text-sm focus:border-emerald-500 focus:outline-none focus:ring-1 focus:ring-emerald-500 dark:border-surface-600 dark:bg-surface-900"
          >
            <option value="">Select region</option>
            {REGIONS.map((r) => (
              <option key={r} value={r}>
                {r}
              </option>
            ))}
          </select>
        </div>
      </div>
    </div>
  );
}
