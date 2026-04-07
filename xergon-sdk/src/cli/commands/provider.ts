/**
 * CLI command: provider
 *
 * List and inspect network providers.
 */

import type { Command, CommandOption, ParsedArgs, CLIContext } from '../mod';
import type { Provider, LeaderboardEntry } from '../../types';

const providerOptions: CommandOption[] = [
  {
    name: 'health',
    short: '',
    long: '--health',
    description: 'Show provider health scores',
    required: false,
    type: 'boolean',
  },
  {
    name: 'region',
    short: '',
    long: '--region',
    description: 'Filter by region',
    required: false,
    type: 'string',
  },
  {
    name: 'json',
    short: '',
    long: '--json',
    description: 'Output as JSON',
    required: false,
    type: 'boolean',
  },
];

async function providerAction(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const showHealth = args.options.health === true;
  const region = args.options.region ? String(args.options.region) : undefined;
  const outputJson = args.options.json === true;

  try {
    const providers: Provider[] = await ctx.client.providers.list();

    let filtered = providers;
    if (region) {
      const term = region.toLowerCase();
      filtered = providers.filter((p: Provider) => p.region.toLowerCase().includes(term));
    }

    if (showHealth) {
      // Use leaderboard for health info
      try {
        const leaderboard: LeaderboardEntry[] = await ctx.client.leaderboard();
        const healthData = leaderboard.map((entry: LeaderboardEntry) => ({
          Provider: `${entry.publicKey.substring(0, 12)}...`,
          Region: entry.region,
          PoWnScore: String(entry.pownScore),
          Models: entry.models.join(', '),
          Requests: String(entry.totalRequests ?? 0),
          Tokens: String(entry.totalTokens ?? 0),
          Online: entry.online ? 'Yes' : 'No',
        }));

        if (outputJson) {
          ctx.output.write(ctx.output.formatOutput(healthData));
        } else {
          ctx.output.write(ctx.output.formatTable(healthData, 'Provider Health'));
        }
      } catch {
        // Fallback: show providers with basic info
        const healthData = filtered.map((p: Provider) => ({
          Provider: `${p.publicKey.substring(0, 12)}...`,
          Region: p.region,
          PoWnScore: String(p.pownScore),
          Models: p.models.join(', '),
          Endpoint: p.endpoint,
        }));

        if (outputJson) {
          ctx.output.write(ctx.output.formatOutput(healthData));
        } else {
          ctx.output.write(ctx.output.formatTable(healthData, 'Provider Health'));
        }
      }
    } else {
      const tableData = filtered.map((p: Provider) => ({
        Provider: `${p.publicKey.substring(0, 12)}...`,
        Region: p.region,
        PoWnScore: String(p.pownScore),
        Models: p.models.join(', ') || '-',
      }));

      if (outputJson) {
        ctx.output.write(ctx.output.formatOutput(filtered));
      } else if (filtered.length === 0) {
        ctx.output.info(region ? `No providers found in region "${region}".` : 'No providers available.');
      } else {
        ctx.output.write(ctx.output.formatTable(tableData, `Providers (${filtered.length})`));
      }
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to list providers: ${message}`);
    process.exit(1);
  }
}

export const providerCommand: Command = {
  name: 'provider',
  description: 'List and inspect network providers',
  aliases: ['providers', 'node'],
  options: providerOptions,
  action: providerAction,
};
