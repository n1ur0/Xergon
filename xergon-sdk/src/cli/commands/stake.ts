//! `xergon stake` CLI command for staking pool management.
//!
//! Provides commands to:
//!   - `stake list`     — list available staking pools
//!   - `stake stake`    — stake XRG into a pool
//!   - `stake unstake`  — unstake from a pool
//!   - `stake delegate` — delegate staking power to a pool
//!   - `stake claim`    — claim pending rewards
//!   - `stake info`     — show pool details + staker position
//!   - `stake apy`      — show APY leaderboard across pools
//!   - `stake suggest`  — yield optimization suggestions

import { Command } from '@cliffy/command';
import { Table } from '@cliffy/table';
import { colors } from '@cliffy/colors';

// ---------------------------------------------------------------------------
// Mock data
// ---------------------------------------------------------------------------

interface Pool {
  id: string; name: string; apy: number; tvl: number; stakers: number;
  status: 'active' | 'full' | 'paused'; lockPeriod: number; minStake: number; autoCompound: boolean;
}

const MOCK_POOLS: Pool[] = [
  { id: 'xrg-01', name: 'Xergon Core Validator',   apy: 12.4, tvl: 2_450_000, stakers: 312,  status: 'active', lockPeriod: 30,  minStake: 100,  autoCompound: true  },
  { id: 'xrg-02', name: 'Community Growth Pool',   apy: 18.7, tvl: 890_000,   stakers: 154,  status: 'active', lockPeriod: 90,  minStake: 500,  autoCompound: true  },
  { id: 'xrg-03', name: 'Stability Reserve',       apy: 6.2,  tvl: 5_100_000, stakers: 1024, status: 'active', lockPeriod: 7,   minStake: 50,   autoCompound: false },
  { id: 'xrg-04', name: 'High-Yield Compute',      apy: 24.1, tvl: 340_000,   stakers: 67,   status: 'active', lockPeriod: 180, minStake: 1000, autoCompound: true  },
  { id: 'xrg-05', name: 'Liquidity Backstop',      apy: 9.8,  tvl: 1_780_000, stakers: 489,  status: 'full',   lockPeriod: 60,  minStake: 200,  autoCompound: false },
  { id: 'xrg-06', name: 'Node Operator Pool',      apy: 15.3, tvl: 1_220_000, stakers: 203,  status: 'active', lockPeriod: 30,  minStake: 1000, autoCompound: true  },
  { id: 'xrg-07', name: 'Ecosystem Rewards',       apy: 11.1, tvl: 670_000,   stakers: 178,  status: 'paused', lockPeriod: 14,  minStake: 100,  autoCompound: false },
];

const MOCK_POSITION = { poolId: 'xrg-01', staked: 5000, rewards: 312.47, compoundEnabled: true, entryDate: '2026-03-01T00:00:00Z', unlockDate: '2026-03-31T00:00:00Z' };

const MOCK_APY_HISTORY: Record<string, Record<string, number>> = {
  '7d':  { 'xrg-01': 12.6, 'xrg-02': 19.1, 'xrg-03': 6.0, 'xrg-04': 25.3, 'xrg-05': 9.5, 'xrg-06': 15.8, 'xrg-07': 10.9 },
  '30d': { 'xrg-01': 12.4, 'xrg-02': 18.7, 'xrg-03': 6.2, 'xrg-04': 24.1, 'xrg-05': 9.8, 'xrg-06': 15.3, 'xrg-07': 11.1 },
  '90d': { 'xrg-01': 11.9, 'xrg-02': 17.5, 'xrg-03': 6.5, 'xrg-04': 22.0, 'xrg-05': 10.2, 'xrg-06': 14.7, 'xrg-07': 11.8 },
};

const fmtXrg = (n: number) => `${n.toLocaleString()} XRG`;
const statusColor = (s: string) => s === 'active' ? colors.green(s) : s === 'full' ? colors.yellow(s) : colors.red(s);

function findPoolOrExit(id: string): Pool {
  const pool = MOCK_POOLS.find((p) => p.id === id);
  if (!pool) { console.error(colors.red(`Pool not found: ${id}`)); process.exit(1); }
  return pool;
}

