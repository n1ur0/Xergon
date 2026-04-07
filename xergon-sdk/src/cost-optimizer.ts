/**
 * Cost optimization utilities for estimating and minimizing inference costs.
 *
 * Provides:
 * - TokenCounter: Estimate token counts for various models
 * - CostEstimator: Estimate costs and find cheapest models
 * - BudgetGuard: Track spending against a budget
 */

// ── Token Counter ────────────────────────────────────────────────────

export interface ChatMessage {
  role: string;
  content: string;
}

/**
 * Token estimation ratios (chars per token) for different model families.
 * These are approximate -- for exact counts, use model-specific tokenizers.
 */
const CHARS_PER_TOKEN: Record<string, number> = {
  'gpt-4': 3.5,
  'gpt-3.5': 4,
  'gpt-4o': 3.5,
  'llama': 3.5,
  'mistral': 3.5,
  'claude': 3.5,
  'default': 4,
};

function getCharsPerToken(model?: string): number {
  if (!model) return CHARS_PER_TOKEN['default'];
  const lower = model.toLowerCase();
  for (const [prefix, ratio] of Object.entries(CHARS_PER_TOKEN)) {
    if (prefix !== 'default' && lower.includes(prefix)) return ratio;
  }
  return CHARS_PER_TOKEN['default'];
}

export class TokenCounter {
  /**
   * Rough estimation of token count based on character ratios.
   * Suitable when exact tokenization isn't available.
   */
  estimateTokens(text: string, model?: string): number {
    if (!text) return 0;
    return Math.ceil(text.length / getCharsPerToken(model));
  }

  /**
   * Count tokens for a model using tiktoken (for GPT models) or fallback to estimation.
   * Falls back to estimateTokens if tiktoken isn't available or model isn't supported.
   */
  async countTokens(text: string, model?: string): Promise<number> {
    // For now, always use estimation. tiktoken can be added as an optional dependency.
    // If tiktoken is available and model is GPT, we could use it here.
    return this.estimateTokens(text, model);
  }

  /**
   * Count tokens in an array of chat messages.
   * Adds 4 tokens per message for formatting overhead (role, delimiters).
   */
  countMessageTokens(messages: ChatMessage[], model?: string): number {
    let total = 0;
    for (const msg of messages) {
      // ~4 tokens overhead per message for role/formatting
      total += 4;
      total += this.estimateTokens(msg.role, model);
      total += this.estimateTokens(msg.content, model);
    }
    // +3 tokens for priming (like OpenAI's calculation)
    total += 3;
    return total;
  }
}

// ── Cost Estimator ───────────────────────────────────────────────────

export interface PricingInfo {
  modelId: string;
  inputPricePerMillionTokens: number; // in nanoERG
  outputPricePerMillionTokens: number; // in nanoERG
  currency: 'nanoerg';
}

export interface CostEstimatorOptions {
  /** Custom fetch implementation (useful for tests). */
  fetchFn?: typeof fetch;
}

export class CostEstimator {
  private pricing: Map<string, PricingInfo>;
  private fetchFn: typeof fetch;

  constructor(pricing: PricingInfo[], options?: CostEstimatorOptions) {
    this.pricing = new Map();
    for (const p of pricing) {
      this.pricing.set(p.modelId, p);
    }
    this.fetchFn = options?.fetchFn ?? fetch;
  }

  /**
   * Estimate cost for a single request in nanoERG.
   */
  estimateCost(modelId: string, inputTokens: number, outputTokens: number): number {
    const info = this.pricing.get(modelId);
    if (!info) {
      throw new Error(`No pricing information for model: ${modelId}`);
    }
    const inputCost = (inputTokens / 1_000_000) * info.inputPricePerMillionTokens;
    const outputCost = (outputTokens / 1_000_000) * info.outputPricePerMillionTokens;
    return Math.ceil(inputCost + outputCost);
  }

  /**
   * Find the cheapest model for a task (given estimated tokens).
   */
  findCheapestModel(
    inputTokens: number,
    outputTokens: number,
  ): { modelId: string; cost: number } {
    let best: { modelId: string; cost: number } | null = null;

    for (const [modelId, info] of this.pricing) {
      const cost = this.estimateCost(modelId, inputTokens, outputTokens);
      if (!best || cost < best.cost) {
        best = { modelId, cost };
      }
    }

    if (!best) {
      throw new Error('No models with pricing information available');
    }

    return best;
  }

  /**
   * Get pricing for all models.
   */
  getPricing(): PricingInfo[] {
    return Array.from(this.pricing.values());
  }

  /**
   * Update pricing from relay /v1/models endpoint.
   * Models that include pricing metadata in their response will be added/updated.
   */
  async syncPricing(relayUrl: string): Promise<void> {
    const url = `${relayUrl.replace(/\/+$/, '')}/v1/models`;
    const res = await this.fetchFn(url);

    if (!res.ok) {
      throw new Error(`Failed to fetch models: HTTP ${res.status}`);
    }

    const data = await res.json() as { data?: Array<{ id: string; pricing?: string }> };

    if (!data.data || !Array.isArray(data.data)) {
      return;
    }

    for (const model of data.data) {
      if (model.pricing) {
        try {
          const pricing = JSON.parse(model.pricing) as {
            inputPerMillion?: number;
            outputPerMillion?: number;
          };
          if (pricing.inputPerMillion !== undefined && pricing.outputPerMillion !== undefined) {
            this.pricing.set(model.id, {
              modelId: model.id,
              inputPricePerMillionTokens: pricing.inputPerMillion,
              outputPricePerMillionTokens: pricing.outputPerMillion,
              currency: 'nanoerg',
            });
          }
        } catch {
          // Skip models with unparseable pricing
        }
      }
    }
  }

