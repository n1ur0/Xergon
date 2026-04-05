import type { ModelInfo } from "@/lib/api/client";

/**
 * Fallback models used when the backend is unavailable.
 * These represent typical models that Xergon providers would host locally
 * (Ollama / llama.cpp). The relay's /v1/models endpoint returns
 * dynamically enriched versions of these from live providers.
 */
export const FALLBACK_MODELS: ModelInfo[] = [
  {
    id: "llama-3.3-70b",
    name: "Llama 3.3 70B",
    provider: "Meta",
    tier: "pro",
    pricePerInputTokenNanoerg: 200,
    pricePerOutputTokenNanoerg: 200,
    effectivePriceNanoerg: 200,
    providerCount: 0,
    available: true,
    description: "General-purpose large language model with strong reasoning.",
    contextWindow: 8192,
    speed: "balanced",
    tags: ["Smart", "Code"],
    freeTier: false,
  },
  {
    id: "qwen3.5-4b-f16.gguf",
    name: "Qwen 3.5 4B (F16)",
    provider: "Alibaba",
    tier: "pro",
    pricePerInputTokenNanoerg: 50,
    pricePerOutputTokenNanoerg: 50,
    effectivePriceNanoerg: 50,
    providerCount: 0,
    available: true,
    description: "High-capability model with strong reasoning and coding.",
    contextWindow: 32768,
    speed: "balanced",
    tags: ["Smart", "Code", "Creative"],
    freeTier: false,
  },
  {
    id: "mistral-small-24b",
    name: "Mistral Small 24B",
    provider: "Mistral AI",
    tier: "free",
    pricePerInputTokenNanoerg: 0,
    pricePerOutputTokenNanoerg: 0,
    effectivePriceNanoerg: 0,
    providerCount: 0,
    available: true,
    description: "Efficient model with strong coding ability.",
    contextWindow: 32768,
    speed: "fast",
    tags: ["Fast", "Code"],
    freeTier: true,
  },
  {
    id: "llama-3.1-8b",
    name: "Llama 3.1 8B",
    provider: "Meta",
    tier: "free",
    pricePerInputTokenNanoerg: 0,
    pricePerOutputTokenNanoerg: 0,
    effectivePriceNanoerg: 0,
    providerCount: 0,
    available: true,
    description: "Fast and efficient language model for general tasks.",
    contextWindow: 4096,
    speed: "fast",
    tags: ["Fast", "Free"],
    freeTier: true,
  },
];
