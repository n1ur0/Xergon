/**
 * Xergon Provider V2 CLI Commands
 *
 * On-chain provider lifecycle operations:
 * - register:   Register a new provider on the Ergo blockchain
 * - heartbeat:  Submit a heartbeat to update on-chain state
 * - status:     Check provider status (on-chain + in-memory)
 * - inspect:    Inspect a provider box on-chain
 * - rent-check: Check storage rent protection status
 * - deregister: Deregister a provider and sink NFT
 * - list:       List all on-chain providers
 * - history:    Get heartbeat and event history
 *
 * Usage: xergon provider-v2 <command> [options]
 */

import { CommandModule } from 'yargs';

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const STORAGE_RENT_THRESHOLD_BLOCKS = 1_051_200;
const BLOCKS_PER_MINUTE = 0.5;
const RENT_RISK_THRESHOLD = 100_000;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Format block countdown to human-readable "X years Y days". */
export function formatRentCountdown(blocksRemaining: number): string {
  if (blocksRemaining <= 0) return 'EXPIRED — rent collection active';
  const totalMinutes = blocksRemaining / BLOCKS_PER_MINUTE;
  const totalHours = totalMinutes / 60;
  const totalDays = totalHours / 24;
  const years = Math.floor(totalDays / 365.25);
  const remainingDays = Math.floor(totalDays - years * 365.25);
  if (years > 0) return `${years}y ${remainingDays}d`;
  if (remainingDays > 0) return `${remainingDays}d`;
  return `${Math.floor(totalHours)}h`;
}

/** Format provider status with ANSI color indicators. */
export function formatProviderStatus(status: string): string {
  const map: Record<string, string> = {
    active: '\x1b[32m● ACTIVE\x1b[0m',
    registering: '\x1b[33m◐ REGISTERING\x1b[0m',
    heartbeat_pending: '\x1b[33m◐ HEARTBEAT_PENDING\x1b[0m',
    rent_protection_needed: '\x1b[31m⚠ RENT_PROTECTION_NEEDED\x1b[0m',
    deregistering: '\x1b[31m◉ DEREGISTERING\x1b[0m',
    inactive: '\x1b[90m○ INACTIVE\x1b[0m',
    valid: '\x1b[32m✓ VALID\x1b[0m',
    invalid: '\x1b[31m✗ INVALID\x1b[0m',
  };
  return map[status] || `\x1b[37m${status.toUpperCase()}\x1b[0m`;
}

/** Display a table of providers with aligned columns. */
export function displayProviderTable(providers: Array<{
  provider_pubkey: string;
  status?: string;
  endpoint?: string;
  pown_score?: number;
  models?: string[];
  is_valid?: boolean;
  rent_status?: { blocks_until_rent: number; is_at_risk: boolean };
}>): void {
  if (providers.length === 0) {
    console.log('\x1b[90mNo providers found.\x1b[0m');
    return;
  }

  const COL = { pubkey: 22, status: 26, endpoint: 30, pown: 6, models: 20, rent: 16 };
  const header = `${'PUBKEY'.padEnd(COL.pubkey)}${'STATUS'.padEnd(COL.status)}${'ENDPOINT'.padEnd(COL.endpoint)}${'PoNW'.padEnd(COL.pown)}${'MODELS'.padEnd(COL.models)}RENT COUNTDOWN`;
  const sep = '─'.repeat(COL.pubkey + COL.status + COL.endpoint + COL.pown + COL.models + COL.rent);

  console.log(`\n${header}\n${sep}`);
  for (const p of providers) {
    const pk = (p.provider_pubkey || '?').slice(0, COL.pubkey - 1).padEnd(COL.pubkey);
    const st = formatProviderStatus(p.status || (p.is_valid ? 'valid' : 'invalid')).padEnd(COL.status);
    const ep = (p.endpoint || '?').slice(0, COL.endpoint - 1).padEnd(COL.endpoint);
    const pw = String(p.pown_score ?? 0).padEnd(COL.pown);
    const md = (p.models || []).join(',').slice(0, COL.models - 1).padEnd(COL.models);
    const rc = p.rent_status ? formatRentCountdown(p.rent_status.blocks_until_rent) : 'N/A';
    console.log(`${pk}${st}${ep}${pw}${md}${rc}`);
  }
  console.log(`\nTotal: ${providers.length} provider(s)\n`);
}

