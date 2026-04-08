/**
 * CLI command: oracle
 *
 * Oracle pool consumer commands for the Xergon Network.
 * Register pools, read prices, check staleness, subscribe to updates,
 * batch-read prices, and view statistics.
 *
 * Usage:
 *   xergon oracle register --pool-id ID --nft-token TOKEN_ID --reward-token TOKEN_ID
 *   xergon oracle pools
 *   xergon oracle price --pool-id ID
 *   xergon oracle history --pool-id ID [--from TIMESTAMP] [--to TIMESTAMP] [--limit N]
 *   xergon oracle staleness --pool-id ID
 *   xergon oracle subscribe --pool-id ID [--callback URL]
 *   xergon oracle batch-prices --pool-ids ID1,ID2,ID3
 *   xergon oracle stats
 */

import type { Command, CommandOption, ParsedArgs, CLIContext } from '../mod';

// ══════════════════════════════════════════════════════════════════
// Types
// ══════════════════════════════════════════════════════════════════

interface OraclePool {
  id: string;
  nftTokenId: string;
  currentRate: number;
  epochCounter: number;
  boxId: string;
  lastUpdated: number;
}

interface PriceReading {
  poolId: string;
  rate: number;
  epoch: number;
  timestamp: number;
  oracleCount: number;
}

interface OracleSubscription {
  id: string;
  poolId: string;
  active: boolean;
  createdAt: number;
}

interface PriceHistoryEntry {
  rate: number;
  epoch: number;
  timestamp: number;
}

interface OracleStats {
  totalReads: number;
  totalSubscriptions: number;
  activePools: number;
}

// ══════════════════════════════════════════════════════════════════
// Helpers
// ══════════════════════════════════════════════════════════════════

function isJsonOutput(args: ParsedArgs): boolean {
  return args.options.json === true;
}

function formatTimestamp(ms: number): string {
  if (!ms) return '-';
  return new Date(ms).toISOString().slice(0, 19).replace('T', ' ');
}

function truncateId(id: string, prefixLen = 10, suffixLen = 6): string {
  if (id.length <= prefixLen + suffixLen + 3) return id;
  return `${id.slice(0, prefixLen)}...${id.slice(-suffixLen)}`;
}

function stalenessLabel(lastUpdated: number): string {
  if (!lastUpdated) return '\x1b[31m● stale (no data)\x1b[0m';
  const diffMs = Date.now() - lastUpdated;
  const diffMin = Math.floor(diffMs / 60000);
  if (diffMin < 5) return '\x1b[32m● fresh\x1b[0m';
  if (diffMin < 30) return '\x1b[33m● aging\x1b[0m';
  return '\x1b[31m● stale\x1b[0m';
}

function stalenessLabelPlain(lastUpdated: number): string {
  if (!lastUpdated) return 'STALE (no data)';
  const diffMs = Date.now() - lastUpdated;
  const diffMin = Math.floor(diffMs / 60000);
  if (diffMin < 5) return 'FRESH';
  if (diffMin < 30) return 'AGING';
  return 'STALE';
}

function formatRate(rate: number): string {
  if (rate === 0) return '0';
  if (Math.abs(rate) >= 1_000_000) return (rate / 1_000_000).toFixed(2) + 'M';
  if (Math.abs(rate) >= 1_000) return (rate / 1_000).toFixed(2) + 'K';
  return rate.toFixed(6);
}

function formatDuration(ms: number): string {
  const diffMin = Math.floor(ms / 60000);
  const diffH = Math.floor(diffMin / 60);
  const diffD = Math.floor(diffH / 24);
  if (diffD > 0) return `${diffD}d ${diffH % 24}h`;
  if (diffH > 0) return `${diffH}h ${diffMin % 60}m`;
  return `${diffMin}m`;
}

// ══════════════════════════════════════════════════════════════════
// Subcommand: register
// ══════════════════════════════════════════════════════════════════