// ---------------------------------------------------------------------------
// Options
// ---------------------------------------------------------------------------

const listOptions = [
  { name: 'sort', short: 's', long: '--sort', description: 'Sort by field (apy, tvl, stakers)', required: false, type: 'string' },
  { name: 'min-apy', short: '', long: '--min-apy', description: 'Minimum APY filter', required: false, type: 'number' },
  { name: 'json', short: '', long: '--json', description: 'Output in JSON format', required: false, type: 'boolean' },
];

const stakeOpts = [
  { name: 'pool', short: 'p', long: '--pool', description: 'Pool ID to stake into', required: true, type: 'string' },
  { name: 'amount', short: 'a', long: '--amount', description: 'Amount of XRG to stake', required: true, type: 'number' },
  { name: 'auto-compound', short: '', long: '--auto-compound', description: 'Enable auto-compound', required: false, type: 'boolean' },
  { name: 'json', short: '', long: '--json', description: 'Output in JSON format', required: false, type: 'boolean' },
];

const unstakeOpts = [
  { name: 'pool', short: 'p', long: '--pool', description: 'Pool ID to unstake from', required: true, type: 'string' },
  { name: 'amount', short: 'a', long: '--amount', description: 'Amount to unstake (default: all)', required: false, type: 'number' },
  { name: 'json', short: '', long: '--json', description: 'Output in JSON format', required: false, type: 'boolean' },
];

const delegateOpts = [
  { name: 'pool', short: 'p', long: '--pool', description: 'Pool ID to delegate to', required: true, type: 'string' },
  { name: 'amount', short: 'a', long: '--amount', description: 'Amount of XRG to delegate', required: true, type: 'number' },
  { name: 'json', short: '', long: '--json', description: 'Output in JSON format', required: false, type: 'boolean' },
];

const claimOpts = [
  { name: 'pool', short: 'p', long: '--pool', description: 'Pool ID to claim rewards from', required: true, type: 'string' },
  { name: 'compound', short: '', long: '--compound', description: 'Reinvest rewards instead of withdrawing', required: false, type: 'boolean' },
  { name: 'json', short: '', long: '--json', description: 'Output in JSON format', required: false, type: 'boolean' },
];

const infoOpts = [
  { name: 'pool', short: 'p', long: '--pool', description: 'Pool ID to inspect', required: true, type: 'string' },
  { name: 'json', short: '', long: '--json', description: 'Output in JSON format', required: false, type: 'boolean' },
];

const apyOpts = [
  { name: 'period', short: '', long: '--period', description: 'APY period (7d, 30d, 90d)', required: false, type: 'string' },
  { name: 'json', short: '', long: '--json', description: 'Output in JSON format', required: false, type: 'boolean' },
];

const suggestOpts = [
  { name: 'risk', short: '', long: '--risk', description: 'Risk tolerance (low, medium, high)', required: false, type: 'string' },
  { name: 'amount', short: 'a', long: '--amount', description: 'Amount of XRG to allocate', required: false, type: 'number' },
  { name: 'json', short: '', long: '--json', description: 'Output in JSON format', required: false, type: 'boolean' },
];

// ---------------------------------------------------------------------------
// Action handlers
// ---------------------------------------------------------------------------

async function listAction(options: Record<string, unknown>) {
  const json = options.json as boolean;
  const sortField = (options.sort as string) || 'apy';
  const minApy = options['min-apy'] as number | undefined;

  let pools = [...MOCK_POOLS];
  if (minApy !== undefined) pools = pools.filter((p) => p.apy >= minApy);

  const key = sortField as keyof Pool;
  pools.sort((a, b) => {
    const av = a[key], bv = b[key];
    if (typeof av === 'number' && typeof bv === 'number') return bv - av;
    return String(av).localeCompare(String(bv));
  });

  if (json) { console.log(JSON.stringify({ pools }, null, 2)); return; }

  console.log(colors.bold(colors.cyan('\n  Xergon Staking Pools\n')));
  new Table()
    .header(['Pool ID', 'Name', 'APY', 'TVL', 'Stakers', 'Status', 'Lock', 'Auto-Compound'])
    .rows(pools.map((p) => [
      colors.bold(p.id), p.name, colors.green(`${p.apy}%`), fmtXrg(p.tvl),
      String(p.stakers), statusColor(p.status), `${p.lockPeriod}d`,
      p.autoCompound ? colors.green('Yes') : colors.gray('No'),
    ]))
    .border(true)
    .render();
  console.log();
}