/** Display a vertical timeline of lifecycle events. */
export function displayTimeline(events: Array<{
  event_type: string;
  height: number;
  timestamp: string;
  details: string;
}>): void {
  if (events.length === 0) {
    console.log('\x1b[90mNo events found.\x1b[0m');
    return;
  }

  console.log('\n\x1b[1mLifecycle Timeline\x1b[0m\n');
  const sorted = [...events].sort((a, b) => a.height - b.height);
  for (let i = 0; i < sorted.length; i++) {
    const e = sorted[i];
    const connector = i === sorted.length - 1 ? '└──' : '├──';
    const typeColor = e.event_type.includes('Deregister') || e.event_type.includes('Expired')
      ? '\x1b[31m' : e.event_type.includes('Heartbeat') ? '\x1b[33m' : '\x1b[36m';
    const typeStr = `${typeColor}${e.event_type.toUpperCase()}\x1b[0m`;
    console.log(`  ${connector} H:${e.height.toString().padStart(8)}  ${typeStr}`);
    console.log(`  ${i === sorted.length - 1 ? '    ' : '│  '} ${new Date(e.timestamp).toLocaleString()}`);
    console.log(`  ${i === sorted.length - 1 ? '    ' : '│  '} ${e.details}`);
    if (i < sorted.length - 1) console.log('  │');
  }
  console.log('');
}

/** Interactive confirmation prompt (mock for CLI). */
export async function confirmAction(message: string): Promise<boolean> {
  // In real CLI, this would use readline. For testing, default to true.
  process.stdout.write(`${message} [y/N] `);
  return true;
}

/** Calculate years from blocks. */
function blocksToYears(blocks: number): number {
  return blocks * BLOCKS_PER_MINUTE / (60 * 24 * 365.25);
}

/** Format nanoERG to ERG. */
function formatNanoErg(nanoErg: number): string {
  return (nanoErg / 1_000_000_000).toFixed(6);
}

