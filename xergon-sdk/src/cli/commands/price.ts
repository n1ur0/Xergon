/**
 * `xergon price` CLI -- Oracle feeds, cost estimation, ERG/USD conversion
 */

import type { Command, ParsedArgs, CLIContext } from '../mod';

// ─── Constants ───────────────────────────────────────────────────────
const NANOERG_PER_ERG = 1_000_000_000n;
const ORACLE_POLL_INTERVAL = 60_000;
const DEFAULT_FEE_NANOERG = 1_000_000n;

const PRICE_PAIRS = ['erg-usd', 'erg-btc', 'erg-eth'] as const;
type PricePair = (typeof PRICE_PAIRS)[number];

interface OracleSource {
  name: string;
  price: number;
  timestamp: number;
  confidence: number;
}

interface PriceHistoryPoint {
  timestamp: number;
  price: number;
  source: string;
}

interface PriceAlert {
  id: string;
  pair: PricePair;
  condition: 'above' | 'below';
  target: number;
  triggered: boolean;
  created: number;
}

interface CostEstimate {
  model: string;
  tokens: number;
  ergCost: string;
  usdCost: string;
  feeErg: string;
  totalErg: string;
}

interface BudgetStatus {
  limitErg: string;
  spentErg: string;
  remainingErg: string;
  percentUsed: number;
  alertThreshold: number;
  status: 'ok' | 'warning' | 'exceeded';
}

// ─── Helpers ─────────────────────────────────────────────────────────

function nanoergToErg(nanoerg: bigint | number): string {
  const n = typeof nanoerg === 'bigint' ? nanoerg : BigInt(nanoerg);
  const whole = n / NANOERG_PER_ERG;
  const frac = n % NANOERG_PER_ERG;
  const fracStr = frac.toString().padStart(9, '0').slice(0, 9).replace(/0+$/, '');
  return fracStr ? `${whole}.${fracStr}` : whole.toString();
}

function ergToNanoerg(erg: string | number): bigint {
  const s = typeof erg === 'number' ? erg.toString() : erg;
  const parts = s.split('.');
  const whole = BigInt(parts[0] || '0');
  const frac = (parts[1] || '').padEnd(9, '0').slice(0, 9);
  return whole * NANOERG_PER_ERG + BigInt(frac);
}

function formatUsd(amount: number): string {
  return `$${amount.toFixed(4)}`;
}

function formatPriceChange(current: number, previous: number): string {
  if (previous === 0) return '';
  const pct = ((current - previous) / previous) * 100;
  const sign = pct >= 0 ? '+' : '';
  return `${sign}${pct.toFixed(2)}%`;
}

function colorPriceChange(current: number, previous: number): string {
  if (previous === 0) return 'white';
  return current >= previous ? 'green' : 'red';
}

function sparkline(points: number[], width: number = 40): string {
  if (points.length === 0) return '▁'.repeat(width);
  const min = Math.min(...points);
  const max = Math.max(...points);
  const range = max - min || 1;
  const chars = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
  const step = Math.max(1, Math.floor(points.length / width));
  let result = '';
  for (let i = 0; i < points.length && result.length < width; i += step) {
    const idx = Math.min(Math.floor(((points[i] - min) / range) * (chars.length - 1)), chars.length - 1);
    result += chars[idx];
  }
  return result;
}

function generateId(): string {
  return Math.random().toString(36).slice(2, 10);
}

// ─── Price Service (offline mode) ────────────────────────────────────

class PriceService {
  private prices: Map<string, { value: number; sources: OracleSource[]; history: PriceHistoryPoint[] }> = new Map();
  private alerts: Map<string, PriceAlert> = new Map();

  constructor() {
    // Initialize with mock data
    this.prices.set('erg-usd', {
      value: 0.42,
      sources: [
        { name: 'ergo-oracle-pool', price: 0.42, timestamp: Date.now() - 5000, confidence: 0.95 },
        { name: 'spectrum-dex', price: 0.418, timestamp: Date.now() - 30000, confidence: 0.85 },
      ],
      history: Array.from({ length: 24 }, (_, i) => ({
        timestamp: Date.now() - (23 - i) * 3600000,
        price: 0.40 + Math.sin(i * 0.3) * 0.03 + Math.random() * 0.01,
        source: 'ergo-oracle-pool',
      })),
    });
    this.prices.set('erg-btc', {
      value: 0.0000062,
      sources: [
        { name: 'ergo-oracle-pool', price: 0.0000062, timestamp: Date.now() - 10000, confidence: 0.9 },
      ],
      history: Array.from({ length: 24 }, (_, i) => ({
        timestamp: Date.now() - (23 - i) * 3600000,
        price: 0.0000058 + Math.sin(i * 0.2) * 0.0000004 + Math.random() * 0.0000001,
        source: 'ergo-oracle-pool',
      })),
    });
  }

