/**
 * Tests for cost optimization utilities -- TokenCounter, CostEstimator, BudgetGuard.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { TokenCounter, CostEstimator, BudgetGuard } from '../src/cost-optimizer';
import type { PricingInfo, BudgetSummary } from '../src/cost-optimizer';

// ═══════════════════════════════════════════════════════════════════════
// 1. TokenCounter
// ═══════════════════════════════════════════════════════════════════════

describe('TokenCounter', () => {
  let counter: TokenCounter;

  beforeEach(() => {
    counter = new TokenCounter();
  });

  describe('estimateTokens', () => {
    it('returns 0 for empty string', () => {
      expect(counter.estimateTokens('')).toBe(0);
    });

    it('estimates tokens for plain text', () => {
      const text = 'Hello world! This is a test message for token estimation.';
      const tokens = counter.estimateTokens(text);
      // Default ratio is ~4 chars/token, so ~64 chars / 4 = ~16 tokens
      expect(tokens).toBeGreaterThan(10);
      expect(tokens).toBeLessThan(30);
    });

    it('uses model-specific ratios', () => {
      const text = 'a'.repeat(100); // 100 chars

      const defaultTokens = counter.estimateTokens(text);
      const gpt4Tokens = counter.estimateTokens(text, 'gpt-4');
      const llamaTokens = counter.estimateTokens(text, 'llama-3.3-70b');

      // GPT-4 and llama have ratio 3.5 (more tokens per char than default 4)
      expect(gpt4Tokens).toBeGreaterThan(defaultTokens);
      expect(llamaTokens).toBeGreaterThan(defaultTokens);
    });

    it('handles multi-byte characters', () => {
      const text = 'Hello 世界 🌍';
      const tokens = counter.estimateTokens(text);
      expect(tokens).toBeGreaterThan(0);
    });
  });

  describe('countTokens', () => {
    it('returns a promise', async () => {
      const result = await counter.countTokens('hello world');
      expect(typeof result).toBe('number');
    });

    it('matches estimateTokens for now (no tiktoken)', async () => {
      const text = 'Hello world test message';
      const estimated = counter.estimateTokens(text);
      const counted = await counter.countTokens(text);
      expect(counted).toBe(estimated);
    });
  });

  describe('countMessageTokens', () => {
    it('counts tokens for empty messages array', () => {
      expect(counter.countMessageTokens([])).toBe(3); // just the 3 priming tokens
    });

    it('counts tokens for single message', () => {
      const messages = [{ role: 'user', content: 'Hello!' }];
      const tokens = counter.countMessageTokens(messages);
      // 4 (overhead) + role tokens + content tokens + 3 (priming)
      expect(tokens).toBeGreaterThan(5);
    });

    it('counts tokens for multi-turn conversation', () => {
      const messages = [
        { role: 'system', content: 'You are a helpful assistant.' },
        { role: 'user', content: 'What is the meaning of life?' },
        { role: 'assistant', content: '42' },
      ];
      const tokens = counter.countMessageTokens(messages);

      const singleTokens = counter.countMessageTokens([messages[0]]);
      expect(tokens).toBeGreaterThan(singleTokens);
    });

    it('accounts for role content in token count', () => {
      const messages = [{ role: 'user', content: '' }];
      const tokens = counter.countMessageTokens(messages);
      // 4 (overhead) + role tokens + 0 content + 3 (priming)
      expect(tokens).toBeGreaterThan(6);
    });
  });
});

// ═══════════════════════════════════════════════════════════════════════
// 2. CostEstimator
// ═══════════════════════════════════════════════════════════════════════

describe('CostEstimator', () => {
  const pricing: PricingInfo[] = [
    { modelId: 'llama-3.3-70b', inputPricePerMillionTokens: 100_000, outputPricePerMillionTokens: 200_000, currency: 'nanoerg' },
    { modelId: 'llama-3.3-8b', inputPricePerMillionTokens: 30_000, outputPricePerMillionTokens: 60_000, currency: 'nanoerg' },
    { modelId: 'mistral-7b', inputPricePerMillionTokens: 25_000, outputPricePerMillionTokens: 50_000, currency: 'nanoerg' },
  ];

  let estimator: CostEstimator;

  beforeEach(() => {
    estimator = new CostEstimator(pricing);
  });

  describe('estimateCost', () => {
    it('estimates cost for a known model', () => {
      const cost = estimator.estimateCost('llama-3.3-70b', 1000, 500);
      // Input: (1000/1M) * 100_000 = 100
      // Output: (500/1M) * 200_000 = 100
      // Total: 200
      expect(cost).toBe(200);
    });

    it('estimates cost for another model', () => {
      const cost = estimator.estimateCost('mistral-7b', 1_000_000, 500_000);
      // Input: (1M/1M) * 25_000 = 25_000
      // Output: (500K/1M) * 50_000 = 25_000
      // Total: 50_000
      expect(cost).toBe(50_000);
    });

    it('throws for unknown model', () => {
      expect(() => estimator.estimateCost('unknown-model', 100, 100)).toThrow('No pricing information');
    });

    it('handles zero tokens', () => {
      expect(estimator.estimateCost('llama-3.3-8b', 0, 0)).toBe(0);
    });

    it('handles only input tokens', () => {
      const cost = estimator.estimateCost('llama-3.3-8b', 1000, 0);
      expect(cost).toBe(30); // (1000/1M) * 30_000
    });
  });

  describe('findCheapestModel', () => {
    it('finds cheapest model for given tokens', () => {
      const cheapest = estimator.findCheapestModel(1000, 1000);
      expect(cheapest.modelId).toBe('mistral-7b');
    });

    it('returns correct cost for cheapest model', () => {
      const cheapest = estimator.findCheapestModel(1_000_000, 1_000_000);
      // mistral-7b: 25_000 + 50_000 = 75_000
      expect(cheapest.cost).toBe(75_000);
    });

    it('finds cheapest when input-heavy', () => {
      const cheapest = estimator.findCheapestModel(10_000_000, 0);
      expect(cheapest.modelId).toBe('mistral-7b');
    });

    it('finds cheapest when output-heavy', () => {
      const cheapest = estimator.findCheapestModel(0, 10_000_000);
      expect(cheapest.modelId).toBe('mistral-7b');
    });
  });

  describe('getPricing', () => {
    it('returns all pricing info', () => {
      const all = estimator.getPricing();
      expect(all).toHaveLength(3);
      expect(all.map((p) => p.modelId)).toContain('llama-3.3-70b');
      expect(all.map((p) => p.modelId)).toContain('llama-3.3-8b');
      expect(all.map((p) => p.modelId)).toContain('mistral-7b');
    });
  });

  describe('getModelCount', () => {
    it('returns number of models', () => {
      expect(estimator.getModelCount()).toBe(3);
    });

    it('returns 0 for empty estimator', () => {
      const empty = new CostEstimator([]);
      expect(empty.getModelCount()).toBe(0);
    });
  });

  describe('syncPricing', () => {
    it('updates pricing from relay response', async () => {
      const mockFetch = vi.fn().mockResolvedValue({
        ok: true,
        status: 200,
        headers: new Headers(),
        json: () =>
          Promise.resolve({
            data: [
              { id: 'new-model', pricing: '{"inputPerMillion": 50000, "outputPerMillion": 100000}' },
              { id: 'bad-pricing', pricing: 'not-json' },
              { id: 'no-pricing' },
            ],
          }),
      });

      const est = new CostEstimator(pricing, { fetchFn: mockFetch as unknown as typeof fetch });
      await est.syncPricing('https://relay.example.com');

      expect(est.estimateCost('new-model', 1_000_000, 1_000_000)).toBe(150_000);
      expect(est.getModelCount()).toBe(4); // 3 original + 1 new
    });

    it('throws on non-ok response', async () => {
      const mockFetch = vi.fn().mockResolvedValue({
        ok: false,
        status: 500,
        statusText: 'Internal Server Error',
        headers: new Headers(),
        json: () => Promise.resolve({}),
      });

      const est = new CostEstimator([], { fetchFn: mockFetch as unknown as typeof fetch });
      await expect(est.syncPricing('https://relay.example.com')).rejects.toThrow('Failed to fetch models');
    });

    it('handles empty data array gracefully', async () => {
      const mockFetch = vi.fn().mockResolvedValue({
        ok: true,
        status: 200,
        headers: new Headers(),
        json: () => Promise.resolve({ data: [] }),
      });

      const est = new CostEstimator([], { fetchFn: mockFetch as unknown as typeof fetch });
      await est.syncPricing('https://relay.example.com');
      expect(est.getModelCount()).toBe(0);
    });
  });
});

// ═══════════════════════════════════════════════════════════════════════
// 3. BudgetGuard
// ═══════════════════════════════════════════════════════════════════════

describe('BudgetGuard', () => {
  let guard: BudgetGuard;

  beforeEach(() => {
    guard = new BudgetGuard(1_000_000); // 1M nanoERG budget
  });

  describe('canAfford', () => {
    it('returns true when budget is sufficient', () => {
      expect(guard.canAfford(500_000)).toBe(true);
    });

    it('returns true for exact budget match', () => {
      expect(guard.canAfford(1_000_000)).toBe(true);
    });

    it('returns false when cost exceeds remaining budget', () => {
      guard.record('model-a', 100, 50, 900_000);
      expect(guard.canAfford(200_000)).toBe(false);
    });

    it('returns false when budget is exhausted', () => {
      guard.record('model-a', 100, 50, 1_000_000);
      expect(guard.canAfford(1)).toBe(false);
    });
  });

  describe('record', () => {
    it('tracks spending correctly', () => {
      guard.record('model-a', 100, 50, 100_000);
      guard.record('model-b', 200, 100, 200_000);

      expect(guard.getRemainingBudget()).toBe(700_000);
    });

    it('increments request count', () => {
      guard.record('model-a', 100, 50, 100_000);
      guard.record('model-a', 100, 50, 100_000);
      guard.record('model-b', 100, 50, 100_000);

      const summary = guard.getUsageSummary();
      expect(summary.totalRequests).toBe(3);
    });

    it('tracks token counts', () => {
      guard.record('model-a', 500, 250, 100_000);
      guard.record('model-b', 300, 150, 100_000);

      const summary = guard.getUsageSummary();
      expect(summary.totalInputTokens).toBe(800);
      expect(summary.totalOutputTokens).toBe(400);
    });

    it('builds model breakdown', () => {
      guard.record('model-a', 100, 50, 100_000);
      guard.record('model-a', 200, 100, 200_000);
      guard.record('model-b', 100, 50, 100_000);

      const summary = guard.getUsageSummary();
      expect(summary.modelBreakdown['model-a']).toEqual({
        requests: 2,
        inputTokens: 300,
        outputTokens: 150,
        costNanoErg: 300_000,
      });
      expect(summary.modelBreakdown['model-b']).toEqual({
        requests: 1,
        inputTokens: 100,
        outputTokens: 50,
        costNanoErg: 100_000,
      });
    });
  });

  describe('getRemainingBudget', () => {
    it('returns full budget initially', () => {
      expect(guard.getRemainingBudget()).toBe(1_000_000);
    });

    it('decreases after recording costs', () => {
      guard.record('model-a', 100, 50, 300_000);
      expect(guard.getRemainingBudget()).toBe(700_000);
    });

    it('never goes below zero', () => {
      guard.record('model-a', 100, 50, 2_000_000);
      expect(guard.getRemainingBudget()).toBe(0);
    });
  });

  describe('getUsageSummary', () => {
    it('returns complete summary', () => {
      guard.record('model-a', 100, 50, 250_000);

      const summary = guard.getUsageSummary();
      expect(summary).toEqual({
        maxBudgetNanoErg: 1_000_000,
        totalSpentNanoErg: 250_000,
        remainingBudgetNanoErg: 750_000,
        totalRequests: 1,
        totalInputTokens: 100,
        totalOutputTokens: 50,
        modelBreakdown: {
          'model-a': { requests: 1, inputTokens: 100, outputTokens: 50, costNanoErg: 250_000 },
        },
        exhausted: false,
      });
    });
  });

  describe('budget exhaustion', () => {
    it('fires onBudgetExhausted callback', () => {
      const onExhausted = vi.fn();
      const guard2 = new BudgetGuard(1_000_000, { onBudgetExhausted: onExhausted });

      guard2.record('model-a', 100, 50, 1_000_000);
      expect(onExhausted).toHaveBeenCalledTimes(1);
      expect(onExhausted).toHaveBeenCalledWith(
        expect.objectContaining({ exhausted: true }),
      );
    });

    it('isExhausted returns true after exhaustion', () => {
      guard.record('model-a', 100, 50, 1_000_000);
      expect(guard.isExhausted()).toBe(true);
    });

    it('canAfford returns false after exhaustion', () => {
      guard.record('model-a', 100, 50, 1_000_000);
      expect(guard.canAfford(0)).toBe(false);
    });

    it('does not fire onBudgetExhausted twice', () => {
      const onExhausted = vi.fn();
      const guard2 = new BudgetGuard(1_000_000, { onBudgetExhausted: onExhausted });

      guard2.record('model-a', 100, 50, 1_000_000);
      guard2.record('model-b', 100, 50, 500_000); // Over budget but callback already fired
      expect(onExhausted).toHaveBeenCalledTimes(1);
    });
  });

  describe('budget warning', () => {
    it('fires onBudgetWarning when threshold is reached', () => {
      const onWarning = vi.fn();
      const guard2 = new BudgetGuard(1_000_000, { warnThreshold: 0.2, onBudgetWarning: onWarning });

      // Spend 800K, leaving 200K (20% remaining, exactly at threshold)
      guard2.record('model-a', 100, 50, 800_000);
      expect(onWarning).toHaveBeenCalledTimes(1);
      expect(onWarning).toHaveBeenCalledWith(200_000, 1_000_000);
    });

    it('does not fire warning below threshold', () => {
      const onWarning = vi.fn();
      const guard2 = new BudgetGuard(1_000_000, { warnThreshold: 0.1, onBudgetWarning: onWarning });

      guard2.record('model-a', 100, 50, 500_000); // 50% remaining, above 10% threshold
      expect(onWarning).not.toHaveBeenCalled();
    });

    it('fires warning only once', () => {
      const onWarning = vi.fn();
      const guard2 = new BudgetGuard(1_000_000, { warnThreshold: 0.2, onBudgetWarning: onWarning });

      guard2.record('model-a', 100, 50, 800_000);
      guard2.record('model-b', 100, 50, 100_000);
      expect(onWarning).toHaveBeenCalledTimes(1);
    });
  });

  describe('reset', () => {
    it('resets spending while keeping budget', () => {
      guard.record('model-a', 100, 50, 500_000);
      guard.reset();

      expect(guard.getRemainingBudget()).toBe(1_000_000);
      expect(guard.getUsageSummary().totalRequests).toBe(0);
      expect(guard.isExhausted()).toBe(false);
    });

    it('resets with new budget', () => {
      guard.record('model-a', 100, 50, 500_000);
      guard.reset(2_000_000);

      expect(guard.getRemainingBudget()).toBe(2_000_000);
      expect(guard.getUsageSummary().maxBudgetNanoErg).toBe(2_000_000);
    });
  });

  describe('getHistory', () => {
    it('returns request history', () => {
      guard.record('model-a', 100, 50, 100_000);
      guard.record('model-b', 200, 100, 200_000);

      const history = guard.getHistory();
      expect(history).toHaveLength(2);
      expect(history[0].modelId).toBe('model-a');
      expect(history[1].modelId).toBe('model-b');
      expect(history[0].timestamp).toBeGreaterThan(0);
    });

    it('returns empty array after reset', () => {
      guard.record('model-a', 100, 50, 100_000);
      guard.reset();

      expect(guard.getHistory()).toHaveLength(0);
    });
  });

  describe('getRemainingBudgetRatio', () => {
    it('returns 1.0 at start', () => {
      expect(guard.getRemainingBudgetRatio()).toBe(1);
    });

    it('returns correct ratio after spending', () => {
      guard.record('model-a', 100, 50, 250_000);
      expect(guard.getRemainingBudgetRatio()).toBe(0.75);
    });

    it('returns 0 when exhausted', () => {
      guard.record('model-a', 100, 50, 1_500_000);
      expect(guard.getRemainingBudgetRatio()).toBe(0);
    });
  });
});
