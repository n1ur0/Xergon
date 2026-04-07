/**
 * CLI command: balance
 *
 * View account balance and transaction history.
 */

import type { Command, CommandOption, ParsedArgs, CLIContext } from '../mod';

const balanceOptions: CommandOption[] = [
  {
    name: 'history',
    short: '',
    long: '--history',
    description: 'Show transaction history',
    required: false,
    type: 'boolean',
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

async function balanceAction(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const showHistory = args.options.history === true;
  const outputJson = args.options.json === true;

  const publicKey = ctx.config.apiKey;
  if (!publicKey) {
    ctx.output.writeError('No public key configured. Run: xergon config set api-key <key>');
    process.exit(1);
  }

  try {
    const balance = await ctx.client.balance.get(publicKey);

    if (outputJson) {
      ctx.output.write(ctx.output.formatOutput(balance));
    } else {
      const data: Record<string, string> = {
        'Public Key': `${balance.publicKey.substring(0, 16)}...${balance.publicKey.substring(balance.publicKey.length - 8)}`,
        'Balance (ERG)': balance.balanceErg,
        'Balance (NanoERG)': balance.balanceNanoerg,
      };
      if (balance.stakingBoxId) {
        data['Staking Box'] = `${balance.stakingBoxId.substring(0, 16)}...`;
      }
      ctx.output.write(ctx.output.formatText(data, 'Account Balance'));
    }

    if (showHistory) {
      ctx.output.info('Transaction history is available through the Ergo blockchain explorer.');
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to fetch balance: ${message}`);
    process.exit(1);
  }
}

export const balanceCommand: Command = {
  name: 'balance',
  description: 'Show account balance',
  aliases: ['bal', 'wallet'],
  options: balanceOptions,
  action: balanceAction,
};