  getPrice(pair: string): { value: number; sources: OracleSource[] } | null {
    const data = this.prices.get(pair);
    return data ? { value: data.value, sources: data.sources } : null;
  }

  getHistory(pair: string): PriceHistoryPoint[] {
    return this.prices.get(pair)?.history || [];
  }

  addAlert(alert: PriceAlert): PriceAlert {
    this.alerts.set(alert.id, alert);
    return alert;
  }

  getAlerts(pair?: string): PriceAlert[] {
    const all = Array.from(this.alerts.values());
    return pair ? all.filter(a => a.pair === pair) : all;
  }

  estimateCost(model: string, tokens: number, ergUsd: number): CostEstimate {
    // Pricing tiers: smaller models cheaper
    const isLarge = model.includes('70b') || model.includes('72b') || model.includes('120b');
    const isMedium = model.includes('32b') || model.includes('34b') || model.includes('40b');
    const baseCostPer1kTokens = isLarge ? 0.00085 : isMedium ? 0.00042 : 0.00015;

    const ergCost = (tokens / 1000) * baseCostPer1kTokens;
    const usdCost = ergCost * ergUsd;
    const feeErg = Number(nanoergToErg(DEFAULT_FEE_NANOERG));
    const totalErg = ergCost + feeErg;

    return {
      model,
      tokens,
      ergCost: nanoergToErg(ergToNanoerg(ergCost.toFixed(9))),
      usdCost: formatUsd(usdCost),
      feeErg: nanoergToErg(DEFAULT_FEE_NANOERG),
      totalErg: nanoergToErg(ergToNanoerg(totalErg.toFixed(9))),
    };
  }

  convert(amount: number, from: 'erg' | 'usd', to: 'erg' | 'usd', rate: number): number {
    if (from === to) return amount;
    if (from === 'erg') return amount * rate;
    return amount / rate;
  }

  getBudget(limitNanoerg: bigint, spentNanoerg: bigint, alertPct: number): BudgetStatus {
    const remaining = limitNanoerg - spentNanoerg;
    const pct = limitNanoerg > 0n ? Number((spentNanoerg * 10000n) / limitNanoerg) / 100 : 0;
    return {
      limitErg: nanoergToErg(limitNanoerg),
      spentErg: nanoergToErg(spentNanoerg),
      remainingErg: nanoergToErg(remaining),
      percentUsed: pct,
      alertThreshold: alertPct,
      status: pct >= 100 ? 'exceeded' : pct >= alertPct ? 'warning' : 'ok',
    };
  }
}

const service = new PriceService();

// ─── Formatters ──────────────────────────────────────────────────────

function formatPriceTable(pair: PricePair, json: boolean): string {
  const data = service.getPrice(pair);
  if (!data) return `No price data for ${pair}`;

  if (json) {
    return JSON.stringify({ pair, price: data.value, sources: data.sources }, null, 2);
  }

  const lines: string[] = [];
  lines.push(`  ERG/${pair.split('-')[1].toUpperCase()} Price`);
  lines.push(`  ${'─'.repeat(40)}`);
  lines.push(`  Aggregated: ${formatUsd(data.value)}`);

  const history = service.getHistory(pair);
  if (history.length >= 2) {
    const prev = history[history.length - 2].price;
    const change = formatPriceChange(data.value, prev);
    const color = colorPriceChange(data.value, prev);
    lines.push(`  24h Change: ${change} (${color})`);
  }

  lines.push('');
  lines.push(`  Sources:`);
  for (const src of data.sources) {
    const age = Math.round((Date.now() - src.timestamp) / 1000);
    lines.push(`    ${src.name.padEnd(20)} ${formatUsd(src.price).padEnd(12)} conf: ${(src.confidence * 100).toFixed(0)}%  age: ${age}s`);
  }

  if (history.length > 1) {
    lines.push('');
    lines.push(`  24h Sparkline:`);
    lines.push(`  ${sparkline(history.map(h => h.price))}`);
  }

  return lines.join('\n');
}