async function handleRegister(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const poolId = args.options.pool_id ? String(args.options.pool_id) : undefined;
  const nftTokenId = args.options.nft_token ? String(args.options.nft_token) : undefined;
  const rewardTokenId = args.options.reward_token ? String(args.options.reward_token) : undefined;

  if (!poolId || !nftTokenId || !rewardTokenId) {
    ctx.output.writeError('Usage: xergon oracle register --pool-id <id> --nft-token <token-id> --reward-token <token-id>');
    process.exit(1);
    return;
  }

  ctx.output.info(`Registering oracle pool ${truncateId(poolId)}...`);

  try {
    let result: OraclePool;

    if (ctx.client?.oracle?.register) {
      result = await ctx.client.oracle.register({
        poolId,
        nftTokenId,
        rewardTokenId,
      });
    } else {
      throw new Error('Oracle client not available. Ensure you are connected to the Xergon network.');
    }

    if (isJsonOutput(args)) {
      ctx.output.write(JSON.stringify(result, null, 2));
      return;
    }

    ctx.output.success('Oracle pool registered successfully');
    ctx.output.write('');
    ctx.output.write(ctx.output.formatText({
      'Pool ID': result.id,
      'NFT Token ID': result.nftTokenId,
      'Current Rate': formatRate(result.currentRate),
      'Epoch Counter': result.epochCounter,
      'Box ID': truncateId(result.boxId),
      'Last Updated': formatTimestamp(result.lastUpdated),
    }, 'Oracle Pool Registered'));
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to register oracle pool: ${message}`);
    process.exit(1);
  }
}

// ══════════════════════════════════════════════════════════════════
// Subcommand: pools
// ══════════════════════════════════════════════════════════════════

async function handlePools(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  try {
    let pools: OraclePool[];

    if (ctx.client?.oracle?.pools) {
      pools = await ctx.client.oracle.pools();
    } else {
      throw new Error('Oracle client not available.');
    }

    if (isJsonOutput(args)) {
      ctx.output.write(JSON.stringify(pools, null, 2));
      return;
    }

    if (pools.length === 0) {
      ctx.output.info('No oracle pools tracked.');
      ctx.output.info('Register one with: xergon oracle register --pool-id <id> --nft-token <token-id> --reward-token <token-id>');
      return;
    }

    ctx.output.write(ctx.output.colorize('Tracked Oracle Pools', 'bold'));
    ctx.output.write(ctx.output.colorize('═══════════════════════════════════════════════════════════════════════════════════════════', 'dim'));
    ctx.output.write('');

    for (const pool of pools) {
      const freshness = stalenessLabel(pool.lastUpdated);
      ctx.output.write(`  ${ctx.output.colorize(truncateId(pool.id), 'green')}  ${freshness}`);
      ctx.output.write(`    ${ctx.output.colorize('Rate:', 'cyan')}       ${formatRate(pool.currentRate)}`);
      ctx.output.write(`    ${ctx.output.colorize('Epoch:', 'cyan')}      ${pool.epochCounter}`);
      ctx.output.write(`    ${ctx.output.colorize('NFT Token:', 'cyan')}  ${truncateId(pool.nftTokenId)}`);
      ctx.output.write(`    ${ctx.output.colorize('Box ID:', 'cyan')}     ${truncateId(pool.boxId)}`);
      ctx.output.write(`    ${ctx.output.colorize('Updated:', 'cyan')}    ${formatTimestamp(pool.lastUpdated)}`);
      ctx.output.write('');
    }

    ctx.output.info(`${pools.length} pool(s) tracked.`);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to list oracle pools: ${message}`);
    process.exit(1);
  }
}

// ══════════════════════════════════════════════════════════════════
// Subcommand: price
// ══════════════════════════════════════════════════════════════════

