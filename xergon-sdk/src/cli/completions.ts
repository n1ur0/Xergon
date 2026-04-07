/**
 * Shell completion scripts for the Xergon CLI.
 *
 * Generates bash, zsh, and fish completion scripts that can be
 * piped to a file and sourced:
 *
 *   xergon completion bash > /etc/bash_completion.d/xergon
 *   xergon completion zsh  > ~/.zfunc/_xergon
 *   xergon completion fish > ~/.config/fish/completions/xergon.fish
 */

import type { Command, CommandOption } from './mod';

// ── Command/option definitions (mirrors the real registry) ───────

const COMMANDS: Array<{ name: string; aliases: string[]; desc: string; options: CommandOption[] }> = [
  {
    name: 'chat',
    aliases: ['ask', 'complete'],
    desc: 'Send a chat completion request',
    options: [
      { name: 'model', short: '-m', long: '--model', description: 'Model to use for completion', required: false, type: 'string' },
      { name: 'system', short: '-s', long: '--system', description: 'System prompt', required: false, type: 'string' },
      { name: 'temperature', short: '-t', long: '--temperature', description: 'Sampling temperature (0.0 - 2.0)', required: false, type: 'number' },
      { name: 'maxTokens', short: '-n', long: '--max-tokens', description: 'Maximum tokens in response', required: false, type: 'number' },
      { name: 'stream', short: '', long: '--stream', description: 'Stream the response token by token', required: false, type: 'boolean' },
      { name: 'interactive', short: '-i', long: '--interactive', description: 'Start interactive REPL mode', required: false, type: 'boolean' },
      { name: 'json', short: '', long: '--json', description: 'Output response as JSON', required: false, type: 'boolean' },
    ],
  },
  {
    name: 'models',
    aliases: ['list-models'],
    desc: 'List available models on the relay',
    options: [
      { name: 'json', short: '', long: '--json', description: 'Output as JSON', required: false, type: 'boolean' },
    ],
  },
  {
    name: 'provider',
    aliases: ['providers', 'node'],
    desc: 'Show relay provider/node information',
    options: [
      { name: 'json', short: '', long: '--json', description: 'Output as JSON', required: false, type: 'boolean' },
    ],
  },
  {
    name: 'balance',
    aliases: ['bal', 'wallet-balance'],
    desc: 'Show account balance',
    options: [
      { name: 'json', short: '', long: '--json', description: 'Output as JSON', required: false, type: 'boolean' },
    ],
  },
  {
    name: 'config',
    aliases: ['settings', 'cfg'],
    desc: 'View and manage CLI configuration',
    options: [
      { name: 'set', short: '', long: '--set', description: 'Set a config value (key=value)', required: false, type: 'string' },
      { name: 'json', short: '', long: '--json', description: 'Output as JSON', required: false, type: 'boolean' },
    ],
  },
  {
    name: 'version',
    aliases: ['v', '-v'],
    desc: 'Show CLI version',
    options: [],
  },
  {
    name: 'onboard',
    aliases: ['setup', 'init'],
    desc: 'Interactive first-run setup wizard',
    options: [
      { name: 'json', short: '', long: '--json', description: 'Output as JSON', required: false, type: 'boolean' },
    ],
  },
  {
    name: 'login',
    aliases: ['auth'],
    desc: 'Authenticate with Xergon relay',
    options: [
      { name: 'key', short: '', long: '--key', description: 'API key (non-interactive mode)', required: false, type: 'string' },
      { name: 'wallet', short: '', long: '--wallet', description: 'Open ErgoAuth wallet flow', required: false, type: 'boolean' },
    ],
  },
  {
    name: 'logout',
    aliases: [],
    desc: 'Clear stored credentials',
    options: [],
  },
  {
    name: 'completion',
    aliases: [],
    desc: 'Generate shell completion script',
    options: [],
  },
];

const GLOBAL_OPTIONS = [
  { name: 'json', short: '-j', long: '--json', description: 'Output in JSON format' },
  { name: 'config', short: '', long: '--config', description: 'Path to config file' },
  { name: 'help', short: '-h', long: '--help', description: 'Show help' },
];