function formatHistory(pair: PricePair, json: boolean): string {
  const history = service.getHistory(pair);
  if (json) return JSON.stringify({ pair, history }, null, 2);

  if (history.length === 0) return `No history for ${pair}`;

  const lines: string[] = [];
  lines.push(`  ${pair.toUpperCase()} Price History (24h)`);
  lines.push(`  ${'─'.repeat(50)}`);
  lines.push(`  ${sparkline(history.map(h => h.price))}`);
  lines.push('');

  // Show last 10 data points
  const recent = history.slice(-10);
  for (const point of recent) {
    const time = new Date(point.timestamp).toLocaleTimeString();
    lines.push(`    ${time.padEnd(12)} ${formatUsd(point.price).padEnd(12)} (${point.source})`);
  }

  // Stats
  const prices = history.map(h => h.price);
  const min = Math.min(...prices);
  const max = Math.max(...prices);
  const avg = prices.reduce((a, b) => a + b, 0) / prices.length;
  lines.push('');
  lines.push(`  Min: ${formatUsd(min)}  Max: ${formatUsd(max)}  Avg: ${formatUsd(avg)}`);

  return lines.join('\n');
}

function formatEstimate(model: string, tokens: number, json: boolean): string {
  const ergUsd = service.getPrice('erg-usd');
  const rate = ergUsd?.value || 0.42;
  const estimate = service.estimateCost(model, tokens, rate);

  if (json) return JSON.stringify(estimate, null, 2);

  const lines: string[] = [];
  lines.push(`  Cost Estimate for ${model}`);
  lines.push(`  ${'─'.repeat(40)}`);
  lines.push(`  Tokens:         ${estimate.tokens.toLocaleString()}`);
  lines.push(`  Inference Cost: ${estimate.ergCost} ERG (${estimate.usdCost})`);
  lines.push(`  Tx Fee:         ${estimate.feeErg} ERG`);
  lines.push(`  ${'─'.repeat(40)}`);
  lines.push(`  Total:          ${estimate.totalErg} ERG`);

  return lines.join('\n');
}

function formatConvert(amount: number, from: string, to: string, json: boolean): string {
  const ergUsd = service.getPrice('erg-usd');
  const rate = ergUsd?.value || 0.42;
  const result = service.convert(amount, from as 'erg' | 'usd', to as 'erg' | 'usd', rate);

  if (json) return JSON.stringify({ amount, from, to, result, rate }, null, 2);

  const lines: string[] = [];
  const fromLabel = from === 'erg' ? `${nanoergToErg(ergToNanoerg(amount.toString()))} ERG` : formatUsd(amount);
  const toLabel = to === 'erg' ? nanoergToErg(ergToNanoerg(result.toFixed(9))) : formatUsd(result);
  lines.push(`  ${fromLabel} = ${toLabel}`);
  lines.push(`  Rate: 1 ERG = ${formatUsd(rate)}`);

  return lines.join('\n');
}

function formatBudget(json: boolean): string {
  const budget = service.getBudget(ergToNanoerg('1.0'), ergToNanoerg('0.35'), 80);
  if (json) return JSON.stringify(budget, null, 2);

  const lines: string[] = [];
  const statusIcon = budget.status === 'exceeded' ? '🔴' : budget.status === 'warning' ? '🟡' : '🟢';
  lines.push(`  ${statusIcon} Budget Status`);
  lines.push(`  ${'─'.repeat(40)}`);
  lines.push(`  Limit:     ${budget.limitErg} ERG`);
  lines.push(`  Spent:     ${budget.spentErg} ERG (${budget.percentUsed.toFixed(1)}%)`);
  lines.push(`  Remaining: ${budget.remainingErg} ERG`);
  lines.push(`  Alert at:  ${budget.alertThreshold}%`);

  // Progress bar
  const barWidth = 30;
  const filled = Math.min(Math.round(budget.percentUsed / 100 * barWidth), barWidth);
  const bar = '█'.repeat(filled) + '░'.repeat(barWidth - filled);
  lines.push(`  [${bar}] ${budget.percentUsed.toFixed(1)}%`);

  return lines.join('\n');
}

function formatAlerts(json: boolean, pair?: string): string {
  const alerts = service.getAlerts(pair as PricePair | undefined);
  if (json) return JSON.stringify({ alerts }, null, 2);

  if (alerts.length === 0) return '  No price alerts configured.';

  const lines: string[] = [];
  lines.push(`  Price Alerts`);
  lines.push(`  ${'─'.repeat(50)}`);
  for (const alert of alerts) {
    const status = alert.triggered ? '✅ TRIGGERED' : '⏳ Active';
    lines.push(`  ${alert.id}  ${alert.pair} ${alert.condition} ${alert.target}  ${status}`);
  }

  return lines.join('\n');
}

// ─── Command ─────────────────────────────────────────────────────────

