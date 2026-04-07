/**
 * CLI command: completion
 *
 * Generate shell completion scripts for bash, zsh, and fish.
 *
 *   xergon completion bash > /etc/bash_completion.d/xergon
 *   xergon completion zsh  > ~/.zfunc/_xergon
 *   xergon completion fish > ~/.config/fish/completions/xergon.fish
 */

import type { Command, CommandOption, ParsedArgs, CLIContext } from '../mod';
import { generateCompletionScript } from '../completions';

const completionOptions: CommandOption[] = [];

async function completionAction(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const shell = args.positional[0];

  if (!shell || !['bash', 'zsh', 'fish'].includes(shell)) {
    ctx.output.writeError('Usage: xergon completion <bash|zsh|fish>');
    ctx.output.info('Generate shell completion script. Pipe output to a file:');
    ctx.output.write('');
    ctx.output.write('  xergon completion bash > /etc/bash_completion.d/xergon');
    ctx.output.write('  xergon completion zsh  > ~/.zfunc/_xergon');
    ctx.output.write('  xergon completion fish > ~/.config/fish/completions/xergon.fish');
    process.exit(1);
    return; // unreachable
  }

  const script = generateCompletionScript(shell as 'bash' | 'zsh' | 'fish');
  process.stdout.write(script);
}

export const completionCommand: Command = {
  name: 'completion',
  description: 'Generate shell completion script',
  aliases: [],
  options: completionOptions,
  action: completionAction,
};