async function handlePrice(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const poolId = args.options.pool_id ? String(args.options.pool_id) : undefined;

  if (!poolId) {
    ctx.output.writeError('Usage: xergon oracle price --pool-id <id>');
    process.exit(1);
    return;
  }

  try {
    let reading: PriceReading;

    if (ctx.client?.oracle?.price) {
      reading = await ctx.client.oracle.price({ poolId });
    } else {
      throw new Error('Oracle client not available.');
    }

    if (isJsonOutput(args)) {
      ctx.output.write(JSON.stringify(reading, null, 2));
      return;
    }

    ctx.output.write(ctx.output.formatText({
      'Pool ID': reading.poolId,
      'Rate': formatRate(reading.rate),
      'Epoch': reading.epoch,
      'Timestamp': formatTimestamp(reading.timestamp),
      'Oracle Count': reading.oracleCount,
    }, 'Current Price'));
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to read price: ${message}`);
    process.exit(1);
  }
}

// ══════════════════════════════════════════════════════════════════
// Subcommand: history
// ══════════════════════════════════════════════════════════════════

async function handleHistory(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const poolId = args.options.pool_id ? String(args.options.pool_id) : undefined;
  const from = args.options.from ? Number(args.options.from) : undefined;
  const to = args.options.to ? Number(args.options.to) : undefined;
  const limit = args.options.limit ? Number(args.options.limit) : undefined;

  if (!poolId) {
    ctx.output.writeError('Usage: xergon oracle history --pool-id <id> [--from TIMESTAMP] [--to TIMESTAMP] [--limit N]');
    process.exit(1);
    return;
  }

  try {
    let history: PriceHistoryEntry[];

    if (ctx.client?.oracle?.history) {
      history = await ctx.client.oracle.history({ poolId, from, to, limit });
    } else {
      throw new Error('Oracle client not available.');
    }

    if (isJsonOutput(args)) {
      ctx.output.write(JSON.stringify(history, null, 2));
      return;
    }

    if (history.length === 0) {
      ctx.output.info('No price history available for this pool.');
      return;
    }

    ctx.output.write(ctx.output.colorize(`Price History — ${truncateId(poolId)}`, 'bold'));
    ctx.output.write(ctx.output.colorize('─────────────────────────────────────────────────────────', 'dim'));
    ctx.output.write('');

    // Table header
    const header = '  EPOCH       RATE              TIMESTAMP';
    ctx.output.write(ctx.output.colorize(header, 'dim'));
    ctx.output.write(ctx.output.colorize('  ──────────────────────────────────────────────────────────────', 'dim'));

    for (const entry of history) {
      const epoch = String(entry.epoch).padEnd(12);
      const rate = formatRate(entry.rate).padEnd(18);
      const ts = formatTimestamp(entry.timestamp);
      ctx.output.write(`  ${ctx.output.colorize(epoch, 'green')}${rate}${ts}`);
    }

    ctx.output.write('');
    ctx.output.info(`${history.length} entry(ies) shown.`);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to get price history: ${message}`);
    process.exit(1);
  }
}

// ══════════════════════════════════════════════════════════════════
// Subcommand: staleness
// ══════════════════════════════════════════════════════════════════

async function handleStaleness(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const poolId = args.options.pool_id ? String(args.options.pool_id) : undefined;

  if (!poolId) {
    ctx.output.writeError('Usage: xergon oracle staleness --pool-id <id>');
    process.exit(1);
    return;
  }

  try {
    let pool: OraclePool;

    if (ctx.client?.oracle?.staleness) {
      pool = await ctx.client.oracle.staleness({ poolId });
    } else {
      throw new Error('Oracle client not available.');
    }

    if (isJsonOutput(args)) {
      ctx.output.write(JSON.stringify({
        poolId: pool.id,
        lastUpdated: pool.lastUpdated,
        staleness: stalenessLabelPlain(pool.lastUpdated),
        epochCounter: pool.epochCounter,
        currentRate: pool.currentRate,
      }, null, 2));
      return;
    }

    const freshness = stalenessLabel(pool.lastUpdated);
    const agoMs = pool.lastUpdated ? Date.now() - pool.lastUpdated : -1;
    const agoStr = agoMs >= 0 ? formatDuration(agoMs) + ' ago' : 'never';

    ctx.output.write(ctx.output.formatText({
      'Pool ID': pool.id,
      'Status': freshness,
      'Last Updated': formatTimestamp(pool.lastUpdated),
      'Time Elapsed': agoStr,
      'Epoch Counter': pool.epochCounter,
      'Current Rate': formatRate(pool.currentRate),
    }, 'Pool Staleness Check'));
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to check staleness: ${message}`);
    process.exit(1);
  }
}

// ══════════════════════════════════════════════════════════════════
// Subcommand: subscribe
// ══════════════════════════════════════════════════════════════════

async function handleSubscribe(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const poolId = args.options.pool_id ? String(args.options.pool_id) : undefined;
  const callback = args.options.callback ? String(args.options.callback) : undefined;

  if (!poolId) {
    ctx.output.writeError('Usage: xergon oracle subscribe --pool-id <id> [--callback <url>]');
    process.exit(1);
    return;
  }

  ctx.output.info(`Subscribing to oracle pool ${truncateId(poolId)}...`);

  try {
    let subscription: OracleSubscription;

    if (ctx.client?.oracle?.subscribe) {
      subscription = await ctx.client.oracle.subscribe({ poolId, callback });
    } else {
      throw new Error('Oracle client not available.');
    }

    if (isJsonOutput(args)) {
      ctx.output.write(JSON.stringify(subscription, null, 2));
      return;
    }

    ctx.output.success('Subscription created');
    ctx.output.write('');
    ctx.output.write(ctx.output.formatText({
      'Subscription ID': subscription.id,
      'Pool ID': subscription.poolId,
      'Active': subscription.active ? 'Yes' : 'No',
      'Created At': formatTimestamp(subscription.createdAt),
    }, 'Oracle Subscription'));
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to subscribe: ${message}`);
    process.exit(1);
  }
}

