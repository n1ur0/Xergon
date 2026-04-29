/**
 * API client layer -- now powered by the Xergon SDK.
 *
 * Maintains backward-compatible exports (ModelInfo, InferenceRequest, etc.)
 * while delegating HTTP requests to the SDK's XergonClient.
 */

import { getWalletPk, API_BASE } from "./config";

// ── Legacy types (marketplace-specific, kept for compatibility) ──

export interface ModelInfo {
  id: string;
  name: string;
  provider: string;
  tier: string;
  /** @deprecated Use pricePerInputTokenNanoerg for real nanoERG pricing from relay */
  pricePerInputToken?: number;
  /** @deprecated Use pricePerOutputTokenNanoerg for real nanoERG pricing from relay */
  pricePerOutputToken?: number;
  pricePerInputTokenNanoerg?: number;
  pricePerOutputTokenNanoerg?: number;
  minProviderPriceNanoerg?: number;
  effectivePriceNanoerg?: number;
  providerCount?: number;
  available: boolean;
  /** One-line description of the model's strengths */
  description?: string;
  /** Context window size in tokens */
  contextWindow?: number;
  /** Speed class: "fast" | "balanced" | "slow" */
  speed?: "fast" | "balanced" | "slow";
  /** Tags for filtering: "Fast" | "Smart" | "Code" | "Creative" | "Free" */
  tags?: string[];
  /** Whether this model is free tier (no ERG required) */
  freeTier?: boolean;
}

export interface InferenceRequest {
  model: string;
  prompt: string;
  maxTokens?: number;
  temperature?: number;
}

export interface InferenceResponse {
  id: string;
  content: string;
  model: string;
  inputTokens: number;
  outputTokens: number;
  costNanoerg: number;
}

export interface LeaderboardEntry {
  provider_id: string;
  provider_name: string;
  region: string;
  ergo_address: string;
  models: string[];
  online: boolean;
  total_requests: number;
  total_prompt_tokens: number;
  total_completion_tokens: number;
  total_tokens: number;
  total_revenue_usd: number;
  unique_models: number;
}

// ── Legacy ApiError ──

export class ApiError extends Error {
  constructor(public status: number, message: string) {
    super(message);
    this.name = "ApiError";
  }
}

// ── SDK-powered endpoint stubs ──

export const endpoints = {
  /** List available models (returns marketplace ModelInfo[]) */
  listModels: async (): Promise<ModelInfo[]> => {
    const walletPk = getWalletPk();
    const res = await fetch(`${API_BASE}/v1/models`, {
      headers: walletPk ? { "x-user-pk": walletPk } : {},
    });
    if (!res.ok) throw new Error(`Failed to list models: ${res.status}`);
    const data = await res.json();
    return (data.data || []).map((m: { id: string; owned_by?: string; pricing?: string }) => ({
      id: m.id,
      name: m.id,
      provider: m.owned_by ?? "unknown",
      tier: "standard",
      pricePerInputTokenNanoerg: m.pricing ? parseInt(m.pricing, 10) : undefined,
      pricePerOutputTokenNanoerg: m.pricing ? parseInt(m.pricing, 10) : undefined,
      effectivePriceNanoerg: m.pricing ? parseInt(m.pricing, 10) : undefined,
      providerCount: 1,
      available: true,
    }));
  },

  /** Run inference (OpenAI-compatible chat completion) */
  infer: async (req: InferenceRequest): Promise<any> => {
    const walletPk = getWalletPk();
    const res = await fetch(`${API_BASE}/v1/chat/completions`, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        ...(walletPk ? { "x-user-pk": walletPk } : {}),
      },
      body: JSON.stringify({
        model: req.model,
        messages: [{ role: "user", content: req.prompt }],
        max_tokens: req.maxTokens,
        temperature: req.temperature,
      }),
    });
    if (!res.ok) throw new Error(`Inference failed: ${res.status}`);
    return res.json();
  },

  /** Get provider leaderboard (public) */
  leaderboard: async (): Promise<any> => {
    const res = await fetch(`${API_BASE}/v1/leaderboard`);
    if (!res.ok) throw new Error(`Failed to fetch leaderboard: ${res.status}`);
    return res.json();
  },
};