const ALL_COMMAND_NAMES = COMMANDS.flatMap(c => [c.name, ...c.aliases]);

// ── Helper to build flag strings ────────────────────────────────

function flagsFor(cmd: typeof COMMANDS[0]): string[] {
  const flags: string[] = [];
  for (const opt of cmd.options) {
    if (opt.long) flags.push(opt.long);
    if (opt.short) flags.push(opt.short);
  }
  return flags;
}

function allFlagsFor(cmd: typeof COMMANDS[0]): string[] {
  return [...flagsFor(cmd), ...GLOBAL_OPTIONS.map(o => o.long)];
}

// ── Bash completion ────────────────────────────────────────────

function generateBash(): string {
  const lines: string[] = [];

  lines.push('#!/bin/bash');
  lines.push('# Xergon CLI bash completion');
  lines.push('# Install: xergon completion bash > /etc/bash_completion.d/xergon');
  lines.push('#    or:  xergon completion bash >> ~/.bash_completion');
  lines.push('');
  lines.push('_xergon_completions() {');
  lines.push('  local cur prev words cword');
  lines.push('  _init_completion -n = || return');
  lines.push('');

  // Command list
  lines.push(`  local commands="${ALL_COMMAND_NAMES.join(' ')}"`);
  lines.push('');

  // If on the first word after "xergon", complete commands
  lines.push('  if [[ $cword -eq 1 ]]; then');
  lines.push('    COMPREPLY=( $(compgen -W "$commands" -- "$cur") )');
  lines.push('    return 0');
  lines.push('  fi');
  lines.push('');

  // Determine the command name
  lines.push('  local cmd="${words[1]}"');
  lines.push('  local flags=""');
  lines.push('');

  // Per-command flags
  for (const cmd of COMMANDS) {
    const names = [cmd.name, ...cmd.aliases];
    const conditions = names.map(n => `    "${n}"`).join('\n      || ');
    const flags = allFlagsFor(cmd).join(' ');
    lines.push(`  if ${conditions}; then`);
    lines.push(`    flags="${flags}"`);
    lines.push('  fi');
    lines.push('');
  }

  lines.push('  # Complete flags');
  lines.push('  case "$cur" in');
  lines.push('    --*)');
  lines.push('      COMPREPLY=( $(compgen -W "$flags" -- "$cur") )');
  lines.push('      ;;');
  lines.push('    -*)');
  lines.push('      COMPREPLY=( $(compgen -W "$flags" -- "$cur") )');
  lines.push('      ;;');
  lines.push('  esac');
  lines.push('');
  lines.push('  # Also complete flags when prev is a flag that takes a value');
  lines.push('  case "$prev" in');
  lines.push('    --model|-m|--system|-s|--config|--set|--key)');
  lines.push('      # File / value completion -- let bash default handle it');
  lines.push('      ;;');
  lines.push('    --temperature|-t|--max-tokens|-n)');
  lines.push('      ;;');
  lines.push('  esac');
  lines.push('}');
  lines.push('');
  lines.push('complete -F _xergon_completions xergon');
  lines.push('');

  return lines.join('\n');
}

// ── Zsh completion ─────────────────────────────────────────────