async function stakeAction(options: Record<string, unknown>) {
  const json = options.json as boolean;
  const poolId = options.pool as string;
  const amount = Number(options.amount);
  const autoCompound = options['auto-compound'] as boolean | undefined;

  if (isNaN(amount) || amount <= 0) { console.error(colors.red('Invalid amount')); process.exit(1); }

  const pool = findPoolOrExit(poolId);
  if (pool.status === 'full') { console.error(colors.red(`Pool ${poolId} is full`)); process.exit(1); }
  if (pool.status === 'paused') { console.error(colors.red(`Pool ${poolId} is paused`)); process.exit(1); }
  if (amount < pool.minStake) { console.error(colors.red(`Min stake for ${poolId} is ${pool.minStake} XRG`)); process.exit(1); }

  const txId = `stake_${Date.now().toString(36)}_${Math.random().toString(36).slice(2, 8)}`;

  if (json) { console.log(JSON.stringify({ txId, poolId, amount, autoCompound: autoCompound ?? false, status: 'pending' }, null, 2)); return; }

  console.log(colors.green(colors.bold('\n  Stake Submitted\n')));
  console.log(`  ${colors.bold('Tx ID')}          ${txId}`);
  console.log(`  ${colors.bold('Pool')}           ${pool.name} (${poolId})`);
  console.log(`  ${colors.bold('Amount')}         ${fmtXrg(amount)}`);
  console.log(`  ${colors.bold('APY')}            ${colors.green(`${pool.apy}%`)}`);
  console.log(`  ${colors.bold('Lock Period')}    ${pool.lockPeriod} days`);
  console.log(`  ${colors.bold('Auto-Compound')}  ${autoCompound ? colors.green('Enabled') : colors.gray('Disabled')}`);
  console.log();
}

async function unstakeAction(options: Record<string, unknown>) {
  const json = options.json as boolean;
  const poolId = options.pool as string;
  const amount = options.amount as number | undefined;
  const pool = findPoolOrExit(poolId);
  const unstakeAmount = amount ?? MOCK_POSITION.staked;

  if (typeof unstakeAmount === 'number' && (isNaN(unstakeAmount) || unstakeAmount <= 0)) {
    console.error(colors.red('Invalid amount')); process.exit(1);
  }

  const txId = `unstake_${Date.now().toString(36)}_${Math.random().toString(36).slice(2, 8)}`;
  const label = amount ? fmtXrg(unstakeAmount as number) : 'all';

  if (json) { console.log(JSON.stringify({ txId, poolId, amount: unstakeAmount, status: 'pending', unlockIn: `${pool.lockPeriod}d` }, null, 2)); return; }

  console.log(colors.yellow(colors.bold('\n  Unstake Initiated\n')));
  console.log(`  ${colors.bold('Tx ID')}          ${txId}`);
  console.log(`  ${colors.bold('Pool')}           ${pool.name} (${poolId})`);
  console.log(`  ${colors.bold('Amount')}         ${label}`);
  console.log(`  ${colors.bold('Unlocks In')}     ${pool.lockPeriod} days`);
  console.log();
}

async function delegateAction(options: Record<string, unknown>) {
  const json = options.json as boolean;
  const poolId = options.pool as string;
  const amount = Number(options.amount);

  if (isNaN(amount) || amount <= 0) { console.error(colors.red('Invalid amount')); process.exit(1); }

  const pool = findPoolOrExit(poolId);
  const txId = `delegate_${Date.now().toString(36)}_${Math.random().toString(36).slice(2, 8)}`;

  if (json) { console.log(JSON.stringify({ txId, poolId, amount, status: 'delegated' }, null, 2)); return; }

  console.log(colors.cyan(colors.bold('\n  Delegation Submitted\n')));
  console.log(`  ${colors.bold('Tx ID')}          ${txId}`);
  console.log(`  ${colors.bold('Pool')}           ${pool.name} (${poolId})`);
  console.log(`  ${colors.bold('Delegated')}      ${fmtXrg(amount)}`);
  console.log(`  ${colors.bold('APY')}            ${colors.green(`${pool.apy}%`)}`);
  console.log(`  ${colors.bold('Note')}           Delegation can be revoked at any time`);
  console.log();
}