export const priceCommand: Command = {
  name: 'price',
  description: 'Oracle price feeds and cost estimation',
  aliases: ['prices', 'oracle-price'],
  options: [
    { name: 'json', short: '-j', long: '--json', description: 'Output as JSON', required: false, default: 'false', type: 'boolean' },
    { name: 'subcommand', short: '-s', long: '--subcommand', description: 'Sub-command: erg-usd, erg-btc, history, estimate, convert, budget, alert', required: false, default: '', type: 'string' },
    { name: 'pair', short: '-p', long: '--pair', description: 'Price pair for history', required: false, default: '', type: 'string' },
    { name: 'model', short: '-m', long: '--model', description: 'Model name for cost estimate', required: false, default: '', type: 'string' },
    { name: 'tokens', short: '-t', long: '--tokens', description: 'Number of tokens for estimate', required: false, default: '1000', type: 'number' },
    { name: 'amount', short: '-a', long: '--amount', description: 'Amount to convert', required: false, default: '0', type: 'number' },
    { name: 'from', short: '', long: '--from', description: 'Source currency (erg, usd)', required: false, default: '', type: 'string' },
    { name: 'to', short: '', long: '--to', description: 'Target currency (erg, usd)', required: false, default: '', type: 'string' },
    { name: 'set', short: '', long: '--set', description: 'Set alert: <pair> <above|below> <price>', required: false, default: '', type: 'string' },
  ],
  action: async (args: ParsedArgs, _ctx: CLIContext) => {
    const json = args.options.json === 'true' || args.options.json === true;
    const sub = (args.options.subcommand as string) || (args.positional[0] || '');

    if (sub === 'erg-usd' || (!sub && !args.positional[0])) {
      console.log(formatPriceTable('erg-usd', json));
    } else if (sub === 'erg-btc') {
      console.log(formatPriceTable('erg-btc', json));
    } else if (sub === 'history') {
      const pair = ((args.options.pair as string) || (args.positional[1] || '')) as PricePair;
      if (!pair || !PRICE_PAIRS.includes(pair)) {
        console.error(`  Error: pair must be one of ${PRICE_PAIRS.join(', ')}`);
        return;
      }
      console.log(formatHistory(pair as PricePair, json));
    } else if (sub === 'estimate') {
      const model = (args.options.model as string) || (args.positional[1] || '');
      const tokens = Number(args.options.tokens) || 1000;
      if (!model) {
        console.error('  Error: --model <name> required');
        return;
      }
      console.log(formatEstimate(model, tokens, json));
    } else if (sub === 'convert') {
      const amount = Number(args.options.amount) || 0;
      const from = args.options.from as string;
      const to = args.options.to as string;
      if (!amount || !from || !to) {
        console.error('  Error: --amount, --from, --to required');
        return;
      }
      console.log(formatConvert(amount, from, to, json));
    } else if (sub === 'budget') {
      console.log(formatBudget(json));
    } else if (sub === 'alert') {
      const setVal = args.options.set as string;
      if (setVal) {
        const parts = setVal.split(' ');
        if (parts.length === 3) {
          const alert = service.addAlert({
            id: generateId(),
            pair: parts[0] as PricePair,
            condition: parts[1] as 'above' | 'below',
            target: parseFloat(parts[2]),
            triggered: false,
            created: Date.now(),
          });
          console.log(`  Alert set: ${alert.pair} ${alert.condition} ${alert.target} (id: ${alert.id})`);
        } else {
          console.log('  Usage: --set "<pair> <above|below> <price>"');
        }
      } else {
        console.log(formatAlerts(json));
      }
    } else {
      console.log('  Usage: xergon price [erg-usd|erg-btc|history|estimate|convert|budget|alert]');
      console.log('');
      console.log('  Subcommands:');
      console.log('    erg-usd       Current ERG/USD price from oracle pool');
      console.log('    erg-btc       Current ERG/BTC price from oracle pool');
      console.log('    history       Price history with sparkline');
      console.log('    estimate      Estimate inference cost in ERG');
      console.log('    convert       Convert ERG/USD amounts');
      console.log('    budget        Show budget status and remaining');
      console.log('    alert         Set or view price alerts');
    }
  },
};

// ─── Exports for testing ─────────────────────────────────────────────
export { nanoergToErg, ergToNanoerg, formatUsd, formatPriceChange, colorPriceChange, sparkline, NANOERG_PER_ERG, PRICE_PAIRS };
export type { OracleSource, PriceHistoryPoint, PriceAlert, CostEstimate, BudgetStatus, PricePair };
export { PriceService };