// ══════════════════════════════════════════════════════════════════
// Subcommand: batch-prices
// ══════════════════════════════════════════════════════════════════

async function handleBatchPrices(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const poolIdsRaw = args.options.pool_ids ? String(args.options.pool_ids) : undefined;

  if (!poolIdsRaw) {
    ctx.output.writeError('Usage: xergon oracle batch-prices --pool-ids ID1,ID2,ID3');
    process.exit(1);
    return;
  }

  const poolIds = poolIdsRaw.split(',').map(s => s.trim()).filter(Boolean);

  if (poolIds.length === 0) {
    ctx.output.writeError('At least one pool ID is required.');
    process.exit(1);
    return;
  }

  try {
    let prices: PriceReading[];

    if (ctx.client?.oracle?.batchPrices) {
      prices = await ctx.client.oracle.batchPrices({ poolIds });
    } else {
      throw new Error('Oracle client not available.');
    }

    if (isJsonOutput(args)) {
      ctx.output.write(JSON.stringify(prices, null, 2));
      return;
    }

    if (prices.length === 0) {
      ctx.output.info('No price data returned for the given pool IDs.');
      return;
    }

    ctx.output.write(ctx.output.colorize('Batch Price Read', 'bold'));
    ctx.output.write(ctx.output.colorize('════════════════════════════════════════════════════════════════════════════════', 'dim'));
    ctx.output.write('');

    const header = '  POOL ID                             RATE              EPOCH    ORACLES';
    ctx.output.write(ctx.output.colorize(header, 'dim'));
    ctx.output.write(ctx.output.colorize('  ─────────────────────────────────────────────────────────────────────────────', 'dim'));

    for (const p of prices) {
      const pid = truncateId(p.poolId, 20, 8).padEnd(37);
      const rate = formatRate(p.rate).padEnd(18);
      const epoch = String(p.epoch).padEnd(9);
      ctx.output.write(`  ${ctx.output.colorize(pid, 'green')}${rate}${epoch}${p.oracleCount}`);
    }

    ctx.output.write('');
    ctx.output.info(`${prices.length} price(s) returned.`);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to batch-read prices: ${message}`);
    process.exit(1);
  }
}

// ══════════════════════════════════════════════════════════════════
// Subcommand: stats
// ══════════════════════════════════════════════════════════════════

async function handleStats(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  try {
    let stats: OracleStats;

    if (ctx.client?.oracle?.stats) {
      stats = await ctx.client.oracle.stats();
    } else {
      throw new Error('Oracle client not available.');
    }

    if (isJsonOutput(args)) {
      ctx.output.write(JSON.stringify(stats, null, 2));
      return;
    }

    ctx.output.write(ctx.output.formatText({
      'Total Reads': stats.totalReads,
      'Total Subscriptions': stats.totalSubscriptions,
      'Active Pools': stats.activePools,
    }, 'Oracle Consumer Statistics'));
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to get oracle stats: ${message}`);
    process.exit(1);
  }
}