async function claimAction(options: Record<string, unknown>) {
  const json = options.json as boolean;
  const poolId = options.pool as string;
  const compound = options.compound as boolean | undefined;
  const pool = findPoolOrExit(poolId);
  const claimed = MOCK_POSITION.rewards;
  const txId = `claim_${Date.now().toString(36)}_${Math.random().toString(36).slice(2, 8)}`;

  if (json) { console.log(JSON.stringify({ txId, poolId, claimed, compound: compound ?? false, status: 'completed' }, null, 2)); return; }

  console.log(colors.green(colors.bold(compound ? '\n  Rewards Compounded\n' : '\n  Rewards Claimed\n')));
  console.log(`  ${colors.bold('Tx ID')}          ${txId}`);
  console.log(`  ${colors.bold('Pool')}           ${pool.name} (${poolId})`);
  console.log(`  ${colors.bold('Rewards')}        ${fmtXrg(claimed)}`);
  console.log(`  ${colors.bold('Action')}         ${compound ? colors.cyan('Reinvested') : colors.green('Withdrawn')}`);
  console.log();
}

async function infoAction(options: Record<string, unknown>) {
  const json = options.json as boolean;
  const poolId = options.pool as string;
  const pool = findPoolOrExit(poolId);
  const pos = poolId === MOCK_POSITION.poolId ? MOCK_POSITION : null;

  if (json) { console.log(JSON.stringify({ pool, position: pos }, null, 2)); return; }

  console.log(colors.bold(colors.cyan(`\n  ${pool.name} (${poolId})\n`)));
  for (const [k, v] of [
    [colors.bold('Status'), statusColor(pool.status)],
    [colors.bold('APY'), colors.green(`${pool.apy}%`)],
    [colors.bold('TVL'), fmtXrg(pool.tvl)],
    [colors.bold('Stakers'), String(pool.stakers)],
    [colors.bold('Lock Period'), `${pool.lockPeriod} days`],
    [colors.bold('Min Stake'), fmtXrg(pool.minStake)],
    [colors.bold('Auto-Compound'), pool.autoCompound ? colors.green('Supported') : colors.gray('Not supported')],
  ] as [string, string][]) { console.log(`  ${(k as string).padEnd(20)} ${v}`); }

  if (pos) {
    console.log(colors.bold(colors.magenta('\n  Your Position\n')));
    for (const [k, v] of [
      [colors.bold('Staked'), fmtXrg(pos.staked)],
      [colors.bold('Pending Rewards'), colors.green(fmtXrg(pos.rewards))],
      [colors.bold('Auto-Compound'), pos.compoundEnabled ? colors.green('Enabled') : colors.gray('Disabled')],
      [colors.bold('Entry Date'), pos.entryDate.slice(0, 10)],
      [colors.bold('Unlock Date'), pos.unlockDate.slice(0, 10)],
    ] as [string, string][]) { console.log(`  ${(k as string).padEnd(20)} ${v}`); }
  } else {
    console.log(colors.gray('\n  You have no position in this pool'));
  }
  console.log();
}

