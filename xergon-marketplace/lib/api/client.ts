/**
 * API client layer -- now powered by the Xergon SDK.
 *
 * Maintains backward-compatible exports (ModelInfo, InferenceRequest, etc.)
 * while delegating HTTP requests to the SDK's XergonClient.
 */

import { getWalletPk, API_BASE, sdk } from "./config";

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
    const models = await sdk.models.list();
    return models.map((m) => ({
      id: m.id,
      name: m.id,
      provider: m.ownedBy ?? "unknown",
      tier: "standard",
      pricePerInputTokenNanoerg: m.pricing ? parseInt(m.pricing, 10) : undefined,
      pricePerOutputTokenNanoerg: m.pricing ? parseInt(m.pricing, 10) : undefined,
      effectivePriceNanoerg: m.pricing ? parseInt(m.pricing, 10) : undefined,
      providerCount: 1,
      available: true,
    }));
  },

  /** Run inference (OpenAI-compatible chat completion) */
  infer: (req: InferenceRequest) =>
    sdk.chat.completions.create({
      model: req.model,
      messages: [{ role: "user", content: req.prompt }],
      maxTokens: req.maxTokens,
      temperature: req.temperature,
    }),

  /** Stream inference (SSE) -- returns raw Response for manual streaming */
  inferStream: (req: InferenceRequest, signal?: AbortSignal) => {
    const walletPk = getWalletPk();
    return fetch(`${API_BASE}/chat/completions`, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        "Accept": "text/event-stream",
        ...(walletPk ? { "X-Wallet-PK": walletPk } : {}),
      },
      body: JSON.stringify({
        model: req.model,
        messages: [{ role: "user", content: req.prompt }],
        max_tokens: req.maxTokens,
        temperature: req.temperature,
        stream: true,
      }),
      signal,
    });
  },

  /** Get provider leaderboard (public) */
  leaderboard: () => sdk.leaderboard(),
};