// ══════════════════════════════════════════════════════════════════
// Command dispatcher
// ══════════════════════════════════════════════════════════════════

async function oracleAction(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const sub = args.positional[0];

  if (!sub) {
    ctx.output.writeError('Usage: xergon oracle <subcommand> [options]');
    ctx.output.writeError('');
    ctx.output.writeError('Subcommands:');
    ctx.output.writeError('  register       Register a new oracle pool');
    ctx.output.writeError('  pools          List tracked oracle pools');
    ctx.output.writeError('  price          Read current price from a pool');
    ctx.output.writeError('  history        Get price history for a pool');
    ctx.output.writeError('  staleness      Check pool data staleness');
    ctx.output.writeError('  subscribe      Subscribe to pool updates');
    ctx.output.writeError('  batch-prices   Batch-read prices from multiple pools');
    ctx.output.writeError('  stats          Get oracle consumer statistics');
    process.exit(1);
    return;
  }

  switch (sub) {
    case 'register':
    case 'reg':
      await handleRegister(args, ctx);
      break;
    case 'pools':
    case 'list':
    case 'ls':
      await handlePools(args, ctx);
      break;
    case 'price':
    case 'read':
    case 'get':
      await handlePrice(args, ctx);
      break;
    case 'history':
    case 'hist':
      await handleHistory(args, ctx);
      break;
    case 'staleness':
    case 'stale':
    case 'freshness':
      await handleStaleness(args, ctx);
      break;
    case 'subscribe':
    case 'sub':
      await handleSubscribe(args, ctx);
      break;
    case 'batch-prices':
    case 'batch':
      await handleBatchPrices(args, ctx);
      break;
    case 'stats':
    case 'statistics':
      await handleStats(args, ctx);
      break;
    default:
      ctx.output.writeError(`Unknown oracle subcommand: ${sub}`);
      ctx.output.writeError('Run: xergon oracle (no args) to see available subcommands.');
      process.exit(1);
      break;
  }
}

// ══════════════════════════════════════════════════════════════════
// Command options
// ══════════════════════════════════════════════════════════════════

const oracleOptions: CommandOption[] = [
  {
    name: 'json',
    short: '',
    long: '--json',
    description: 'Output as JSON',
    required: false,
    type: 'boolean',
  },
  {
    name: 'pool_id',
    short: '',
    long: '--pool-id',
    description: 'Oracle pool identifier',
    required: false,
    type: 'string',
  },
  {
    name: 'nft_token',
    short: '',
    long: '--nft-token',
    description: 'NFT token ID for the pool',
    required: false,
    type: 'string',
  },
  {
    name: 'reward_token',
    short: '',
    long: '--reward-token',
    description: 'Reward token ID for the pool',
    required: false,
    type: 'string',
  },
  {
    name: 'from',
    short: '',
    long: '--from',
    description: 'Start timestamp for history query',
    required: false,
    type: 'number',
  },
  {
    name: 'to',
    short: '',
    long: '--to',
    description: 'End timestamp for history query',
    required: false,
    type: 'number',
  },
  {
    name: 'limit',
    short: '',
    long: '--limit',
    description: 'Maximum number of results',
    required: false,
    type: 'number',
  },
  {
    name: 'callback',
    short: '',
    long: '--callback',
    description: 'Callback URL for subscriptions',
    required: false,
    type: 'string',
  },
  {
    name: 'pool_ids',
    short: '',
    long: '--pool-ids',
    description: 'Comma-separated pool IDs for batch price read',
    required: false,
    type: 'string',
  },
];

export const oracleCommand: Command = {
  name: 'oracle',
  description: 'Oracle pool consumer commands — register pools, read prices, check staleness, subscribe',
  aliases: ['oracles'],
  options: oracleOptions,
  action: oracleAction,
};