/** Make a fetch request to the agent or relay. */
async function apiFetch(baseUrl: string, path: string, options?: RequestInit): Promise<any> {
  try {
    const res = await fetch(`${baseUrl}${path}`, {
      headers: { 'Content-Type': 'application/json', ...options?.headers },
      ...options,
    });
    return { ok: res.ok, status: res.status, data: await res.json() };
  } catch (err: any) {
    return { ok: false, status: 0, error: err.message };
  }
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

const registerCmd: CommandModule = {
  command: 'register',
  describe: 'Register a new provider on-chain',
  builder: (yargs) => yargs
    .option('endpoint', { type: 'string', demandOption: true, describe: 'Provider endpoint URL' })
    .option('models', { type: 'string', describe: 'Comma-separated model list' })
    .option('metadata-url', { type: 'string', describe: 'Metadata URL' })
    .option('pown-score', { type: 'number', default: 0, describe: 'Initial PoNW score' })
    .option('value-erg', { type: 'number', default: 0.1, describe: 'Box value in ERG' })
    .option('agent-url', { type: 'string', default: 'http://localhost:9090', describe: 'Agent URL' })
    .option('json', { type: 'boolean', default: false, describe: 'JSON output' }),
  handler: async (argv: any) => {
    const models = argv.models ? argv.models.split(',').map((s: string) => s.trim()) : undefined;
    const body = {
      pubkey: `provider_${Date.now()}`,
      endpoint: argv.endpoint,
      models,
      pown_score: argv.pownScore,
      value_erg: argv.valueErg,
    };

    const res = await apiFetch(argv.agentUrl, '/xergon/lifecycle/register', {
      method: 'POST',
      body: JSON.stringify(body),
    });

    if (argv.json) {
      console.log(JSON.stringify(res, null, 2));
      return;
    }

    if (!res.ok) {
      console.error(`\x1b[31mError: ${res.data?.error || res.error}\x1b[0m`);
      process.exit(1);
    }

    const lc = res.data.lifecycle;
    console.log(`\x1b[32mProvider registered successfully\x1b[0m`);
    console.log(`  Box ID:    ${lc.box_id}`);
    console.log(`  NFT:       ${lc.nft_token_id}`);
    console.log(`  Pubkey:    ${lc.provider_pubkey}`);
    console.log(`  Endpoint:  ${argv.endpoint}`);
    console.log(`  Value:     ${argv.valueErg} ERG`);
    console.log(`\x1b[33m⚠ Storage rent: boxes expire after ~4 years (${STORAGE_RENT_THRESHOLD_BLOCKS} blocks).\x1b[0m`);
    console.log(`  Submit heartbeats regularly to stay active.`);
  },
};

const heartbeatCmd: CommandModule = {
  command: 'heartbeat',
  describe: 'Submit a heartbeat for a provider',
  builder: (yargs) => yargs
    .option('pubkey', { type: 'string', demandOption: true, alias: 'p', describe: 'Provider pubkey' })
    .option('pown-score', { type: 'number', describe: 'Updated PoNW score' })
    .option('models', { type: 'string', describe: 'Updated comma-separated model list' })
    .option('agent-url', { type: 'string', default: 'http://localhost:9090', describe: 'Agent URL' })
    .option('json', { type: 'boolean', default: false, describe: 'JSON output' }),
  handler: async (argv: any) => {
    const models = argv.models ? argv.models.split(',').map((s: string) => s.trim()) : undefined;
    const body = { pubkey: argv.pubkey, pown_score: argv.pownScore, models };

    const res = await apiFetch(argv.agentUrl, '/xergon/lifecycle/heartbeat', {
      method: 'POST',
      body: JSON.stringify(body),
    });

    if (argv.json) { console.log(JSON.stringify(res, null, 2)); return; }

    if (!res.ok) {
      console.error(`\x1b[31mError: ${res.data?.error || res.error}\x1b[0m`);
      process.exit(1);
    }

    const lc = res.data.lifecycle;
    console.log(`\x1b[32mHeartbeat submitted\x1b[0m`);
    console.log(`  Total heartbeats: ${lc.total_heartbeats}`);
    console.log(`  Last heartbeat:   H:${lc.last_heartbeat_height}`);
    console.log(`  Status:           ${formatProviderStatus(lc.status)}`);
  },
};

const statusCmd: CommandModule = {
  command: 'status',
  describe: 'Check provider status (on-chain + in-memory)',
  builder: (yargs) => yargs
    .option('pubkey', { type: 'string', demandOption: true, alias: 'p', describe: 'Provider pubkey' })
    .option('chain', { type: 'boolean', default: false, describe: 'Also check on-chain box' })
    .option('agent-url', { type: 'string', default: 'http://localhost:9090', describe: 'Agent URL' })
    .option('relay-url', { type: 'string', default: 'http://localhost:9091', describe: 'Relay URL' })
    .option('json', { type: 'boolean', default: false, describe: 'JSON output' }),
  handler: async (argv: any) => {
    const res = await apiFetch(argv.agentUrl, `/xergon/lifecycle/status/${argv.pubkey}`);

    if (argv.json) {
      console.log(JSON.stringify(res, null, 2));
      return;
    }

    if (!res.ok) {
      console.error(`\x1b[31mError: ${res.data?.error || res.error}\x1b[0m`);
      process.exit(1);
    }

    const lc = res.data.lifecycle;
    console.log(`\x1b[1mProvider Status: ${argv.pubkey}\x1b[0m`);
    console.log(`  Status:       ${formatProviderStatus(lc.status)}`);
    console.log(`  Box ID:       ${lc.box_id}`);
    console.log(`  NFT:          ${lc.nft_token_id}`);
    console.log(`  Registered:   H:${lc.registered_at_height}`);
    console.log(`  Last HB:      H:${lc.last_heartbeat_height}`);
    console.log(`  Total HBs:    ${lc.total_heartbeats}`);
    console.log(`  Missed HBs:   ${lc.consecutive_missed_heartbeats}`);
    console.log(`  Value:        ${formatNanoErg(lc.creation_value_nanoerg)} ERG`);

    if (argv.chain) {
      const chainRes = await apiFetch(argv.relayUrl, `/v1/chain/providers/${argv.pubkey}`);
      if (chainRes.ok && chainRes.data.provider) {
        const box = chainRes.data.provider;
        console.log(`\n\x1b[1mOn-Chain Box\x1b[0m`);
        console.log(`  Valid:        ${formatProviderStatus(box.is_valid ? 'valid' : 'invalid')}`);
        console.log(`  Endpoint:     ${box.registers?.endpoint || 'N/A'}`);
        console.log(`  PoNW:         ${box.registers?.pown_score || 0}`);
        console.log(`  Models:       ${(box.registers?.models_served || []).join(', ')}`);
        console.log(`  Rent:         ${formatRentCountdown(box.rent_status?.blocks_until_rent ?? 0)}`);
        if (box.validation_error) console.log(`  Error:        ${box.validation_error}`);
      } else {
        console.log(`\n\x1b[90mNo on-chain box found for ${argv.pubkey}\x1b[0m`);
      }
    }
  },
};

const inspectCmd: CommandModule = {
  command: 'inspect',
  describe: 'Inspect a provider box on-chain',
  builder: (yargs) => yargs
    .option('pubkey', { type: 'string', demandOption: true, alias: 'p', describe: 'Provider pubkey' })
    .option('relay-url', { type: 'string', default: 'http://localhost:9091', describe: 'Relay URL' })
    .option('json', { type: 'boolean', default: false, describe: 'JSON output' }),
  handler: async (argv: any) => {
    const res = await apiFetch(argv.relayUrl, `/v1/chain/providers/${argv.pubkey}`);

    if (argv.json) { console.log(JSON.stringify(res, null, 2)); return; }

    if (!res.ok) {
      console.error(`\x1b[31mError: ${res.data?.error || res.error}\x1b[0m`);
      process.exit(1);
    }

    const box = res.data.provider;
    console.log(`\x1b[1mProvider Box Inspection: ${argv.pubkey}\x1b[0m`);
    console.log(`  Box ID:       ${box.box_id}`);
    console.log(`  Valid:        ${formatProviderStatus(box.is_valid ? 'valid' : 'invalid')}`);
    console.log(`  NFT Token:    ${box.nft_token_id}`);
    console.log(`  Value:        ${formatNanoErg(box.value_nanoerg)} ERG`);
    console.log(`  Created:      H:${box.creation_height}`);
    console.log(`\n  Registers:`);
    console.log(`    R4 Endpoint:  ${box.registers?.endpoint || 'N/A'}`);
    console.log(`    R5 PoNW:      ${box.registers?.pown_score ?? 'N/A'}`);
    console.log(`    R6 Models:    ${(box.registers?.models_served || []).join(', ') || 'N/A'}`);
    console.log(`    R7 Metadata:  ${box.registers?.metadata_url || 'N/A'}`);
    console.log(`    R8 Heartbeat: H:${box.registers?.last_heartbeat_height ?? 'N/A'}`);
    console.log(`\n  Storage Rent:`);
    console.log(`    Blocks left:  ${box.rent_status?.blocks_until_rent ?? 'N/A'}`);
    console.log(`    Years left:   ${box.rent_status?.years_until_rent?.toFixed(2) ?? 'N/A'}`);
    console.log(`    At risk:      ${box.rent_status?.is_at_risk ? '\x1b[31mYES\x1b[0m' : '\x1b[32mNo\x1b[0m'}`);
    console.log(`    Est. cost:    ${box.rent_status?.estimated_rent_erg ?? 'N/A'} ERG/cycle`);
    console.log(`    Level:        ${box.rent_status?.risk_level || 'N/A'}`);
    if (box.validation_error) console.log(`\n  \x1b[31mValidation: ${box.validation_error}\x1b[0m`);
  },
};

const rentCheckCmd: CommandModule = {
  command: 'rent-check',
  describe: 'Check storage rent protection status for all providers',
  builder: (yargs) => yargs
    .option('threshold-blocks', { type: 'number', default: 900_000, describe: 'Rent risk threshold in blocks' })
    .option('agent-url', { type: 'string', default: 'http://localhost:9090', describe: 'Agent URL' })
    .option('json', { type: 'boolean', default: false, describe: 'JSON output' }),
  handler: async (argv: any) => {
    const res = await apiFetch(argv.agentUrl, '/xergon/lifecycle/rent-check');

    if (argv.json) { console.log(JSON.stringify(res, null, 2)); return; }

    if (!res.ok) {
      console.error(`\x1b[31mError: ${res.data?.error || res.error}\x1b[0m`);
      process.exit(1);
    }

    const providers = res.data.providers_needing_protection || [];
    if (providers.length === 0) {
      console.log('\x1b[32mAll providers are within safe storage rent thresholds.\x1b[0m');
      return;
    }

    console.log(`\x1b[33m⚠ ${providers.length} provider(s) need rent protection:\x1b[0m\n`);
    for (const p of providers) {
      console.log(`  \x1b[31m● ${p.provider_pubkey}\x1b[0m`);
      console.log(`    Box:      ${p.box_id}`);
      console.log(`    Created:  H:${p.registered_at_height}`);
      console.log(`    Value:    ${formatNanoErg(p.creation_value_nanoerg)} ERG`);
    }
    console.log(`\nRun \x1b[1mxergon provider-v2 rent-protect --pubkey <PK>\x1b[0m to protect.`);
  },
};

const deregisterCmd: CommandModule = {
  command: 'deregister',
  describe: 'Deregister a provider and sink NFT',
  builder: (yargs) => yargs
    .option('pubkey', { type: 'string', demandOption: true, alias: 'p', describe: 'Provider pubkey' })
    .option('force', { type: 'boolean', default: false, alias: 'f', describe: 'Skip confirmation' })
    .option('agent-url', { type: 'string', default: 'http://localhost:9090', describe: 'Agent URL' })
    .option('json', { type: 'boolean', default: false, describe: 'JSON output' }),
  handler: async (argv: any) => {
    if (!argv.force) {
      const confirmed = await confirmAction(`Deregister provider ${argv.pubkey}? NFT will be sunk.`);
      if (!confirmed) {
        console.log('Cancelled.');
        return;
      }
    }

    const res = await apiFetch(argv.agentUrl, '/xergon/lifecycle/deregister', {
      method: 'POST',
      body: JSON.stringify({ pubkey: argv.pubkey }),
    });

    if (argv.json) { console.log(JSON.stringify(res, null, 2)); return; }

    if (!res.ok) {
      console.error(`\x1b[31mError: ${res.data?.error || res.error}\x1b[0m`);
      process.exit(1);
    }

    console.log(`\x1b[32mProvider ${argv.pubkey} deregistered.\x1b[0m`);
    console.log(`  NFT sunk, ERG returned to wallet.`);
  },
};

const listCmd: CommandModule = {
  command: 'list',
  describe: 'List all on-chain providers',
  builder: (yargs) => yargs
    .option('status', { type: 'string', describe: 'Filter by status' })
    .option('sort', { type: 'string', default: 'pown_score', choices: ['pown_score', 'box_age', 'value'] })
    .option('limit', { type: 'number', default: 20, describe: 'Max providers to show' })
    .option('relay-url', { type: 'string', default: 'http://localhost:9091', describe: 'Relay URL' })
    .option('json', { type: 'boolean', default: false, describe: 'JSON output' }),
  handler: async (argv: any) => {
    const res = await apiFetch(argv.relayUrl, '/v1/chain/providers');

    if (argv.json) { console.log(JSON.stringify(res, null, 2)); return; }

    if (!res.ok) {
      console.error(`\x1b[31mError: ${res.data?.error || res.error}\x1b[0m`);
      process.exit(1);
    }

    let providers = res.data.providers || [];
    if (argv.status) providers = providers.filter((p: any) => p.registers?.endpoint?.includes(argv.status));
    providers = providers.slice(0, argv.limit);

    displayProviderTable(providers.map((p: any) => ({
      provider_pubkey: p.provider_pubkey,
      status: p.is_valid ? 'valid' : 'invalid',
      endpoint: p.registers?.endpoint,
      pown_score: p.registers?.pown_score,
      models: p.registers?.models_served,
      is_valid: p.is_valid,
      rent_status: p.rent_status,
    })));
  },
};

const historyCmd: CommandModule = {
  command: 'history',
  describe: 'Get heartbeat and event history for a provider',
  builder: (yargs) => yargs
    .option('pubkey', { type: 'string', demandOption: true, alias: 'p', describe: 'Provider pubkey' })
    .option('limit', { type: 'number', default: 50, describe: 'Max events to show' })
    .option('agent-url', { type: 'string', default: 'http://localhost:9090', describe: 'Agent URL' })
    .option('json', { type: 'boolean', default: false, describe: 'JSON output' }),
  handler: async (argv: any) => {
    const res = await apiFetch(argv.agentUrl, `/xergon/lifecycle/history/${argv.pubkey}`);

    if (argv.json) { console.log(JSON.stringify(res, null, 2)); return; }

    if (!res.ok) {
      console.error(`\x1b[31mError: ${res.data?.error || res.error}\x1b[0m`);
      process.exit(1);
    }

    const { heartbeats = [], events = [] } = res.data;
    console.log(`\x1b[1mProvider History: ${argv.pubkey}\x1b[0m`);
    console.log(`  Heartbeats: ${heartbeats.length}`);
    console.log(`  Events:     ${events.length}`);

    if (events.length > 0) {
      displayTimeline(events.slice(-argv.limit));
    }

    if (heartbeats.length > 0) {
      console.log('\x1b[1mRecent Heartbeats\x1b[0m\n');
      const recent = heartbeats.slice(-Math.min(10, argv.limit));
      for (const hb of recent) {
        console.log(`  H:${String(hb.height).padStart(8)}  PoNW:${String(hb.pown_score).padStart(4)}  Models:${hb.models_count}  ${hb.tx_id}`);
      }
    }
  },
};

// ---------------------------------------------------------------------------
// Main Command
// ---------------------------------------------------------------------------

export const providerV2Command: CommandModule = {
  command: 'provider-v2',
  describe: 'On-chain provider lifecycle management (register, heartbeat, rent protection, deregister)',
  builder: (yargs) => yargs
    .command(registerCmd)
    .command(heartbeatCmd)
    .command(statusCmd)
    .command(inspectCmd)
    .command(rentCheckCmd)
    .command(deregisterCmd)
    .command(listCmd)
    .command(historyCmd)
    .demandCommand(1, 'Specify a provider-v2 subcommand'),
  handler: () => {},
};
