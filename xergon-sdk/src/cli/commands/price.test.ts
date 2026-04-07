/**
 * Tests for `xergon price` CLI command
 */

import { describe, it, expect, beforeEach } from 'vitest';
import {
  nanoergToErg,
  ergToNanoerg,
  formatUsd,
  formatPriceChange,
  colorPriceChange,
  sparkline,
  PriceService,
  PRICE_PAIRS,
  NANOERG_PER_ERG,
  type PricePair,
  type OracleSource,
  type PriceAlert,
  type CostEstimate,
  type BudgetStatus,
} from './price';

// ─── nanoergToErg ────────────────────────────────────────────────────

describe('nanoergToErg', () => {
  it('converts whole ERG amounts', () => {
    expect(nanoergToErg(1_000_000_000n)).toBe('1');
  });

  it('converts fractional ERG amounts', () => {
    expect(nanoergToErg(500_000_000n)).toBe('0.5');
  });

  it('handles zero', () => {
    expect(nanoergToErg(0n)).toBe('0');
  });

  it('handles large amounts', () => {
    expect(nanoergToErg(1_500_000_000_000n)).toBe('1500');
  });

  it('handles small fractional amounts', () => {
    expect(nanoergToErg(1n)).toBe('0.000000001');
  });

  it('accepts number input', () => {
    expect(nanoergToErg(1_000_000_000)).toBe('1');
  });

  it('trims trailing zeros', () => {
    expect(nanoergToErg(1_234_500_000n)).toBe('1.2345');
  });
});

// ─── ergToNanoerg ────────────────────────────────────────────────────

describe('ergToNanoerg', () => {
  it('converts whole ERG to nanoERG', () => {
    expect(ergToNanoerg('1')).toBe(1_000_000_000n);
  });

  it('converts fractional ERG to nanoERG', () => {
    expect(ergToNanoerg('0.5')).toBe(500_000_000n);
  });

  it('converts zero', () => {
    expect(ergToNanoerg('0')).toBe(0n);
  });

  it('handles number input', () => {
    expect(ergToNanoerg(1)).toBe(1_000_000_000n);
  });

  it('rounds at 9 decimals', () => {
    const result = ergToNanoerg('0.1234567895');
    expect(result).toBe(123_456_789n);
  });

  it('is inverse of nanoergToErg', () => {
    const original = '3.141592653';
    expect(nanoergToErg(ergToNanoerg(original))).toBe(original);
  });
});

// ─── formatUsd ───────────────────────────────────────────────────────

describe('formatUsd', () => {
  it('formats small amounts', () => {
    expect(formatUsd(0.42)).toBe('$0.4200');
  });

  it('formats larger amounts', () => {
    expect(formatUsd(123.456)).toBe('$123.4560');
  });

  it('formats zero', () => {
    expect(formatUsd(0)).toBe('$0.0000');
  });

  it('always shows 4 decimals', () => {
    const result = formatUsd(1);
    expect(result).toMatch(/^\$1\.0+$/);
  });
});

// ─── formatPriceChange ───────────────────────────────────────────────

describe('formatPriceChange', () => {
  it('shows positive change with +', () => {
    expect(formatPriceChange(1.1, 1.0)).toBe('+10.00%');
  });

  it('shows negative change', () => {
    expect(formatPriceChange(0.9, 1.0)).toBe('-10.00%');
  });

  it('shows zero change', () => {
    expect(formatPriceChange(1.0, 1.0)).toBe('+0.00%');
  });

  it('handles zero previous', () => {
    expect(formatPriceChange(1.0, 0)).toBe('');
  });
});

// ─── colorPriceChange ────────────────────────────────────────────────

describe('colorPriceChange', () => {
  it('returns green for increase', () => {
    expect(colorPriceChange(1.1, 1.0)).toBe('green');
  });

  it('returns red for decrease', () => {
    expect(colorPriceChange(0.9, 1.0)).toBe('red');
  });

  it('returns green for no change', () => {
    expect(colorPriceChange(1.0, 1.0)).toBe('green');
  });

  it('returns white for zero previous', () => {
    expect(colorPriceChange(1.0, 0)).toBe('white');
  });
});

// ─── sparkline ───────────────────────────────────────────────────────

describe('sparkline', () => {
  it('generates sparkline from ascending data', () => {
    const result = sparkline([1, 2, 3, 4, 5, 6, 7, 8], 8);
    expect(result.length).toBeLessThanOrEqual(8);
    expect(result).toBeTruthy();
  });

  it('generates sparkline from descending data', () => {
    const result = sparkline([8, 7, 6, 5, 4, 3, 2, 1], 8);
    expect(result.length).toBeLessThanOrEqual(8);
  });

  it('generates flat line for constant data', () => {
    const result = sparkline([5, 5, 5, 5], 4);
    expect(result).toBeTruthy();
  });

  it('handles empty array', () => {
    expect(sparkline([])).toBeTruthy();
  });

  it('handles single point', () => {
    expect(sparkline([42])).toBeTruthy();
  });

  it('respects width parameter', () => {
    const result = sparkline([1, 2, 3, 4, 5], 3);
    expect(result.length).toBeLessThanOrEqual(3);
  });

  it('uses only sparkline characters', () => {
    const chars = new Set('▁▂▃▄▅▆▇█');
    const result = sparkline([1, 2, 3, 4, 5, 6, 7, 8]);
    for (const c of result) {
      expect(chars.has(c)).toBe(true);
    }
  });
});