function generateZsh(): string {
  const lines: string[] = [];

  lines.push('#compdef xergon');
  lines.push('# Xergon CLI zsh completion');
  lines.push('# Install: xergon completion zsh > ~/.zfunc/_xergon');
  lines.push('#          then add: fpath=(~/.zfunc $fpath) && autoload -U compinit && compinit');
  lines.push('');
  lines.push('_xergon() {');
  lines.push('  local -a args');
  lines.push('  local context state state_descr line');
  lines.push('  typeset -A opt_args');
  lines.push('');

  // Build subcommands
  lines.push('  _arguments -C \\');
  lines.push('    "--json[Output in JSON format]" \\');
  lines.push('    "--config[Path to config file]:config:_files" \\');
  lines.push('    "--help[Show help]" \\');
  lines.push('    "-h[Show help]" \\');
  lines.push('    ":command:->command" \\');
  lines.push('    "*::args:->args"');
  lines.push('');

  lines.push('  case $state in');
  lines.push('    command)');

  for (const cmd of COMMANDS) {
    const aliases = cmd.aliases.length > 0 ? `:${cmd.aliases.join(':')}:` : '';
    lines.push(`      "${cmd.name}${aliases}:${cmd.desc}" \\`);
  }

  lines.push('    ;;');
  lines.push('    args)');
  lines.push('      case $words[1] in');

  for (const cmd of COMMANDS) {
    const names = [cmd.name, ...cmd.aliases];
    const conditions = names.join('|');
    lines.push(`        ${conditions})`);

    if (cmd.options.length > 0) {
      lines.push('          _arguments \\');
      for (const opt of cmd.options) {
        const action = opt.type === 'boolean' ? '' : ':value:';
        if (opt.short && opt.long) {
          lines.push(`            "${opt.short}[${opt.description}]${action}" \\`);
          lines.push(`            "${opt.long}[${opt.description}]${action}" \\`);
        } else if (opt.long) {
          lines.push(`            "${opt.long}[${opt.description}]${action}" \\`);
        }
      }
      lines.push('            "--help[Show help]" \\');
      lines.push('            "-h[Show help]"');
    }

    lines.push('        ;;');
  }

  lines.push('      esac');
  lines.push('    ;;');
  lines.push('  esac');
  lines.push('}');
  lines.push('');
  lines.push('_xergon "$@"');
  lines.push('');

  return lines.join('\n');
}

// ── Fish completion ────────────────────────────────────────────

function generateFish(): string {
  const lines: string[] = [];

  lines.push('# Xergon CLI fish completion');
  lines.push('# Install: xergon completion fish > ~/.config/fish/completions/xergon.fish');
  lines.push('');

  // Disable file completions by default
  lines.push('complete -c xergon -f');
  lines.push('');

  // Global options
  lines.push('# Global options');
  lines.push("complete -c xergon -n '__fish_use_subcommand' -l json -s j -d 'Output in JSON format'");
  lines.push("complete -c xergon -n '__fish_use_subcommand' -l config -d 'Path to config file' -r");
  lines.push("complete -c xergon -n '__fish_use_subcommand' -l help -s h -d 'Show help'");
  lines.push('');

  // Commands
  for (const cmd of COMMANDS) {
    lines.push(`# Command: ${cmd.name}`);

    // Description
    lines.push(`complete -c xergon -f -n '__fish_use_subcommand' -a ${cmd.name} -d '${cmd.desc}'`);

    // Aliases
    for (const alias of cmd.aliases) {
      lines.push(`complete -c xergon -f -n '__fish_use_subcommand' -a ${alias} -d '${cmd.desc} (alias)'`);
    }

    // Command-specific options
    const cmdNames = [cmd.name, ...cmd.aliases].join(',');
    for (const opt of cmd.options) {
      const condition = `__fish_seen_subcommand_from ${cmdNames}`;
      if (opt.short && opt.long) {
        const shortDesc = opt.short.replace('-', '');
        lines.push(`complete -c xergon -f -n '${condition}' -l ${opt.long.replace('--', '')} -s ${shortDesc} -d '${opt.description}'`);
      } else if (opt.long) {
        lines.push(`complete -c xergon -f -n '${condition}' -l ${opt.long.replace('--', '')} -d '${opt.description}'`);
      }
    }
    lines.push('');
  }

  return lines.join('\n');
}

// ── Public API ─────────────────────────────────────────────────

export function generateCompletionScript(shell: 'bash' | 'zsh' | 'fish'): string {
  switch (shell) {
    case 'bash':
      return generateBash();
    case 'zsh':
      return generateZsh();
    case 'fish':
      return generateFish();
    default:
      throw new Error(`Unsupported shell: ${shell}. Supported: bash, zsh, fish`);
  }
}