async function apyAction(options: Record<string, unknown>) {
  const json = options.json as boolean;
  const period = (options.period as string) || '30d';

  if (!['7d', '30d', '90d'].includes(period)) { console.error(colors.red('Invalid period — use 7d, 30d, or 90d')); process.exit(1); }

  const apyData = MOCK_APY_HISTORY[period];
  if (!apyData) { console.error(colors.red(`No APY data for period: ${period}`)); process.exit(1); }

  const entries = Object.entries(apyData)
    .map(([id, apy]) => ({ id, apy, pool: MOCK_POOLS.find((p) => p.id === id)! }))
    .filter((e) => e.pool)
    .sort((a, b) => b.apy - a.apy);

  if (json) { console.log(JSON.stringify({ period, leaderboard: entries }, null, 2)); return; }

  console.log(colors.bold(colors.cyan(`\n  APY Leaderboard (${period})\n`)));
  new Table()
    .header(['Rank', 'Pool ID', 'Name', 'APY', 'Trend'])
    .rows(entries.map((e, i) => {
      const diff = e.apy - e.pool.apy;
      const trend = diff >= 0 ? colors.green(`+${diff.toFixed(1)}%`) : colors.red(`${diff.toFixed(1)}%`);
      return [`#${i + 1}`, colors.bold(e.id), e.pool.name, colors.green(`${e.apy}%`), trend];
    }))
    .border(true)
    .render();
  console.log();
}

async function suggestAction(options: Record<string, unknown>) {
  const json = options.json as boolean;
  const risk = (options.risk as string) || 'medium';
  const amount = options.amount as number | undefined;

  if (!['low', 'medium', 'high'].includes(risk)) { console.error(colors.red('Invalid risk — use low, medium, or high')); process.exit(1); }

  let candidates = MOCK_POOLS.filter((p) => p.status === 'active');
  if (risk === 'low') {
    candidates = candidates.filter((p) => p.lockPeriod <= 14 && p.tvl > 1_000_000);
    candidates.sort((a, b) => a.lockPeriod - b.lockPeriod);
  } else if (risk === 'high') {
    candidates = candidates.filter((p) => p.lockPeriod >= 30);
    candidates.sort((a, b) => b.apy - a.apy);
  } else {
    candidates.sort((a, b) => b.apy - a.apy);
  }

  const top3 = candidates.slice(0, 3);

  if (json) { console.log(JSON.stringify({ risk, suggestions: top3, amount }, null, 2)); return; }

  console.log(colors.bold(colors.cyan(`\n  Yield Optimization Suggestions (${risk} risk)\n`)));
  if (amount) console.log(`  Allocating ${colors.bold(fmtXrg(amount))} across top pools:\n`);

  new Table()
    .header(['#', 'Pool', 'APY', 'TVL', 'Lock', 'Min Stake', 'Rationale'])
    .rows(top3.map((p, i) => [
      String(i + 1), `${p.name} (${p.id})`, colors.green(`${p.apy}%`), fmtXrg(p.tvl),
      `${p.lockPeriod}d`, fmtXrg(p.minStake),
      colors.gray(risk === 'low' ? 'Short lock, high TVL stability' : risk === 'high' ? 'Maximum yield potential' : 'Balanced risk/reward'),
    ]))
    .border(true)
    .render();

  if (amount && top3.length > 0) {
    const avgApy = top3.reduce((s, p) => s + p.apy, 0) / top3.length;
    console.log(`  ${colors.bold('Est. Annual Yield')}  ${colors.green(fmtXrg(Math.round(amount * avgApy / 100)))} at avg ${avgApy.toFixed(1)}% APY`);
  }
  console.log();
}

// ---------------------------------------------------------------------------
// Command export
// ---------------------------------------------------------------------------

export const stakeCommand: Command = {
  name: 'stake',
  description: 'Staking pool management — stake, unstake, delegate, and track rewards',
  aliases: ['stk'],
  options: [],
  action: () => {},
  subcommands: [
    { name: 'list', description: 'List available staking pools', options: listOptions, action: listAction },
    { name: 'stake', description: 'Stake XRG into a pool', options: stakeOpts, action: stakeAction },
    { name: 'unstake', description: 'Unstake from a pool', options: unstakeOpts, action: unstakeAction },
    { name: 'delegate', description: 'Delegate staking power to a pool', options: delegateOpts, action: delegateAction },
    { name: 'claim', description: 'Claim pending rewards', options: claimOpts, action: claimAction },
    { name: 'info', description: 'Show pool details and staker position', options: infoOpts, action: infoAction },
    { name: 'apy', description: 'Show APY leaderboard across all pools', options: apyOpts, action: apyAction },
    { name: 'suggest', description: 'Yield optimization suggestions', options: suggestOpts, action: suggestAction },
  ],
};