// ─── PRICE_PAIRS ─────────────────────────────────────────────────────

describe('PRICE_PAIRS', () => {
  it('includes erg-usd', () => {
    expect(PRICE_PAIRS).toContain('erg-usd');
  });

  it('includes erg-btc', () => {
    expect(PRICE_PAIRS).toContain('erg-btc');
  });

  it('includes erg-eth', () => {
    expect(PRICE_PAIRS).toContain('erg-eth');
  });
});

// ─── NANOERG_PER_ERG ────────────────────────────────────────────────

describe('NANOERG_PER_ERG', () => {
  it('equals 1 billion', () => {
    expect(NANOERG_PER_ERG).toBe(1_000_000_000n);
  });
});

// ─── PriceService ────────────────────────────────────────────────────

describe('PriceService', () => {
  let svc: PriceService;

  beforeEach(() => {
    svc = new PriceService();
  });

  it('returns ERG/USD price', () => {
    const price = svc.getPrice('erg-usd');
    expect(price).not.toBeNull();
    expect(price!.value).toBeGreaterThan(0);
    expect(price!.sources.length).toBeGreaterThan(0);
  });

  it('returns ERG/BTC price', () => {
    const price = svc.getPrice('erg-btc');
    expect(price).not.toBeNull();
    expect(price!.value).toBeGreaterThan(0);
  });

  it('returns null for unknown pair', () => {
    expect(svc.getPrice('unknown')).toBeNull();
  });

  it('returns history for known pair', () => {
    const history = svc.getHistory('erg-usd');
    expect(history.length).toBe(24);
    for (const point of history) {
      expect(point.price).toBeGreaterThan(0);
      expect(point.timestamp).toBeGreaterThan(0);
    }
  });

  it('returns empty history for unknown pair', () => {
    expect(svc.getHistory('unknown')).toEqual([]);
  });

  it('estimates cost for small model', () => {
    const estimate = svc.estimateCost('qwen3.5-4b', 1000, 0.42);
    expect(estimate.model).toBe('qwen3.5-4b');
    expect(estimate.tokens).toBe(1000);
    expect(parseFloat(estimate.ergCost)).toBeGreaterThan(0);
  });

  it('estimates cost for large model (more expensive)', () => {
    const small = svc.estimateCost('qwen3.5-4b', 1000, 0.42);
    const large = svc.estimateCost('qwen3.5-72b', 1000, 0.42);
    expect(parseFloat(large.ergCost)).toBeGreaterThan(parseFloat(small.ergCost));
  });

  it('converts ERG to USD', () => {
    const result = svc.convert(1, 'erg', 'usd', 0.42);
    expect(result).toBe(0.42);
  });

  it('converts USD to ERG', () => {
    const result = svc.convert(0.42, 'usd', 'erg', 0.42);
    expect(result).toBe(1);
  });

  it('returns same amount for same currency', () => {
    expect(svc.convert(5, 'erg', 'erg', 0.42)).toBe(5);
  });

  it('gets budget status', () => {
    const budget = svc.getBudget(ergToNanoerg('1.0'), ergToNanoerg('0.35'), 80);
    expect(budget.status).toBe('warning');
    expect(budget.percentUsed).toBe(35);
  });

  it('detects exceeded budget', () => {
    const budget = svc.getBudget(ergToNanoerg('1.0'), ergToNanoerg('1.5'), 80);
    expect(budget.status).toBe('exceeded');
  });

  it('detects ok budget', () => {
    const budget = svc.getBudget(ergToNanoerg('1.0'), ergToNanoerg('0.1'), 80);
    expect(budget.status).toBe('ok');
  });

  it('adds and retrieves alerts', () => {
    const alert = svc.addAlert({
      id: 'test1',
      pair: 'erg-usd',
      condition: 'above',
      target: 1.0,
      triggered: false,
      created: Date.now(),
    });
    expect(alert.id).toBe('test1');
    const alerts = svc.getAlerts('erg-usd');
    expect(alerts).toHaveLength(1);
  });

  it('filters alerts by pair', () => {
    svc.addAlert({ id: 'a1', pair: 'erg-usd', condition: 'above', target: 1, triggered: false, created: Date.now() });
    svc.addAlert({ id: 'a2', pair: 'erg-btc', condition: 'below', target: 0.00001, triggered: false, created: Date.now() });
    expect(svc.getAlerts('erg-usd')).toHaveLength(1);
    expect(svc.getAlerts()).toHaveLength(2);
  });
});

// ─── Command definition ──────────────────────────────────────────────

describe('priceCommand', () => {
  it('has correct name', () => {
    const { priceCommand } = require('./price');
    expect(priceCommand.command).toBe('price');
  });

  it('has describe text', () => {
    const { priceCommand } = require('./price');
    expect(priceCommand.describe).toBeTruthy();
  });

  it('has builder function', () => {
    const { priceCommand } = require('./price');
    expect(typeof priceCommand.builder).toBe('function');
  });
});
