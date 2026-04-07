/**
 * CLI command: log
 *
 * Manage enhanced logging: set level, show logs, export, clear history.
 */

import type { Command, CommandOption, ParsedArgs, CLIContext } from '../mod';
import {
  setLevel,
  getLevel,
  getHistory,
  exportLogs,
  clearHistory,
  LogLevel,
} from '../../log';
import * as fs from 'node:fs';

const logOptions: CommandOption[] = [
  {
    name: 'tail',
    short: '',
    long: '--tail',
    description: 'Number of recent log entries to show (default: 50)',
    required: false,
    type: 'number',
  },
  {
    name: 'level',
    short: '',
    long: '--level',
    description: 'Filter by log level: debug, info, warn, error',
    required: false,
    type: 'string',
  },
  {
    name: 'format',
    short: '',
    long: '--format',
    description: 'Export format: json or text (default: text)',
    required: false,
    type: 'string',
  },
  {
    name: 'output',
    short: '-o',
    long: '--output',
    description: 'Output file path for export',
    required: false,
    type: 'string',
  },
];

async function logAction(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const subCommand = args.positional[0];

  switch (subCommand) {
    case 'level':
      await handleLevel(args, ctx);
      break;
    case 'show':
      await handleShow(args, ctx);
      break;
    case 'export':
      await handleExport(args, ctx);
      break;
    case 'clear':
      await handleClear(ctx);
      break;
    default:
      ctx.output.writeError(`Unknown log subcommand: ${subCommand || '(none)'}`);
      ctx.output.info('Usage: xergon log <level|show|export|clear> [options]');
      process.exit(1);
  }
}

async function handleLevel(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const levelStr = args.positional[1];

  if (!levelStr) {
    const current = getLevel();
    const levelNames: Record<number, string> = {
      [LogLevel.Debug]: 'debug',
      [LogLevel.Info]: 'info',
      [LogLevel.Warn]: 'warn',
      [LogLevel.Error]: 'error',
      [LogLevel.None]: 'none',
    };
    ctx.output.info(`Current log level: ${levelNames[current]}`);
    ctx.output.info('Usage: xergon log level <debug|info|warn|error|none>');
    return;
  }

  const levelMap: Record<string, LogLevel> = {
    debug: LogLevel.Debug,
    info: LogLevel.Info,
    warn: LogLevel.Warn,
    error: LogLevel.Error,
    none: LogLevel.None,
  };

  const level = levelMap[levelStr.toLowerCase()];
  if (level === undefined) {
    ctx.output.writeError(`Invalid log level: ${levelStr}`);
    ctx.output.info('Valid levels: debug, info, warn, error, none');
    process.exit(1);
  }

  setLevel(level);
  ctx.output.success(`Log level set to: ${levelStr.toLowerCase()}`);
}

async function handleShow(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const tail = args.options.tail ? Number(args.options.tail) : 50;
  const filterLevel = args.options.level ? String(args.options.level) : undefined;

  let entries = getHistory(tail);

  // Filter by level if specified
  if (filterLevel) {
    const levelMap: Record<string, LogLevel> = {
      debug: LogLevel.Debug,
      info: LogLevel.Info,
      warn: LogLevel.Warn,
      error: LogLevel.Error,
    };
    const targetLevel = levelMap[filterLevel.toLowerCase()];
    if (targetLevel !== undefined) {
      entries = entries.filter(e => e.level === targetLevel);
    }
  }

  if (entries.length === 0) {
    ctx.output.info('No log entries found.');
    return;
  }

  const levelColors: Record<number, 'dim' | 'cyan' | 'yellow' | 'red'> = {
    [LogLevel.Debug]: 'dim',
    [LogLevel.Info]: 'cyan',
    [LogLevel.Warn]: 'yellow',
    [LogLevel.Error]: 'red',
  };

  const levelNames: Record<number, string> = {
    [LogLevel.Debug]: 'DEBUG',
    [LogLevel.Info]: 'INFO',
    [LogLevel.Warn]: 'WARN',
    [LogLevel.Error]: 'ERROR',
  };

  ctx.output.write(ctx.output.colorize(`Recent Logs (${entries.length} entries):\n`, 'bold'));
  ctx.output.write(ctx.output.colorize('─'.repeat(80) + '\n', 'dim'));

  for (const entry of entries) {
    const color = levelColors[entry.level] || 'dim';
    const name = levelNames[entry.level] || '???';
    const ctx_str = entry.context ? ` [${entry.context}]` : '';
    const line = `${entry.timestamp}  ${ctx.output.colorize(name.padEnd(5), color)}${ctx_str}  ${entry.message}`;
    ctx.output.write(line + '\n');
  }

  ctx.output.write(ctx.output.colorize('─'.repeat(80) + '\n', 'dim'));
}

async function handleExport(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const formatStr = args.options.format ? String(args.options.format) : 'text';
  const outputFile = args.options.output ? String(args.options.output) : undefined;

  if (formatStr !== 'json' && formatStr !== 'text') {
    ctx.output.writeError('Invalid format. Use: json or text');
    process.exit(1);
  }

  const content = exportLogs(formatStr as 'json' | 'text');

  if (outputFile) {
    try {
      fs.writeFileSync(outputFile, content, 'utf-8');
      ctx.output.success(`Logs exported to: ${outputFile}`);
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      ctx.output.writeError(`Failed to write file: ${message}`);
      process.exit(1);
    }
  } else {
    process.stdout.write(content);
    if (!content.endsWith('\n')) process.stdout.write('\n');
  }
}

async function handleClear(ctx: CLIContext): Promise<void> {
  clearHistory();
  ctx.output.success('Log history cleared.');
}

export const logCommand: Command = {
  name: 'log',
  description: 'Manage logging: level, show, export, clear',
  aliases: ['logs'],
  options: logOptions,
  action: logAction,
};
