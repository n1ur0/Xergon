/**
 * CLI command: version
 *
 * Show SDK version information.
 */

import type { Command, ParsedArgs, CLIContext } from '../mod';

const VERSION = '0.1.0';

async function versionAction(_args: ParsedArgs, ctx: CLIContext): Promise<void> {
  ctx.output.write(`xergon-cli v${VERSION}\n`);
}

export const versionCommand: Command = {
  name: 'version',
  description: 'Show CLI version',
  aliases: ['ver', 'v'],
  options: [],
  action: versionAction,
};

export { VERSION };