  /**
   * Get the number of models with pricing information.
   */
  getModelCount(): number {
    return this.pricing.size;
  }
}

// ── Budget Guard ─────────────────────────────────────────────────────

export interface BudgetOptions {
  /** Called when budget is exhausted. */
  onBudgetExhausted?: (summary: BudgetSummary) => void;
  /** Called when budget drops below this percentage (0-1). Default: 0.1 (10%). */
  warnThreshold?: number;
  /** Called when budget drops below warnThreshold. */
  onBudgetWarning?: (remaining: number, total: number) => void;
}

export interface BudgetUsageEntry {
  modelId: string;
  inputTokens: number;
  outputTokens: number;
  estimatedCostNanoErg: number;
  timestamp: number;
}

export interface BudgetSummary {
  maxBudgetNanoErg: number;
  totalSpentNanoErg: number;
  remainingBudgetNanoErg: number;
  totalRequests: number;
  totalInputTokens: number;
  totalOutputTokens: number;
  modelBreakdown: Record<string, { requests: number; inputTokens: number; outputTokens: number; costNanoErg: number }>;
  exhausted: boolean;
}

export class BudgetGuard {
  private maxBudget: number;
  private totalSpent: number = 0;
  private totalRequests: number = 0;
  private totalInputTokens: number = 0;
  private totalOutputTokens: number = 0;
  private modelBreakdown: Record<string, { requests: number; inputTokens: number; outputTokens: number; costNanoErg: number }> = {};
  private history: BudgetUsageEntry[] = [];
  private exhausted: boolean = false;
  private warnThreshold: number;
  private warned: boolean = false;
  private onBudgetExhausted?: (summary: BudgetSummary) => void;
  private onBudgetWarning?: (remaining: number, total: number) => void;

  constructor(maxBudgetNanoErg: number, options?: BudgetOptions) {
    this.maxBudget = maxBudgetNanoErg;
    this.warnThreshold = options?.warnThreshold ?? 0.1;
    this.onBudgetExhausted = options?.onBudgetExhausted;
    this.onBudgetWarning = options?.onBudgetWarning;
  }

  /**
   * Check if a request would exceed the remaining budget.
   */
  canAfford(estimatedCostNanoErg: number): boolean {
    if (this.exhausted) return false;
    return this.totalSpent + estimatedCostNanoErg <= this.maxBudget;
  }

  /**
   * Record actual cost after request completion.
   */
  record(modelId: string, inputTokens: number, outputTokens: number, costNanoErg?: number): void {
    const cost = costNanoErg ?? 0;
    this.totalSpent += cost;
    this.totalRequests += 1;
    this.totalInputTokens += inputTokens;
    this.totalOutputTokens += outputTokens;

    // Update model breakdown
    if (!this.modelBreakdown[modelId]) {
      this.modelBreakdown[modelId] = { requests: 0, inputTokens: 0, outputTokens: 0, costNanoErg: 0 };
    }
    this.modelBreakdown[modelId].requests += 1;
    this.modelBreakdown[modelId].inputTokens += inputTokens;
    this.modelBreakdown[modelId].outputTokens += outputTokens;
    this.modelBreakdown[modelId].costNanoErg += cost;

    this.history.push({
      modelId,
      inputTokens,
      outputTokens,
      estimatedCostNanoErg: cost,
      timestamp: Date.now(),
    });

    // Check budget warning
    const remaining = this.maxBudget - this.totalSpent;
    const remainingRatio = remaining / this.maxBudget;
    if (!this.warned && remainingRatio <= this.warnThreshold && remainingRatio > 0) {
      this.warned = true;
      this.onBudgetWarning?.(remaining, this.maxBudget);
    }

    // Check budget exhaustion
    if (this.totalSpent >= this.maxBudget && !this.exhausted) {
      this.exhausted = true;
      this.onBudgetExhausted?.(this.getUsageSummary());
    }
  }

  /**
   * Get remaining budget in nanoERG.
   */
  getRemainingBudget(): number {
    return Math.max(0, this.maxBudget - this.totalSpent);
  }

  /**
   * Get remaining budget as a fraction (0-1).
   */
  getRemainingBudgetRatio(): number {
    if (this.maxBudget <= 0) return 0;
    return this.getRemainingBudget() / this.maxBudget;
  }

  /**
   * Get usage summary.
   */
  getUsageSummary(): BudgetSummary {
    return {
      maxBudgetNanoErg: this.maxBudget,
      totalSpentNanoErg: this.totalSpent,
      remainingBudgetNanoErg: this.getRemainingBudget(),
      totalRequests: this.totalRequests,
      totalInputTokens: this.totalInputTokens,
      totalOutputTokens: this.totalOutputTokens,
      modelBreakdown: { ...this.modelBreakdown },
      exhausted: this.exhausted,
    };
  }

  /**
   * Reset budget (optionally with a new budget amount).
   */
  reset(newBudgetNanoErg?: number): void {
    if (newBudgetNanoErg !== undefined) {
      this.maxBudget = newBudgetNanoErg;
    }
    this.totalSpent = 0;
    this.totalRequests = 0;
    this.totalInputTokens = 0;
    this.totalOutputTokens = 0;
    this.modelBreakdown = {};
    this.history = [];
    this.exhausted = false;
    this.warned = false;
  }

  /**
   * Check if the budget has been exhausted.
   */
  isExhausted(): boolean {
    return this.exhausted;
  }

  /**
   * Get request history.
   */
  getHistory(): BudgetUsageEntry[] {
    return [...this.history];
  }
}
