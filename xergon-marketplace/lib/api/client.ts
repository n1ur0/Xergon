import { getToken, API_BASE } from "@/lib/api/config";

interface RequestOptions {
  body?: unknown;
  headers?: Record<string, string>;
  signal?: AbortSignal;
}

class ApiClient {
  private baseUrl: string;

  constructor(baseUrl: string) {
    this.baseUrl = baseUrl;
  }

  private async request<T>(
    path: string,
    method: string,
    options?: RequestOptions,
  ): Promise<T> {
    const token = getToken();
    const res = await fetch(`${this.baseUrl}${path}`, {
      method,
      headers: {
        "Content-Type": "application/json",
        ...(token ? { Authorization: `Bearer ${token}` } : {}),
        ...options?.headers,
      },
      body: options?.body ? JSON.stringify(options.body) : undefined,
      signal: options?.signal,
    });

    if (!res.ok) {
      const data = await res.json().catch(() => ({ message: res.statusText }));
      let message: string;
      if (data.error && typeof data.error === "object") {
        message = (data.error as { message?: string }).message ?? JSON.stringify(data.error);
      } else if (data.error && typeof data.error === "string") {
        message = data.error;
      } else if (data.message) {
        message = data.message;
      } else {
        message = res.statusText;
      }
      throw new ApiError(res.status, message);
    }

    return res.json() as Promise<T>;
  }

  async get<T>(path: string, options?: RequestOptions): Promise<T> {
    return this.request<T>(path, "GET", options);
  }

  async post<T>(path: string, body?: unknown, options?: RequestOptions): Promise<T> {
    return this.request<T>(path, "POST", { ...options, body });
  }

  async put<T>(path: string, body?: unknown, options?: RequestOptions): Promise<T> {
    return this.request<T>(path, "PUT", { ...options, body });
  }

  async del<T>(path: string, options?: RequestOptions): Promise<T> {
    return this.request<T>(path, "DELETE", options);
  }
}

export class ApiError extends Error {
  constructor(public status: number, message: string) {
    super(message);
    this.name = "ApiError";
  }
}

export const api = new ApiClient(API_BASE);

// ── Typed endpoint stubs ──

export interface ModelInfo {
  id: string;
  name: string;
  provider: string;
  tier: string;
  pricePerInputToken: number;
  pricePerOutputToken: number;
  available: boolean;
  /** One-line description of the model's strengths */
  description?: string;
  /** Context window size in tokens */
  contextWindow?: number;
  /** Speed class: "fast" | "balanced" | "slow" */
  speed?: "fast" | "balanced" | "slow";
  /** Tags for filtering: "Fast" | "Smart" | "Code" | "Creative" | "Free" */
  tags?: string[];
  /** Whether this model is free tier (no credits required) */
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
  creditsCharged: number;
}

export interface UserInfo {
  id: string;
  email: string;
  name?: string;
  credits: number;
  tier: string;
}

/** Shape returned by the backend for /auth/me and /auth/profile */
export interface MeResponse {
  id: string;
  email: string;
  name?: string;
  tier: string;
  credits_usd: number;
  ergo_address?: string | null;
}

export interface CreditBalance {
  credits_usd: number;
  currency: string;
}

export interface CreditPack {
  id: string;
  amount_usd: number;
  display_price: string;
  bonus_credits_usd: number;
}

export interface TransactionView {
  id: string;
  amount_usd: number;
  balance_after: number;
  kind: string;
  description: string;
  created_at: string;
}

export interface AutoReplenishSettings {
  enabled: boolean;
  pack_id: string | null;
  threshold_usd: number;
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

// ── Endpoint methods ──

export const endpoints = {
  /** List available models */
  listModels: () => api.get<ModelInfo[]>("/models"),

  /** Run inference */
  infer: (req: InferenceRequest) =>
    api.post<InferenceResponse>("/inference", req),

  /** Stream inference (SSE) — returns raw Response for manual streaming */
  inferStream: (req: InferenceRequest, signal?: AbortSignal) => {
    const token = getToken();
    return fetch(`${API_BASE}/inference/stream`, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        ...(token ? { Authorization: `Bearer ${token}` } : {}),
      },
      body: JSON.stringify(req),
      signal,
    });
  },

  /** Get current user */
  getMe: () => api.get<UserInfo>("/auth/me"),

  /** Get credit balance */
  getBalance: () => api.get<CreditBalance>("/credits/balance"),

  /** Get credit transaction history */
  getTransactions: () => api.get<{ transactions: TransactionView[] }>("/credits/transactions"),

  /** List available credit packs */
  getPacks: () => api.get<{ packs: CreditPack[] }>("/credits/packs"),

  /** Purchase credits — returns Stripe checkout URL */
  purchaseCredits: (packId: string) =>
    api.post<{ checkout_url: string; session_id: string }>("/credits/purchase", { pack_id: packId }),

  /** Get auto-replenish settings */
  getAutoReplenish: () => api.get<AutoReplenishSettings>("/credits/auto-replenish"),

  /** Update auto-replenish settings */
  updateAutoReplenish: (settings: AutoReplenishSettings) =>
    api.put<AutoReplenishSettings>("/credits/auto-replenish", settings),

  /** Update user profile (name/email) — returns full MeResponse */
  updateProfile: (data: { name?: string; email?: string }) =>
    api.put<MeResponse>("/auth/profile", data),

  /** Change user password */
  changePassword: (data: { current_password: string; new_password: string }) =>
    api.put<{ message: string }>("/auth/password", data),

  /** Request password reset email */
  forgotPassword: (email: string) =>
    api.post<{ message: string }>("/auth/forgot-password", { email }),

  /** Reset password with token */
  resetPassword: (token: string, new_password: string) =>
    api.post<{ message: string }>(`/auth/reset-password`, { token, new_password }),

  /** Update Ergo wallet address */
  updateWalletAddress: (ergo_address: string | null) =>
    api.put<UserInfo>("/auth/wallet", { ergo_address }),

  /** Get provider leaderboard (public) */
  leaderboard: () => api.get<LeaderboardEntry[]>("/leaderboard"),
};
