/**
 * Xergon CLI -- command-line interface for the Xergon Network SDK.
 *
 * Provides a framework for parsing arguments, formatting output,
 * and dispatching to command handlers.
 */

// ── Types ───────────────────────────────────────────────────────────

declare const process: {
  env: Record<string, string | undefined>;
  stdout: { write(data: string): void };
  stderr: { write(data: string): void };
  exit(code?: number): never;
};

export interface CommandOption {
  name: string;
  short: string;        // e.g., '-m'
  long: string;         // e.g., '--model'
  description: string;
  required: boolean;
  default?: string;
  type: 'string' | 'number' | 'boolean';
}

export interface Command {
  name: string;
  description: string;
  aliases: string[];
  options: CommandOption[];
  action: (args: ParsedArgs, ctx: CLIContext) => void | Promise<void>;
  subcommands?: any[];
}

export interface ParsedArgs {
  command: string;
  positional: string[];
  options: Record<string, string | number | boolean>;
}

export interface CLIConfig {
  baseUrl: string;
  apiKey: string;
  defaultModel: string;
  outputFormat: 'text' | 'json' | 'table';
  color: boolean;
  timeout: number;
}

export interface CLIContext {
  client: any;  // XergonClient -- using any to avoid circular dep
  config: CLIConfig;
  output: OutputFormatter;
}

// ── Argument Parser ────────────────────────────────────────────────

export class ArgumentParser {
  private commands: Map<string, Command> = new Map();
  private globalOptions: CommandOption[] = [];

  constructor(private programName: string = 'xergon', private version: string = '0.1.0') {}

  registerCommand(cmd: Command): void {
    this.commands.set(cmd.name, cmd);
    for (const alias of cmd.aliases) {
      this.commands.set(alias, cmd);
    }
  }

  addGlobalOption(opt: CommandOption): void {
    this.globalOptions.push(opt);
  }

  getCommand(name: string): Command | undefined {
    return this.commands.get(name);
  }

  getAllCommands(): Command[] {
    const seen = new Set<string>();
    const result: Command[] = [];
    for (const [, cmd] of this.commands) {
      if (!seen.has(cmd.name)) {
        seen.add(cmd.name);
        result.push(cmd);
      }
    }
    return result;
  }

  /**
   * Parse command-line arguments into a structured ParsedArgs object.
   */
  parse(argv: string[]): ParsedArgs {
    const args = argv.slice(2); // strip node and script paths

    // Check for --version / -v before command
    if (args.includes('--version') || args.includes('-v')) {
      return {
        command: 'version',
        positional: [],
        options: {},
      };
    }

    // Check for --help / -h before command
    if (args.length === 0 || args.includes('--help') || args.includes('-h')) {
      return {
        command: 'help',
        positional: [],
        options: {},
      };
    }

    // First non-flag arg is the command name
    const commandName = args.shift()!;
    const cmd = this.commands.get(commandName);

    if (!cmd) {
      return {
        command: 'unknown',
        positional: [commandName, ...args],
        options: {},
      };
    }

    const positional: string[] = [];
    const options: Record<string, string | number | boolean> = {};

    // Build lookup for known options
    const shortLookup = new Map<string, CommandOption>();
    const longLookup = new Map<string, CommandOption>();
    for (const opt of [...this.globalOptions, ...cmd.options]) {
      if (opt.short) shortLookup.set(opt.short, opt);
      if (opt.long) longLookup.set(opt.long, opt);
    }

    // Apply defaults
    for (const opt of cmd.options) {
      if (opt.default !== undefined && !opt.required) {
        options[opt.name] = opt.type === 'number'
          ? Number(opt.default)
          : opt.type === 'boolean'
            ? opt.default === 'true'
            : opt.default;
      }
    }

    let i = 0;
    while (i < args.length) {
      const arg = args[i];

      if (arg.startsWith('--')) {
        // Long option
        const equalIdx = arg.indexOf('=');
        let optName: string;
        let optValue: string | undefined;

        if (equalIdx !== -1) {
          optName = arg.substring(0, equalIdx);
          optValue = arg.substring(equalIdx + 1);
        } else {
          optName = arg;
        }

        const optDef = longLookup.get(optName);
        if (!optDef) {
          throw new CLIError(`Unknown option: ${optName}`);
        }

        if (optDef.type === 'boolean') {
          options[optDef.name] = optValue !== 'false';
          i++;
        } else if (optValue !== undefined) {
          options[optDef.name] = optDef.type === 'number'
            ? Number(optValue)
            : optValue;
          i++;
        } else {
          // Next arg is the value
          i++;
          if (i >= args.length || args[i].startsWith('-')) {
            throw new CLIError(`Option ${optName} requires a value`);
          }
          options[optDef.name] = optDef.type === 'number'
            ? Number(args[i])
            : args[i];
          i++;
        }
      } else if (arg.startsWith('-') && arg.length >= 2) {
        // Short option(s) - support combining: -mf val
        const shortOpt = arg.substring(0, 2);
        const optDef = shortLookup.get(shortOpt);

        if (!optDef) {
          throw new CLIError(`Unknown option: ${shortOpt}`);
        }

        if (optDef.type === 'boolean') {
          options[optDef.name] = true;
          // Handle combined boolean flags: -abc
          if (arg.length > 2) {
            for (const ch of arg.substring(2)) {
              const subOpt = shortLookup.get(`-${ch}`);
              if (subOpt && subOpt.type === 'boolean') {
                options[subOpt.name] = true;
              }
            }
          }
          i++;
        } else {
          // Value may be attached: -mval or next arg
          const attachedValue = arg.length > 2 ? arg.substring(2) : undefined;
          if (attachedValue) {
            options[optDef.name] = optDef.type === 'number'
              ? Number(attachedValue)
              : attachedValue;
            i++;
          } else {
            i++;
            if (i >= args.length || args[i].startsWith('-')) {
              throw new CLIError(`Option ${shortOpt} requires a value`);
            }
            options[optDef.name] = optDef.type === 'number'
              ? Number(args[i])
              : args[i];
            i++;
          }
        }
      } else {
        positional.push(arg);
        i++;
      }
    }

    // Validate required options
    for (const opt of cmd.options) {
      if (opt.required && options[opt.name] === undefined) {
        throw new CLIError(`Missing required option: ${opt.long || opt.short}`);
      }
    }

    return {
      command: commandName,
      positional,
      options,
    };
  }

  /**
   * Generate help text for a specific command or the entire program.
   */
  generateHelp(commandName?: string): string {
    if (commandName) {
      const cmd = this.commands.get(commandName);
      if (!cmd) return `Unknown command: ${commandName}\n`;
      return formatCommandHelp(cmd, this.programName);
    }
    return formatProgramHelp(
      this.programName,
      this.version,
      this.getAllCommands(),
      this.globalOptions,
    );
  }
}

// ── Output Formatter ───────────────────────────────────────────────

export class OutputFormatter {
  private useColor: boolean;
  private format: 'text' | 'json' | 'table';

  constructor(format: 'text' | 'json' | 'table' = 'text', color: boolean = true) {
    this.format = format;
    this.useColor = color && !process.env.NO_COLOR;
  }

  setFormat(format: 'text' | 'json' | 'table'): void {
    this.format = format;
  }

  setColor(color: boolean): void {
    this.useColor = color && !process.env.NO_COLOR;
  }

  /**
   * Format data for output based on the current format setting.
   */
  formatOutput(data: unknown, title?: string): string {
    if (this.format === 'json') {
      return JSON.stringify(data, null, 2);
    }
    if (this.format === 'table' && Array.isArray(data) && data.length > 0) {
      return this.formatTable(data, title);
    }
    return this.formatText(data, title);
  }

  /**
   * Human-readable text output.
   */
  formatText(data: unknown, title?: string): string {
    const lines: string[] = [];

    if (title) {
      lines.push(this.colorize(title, 'bold'));
      lines.push('─'.repeat(title.length));
    }

    if (typeof data === 'string') {
      lines.push(data);
    } else if (Array.isArray(data)) {
      for (const item of data) {
        lines.push(this.formatItem(item));
        lines.push('');
      }
    } else if (data !== null && typeof data === 'object') {
      const obj = data as Record<string, unknown>;
      for (const [key, value] of Object.entries(obj)) {
        const label = this.colorize(`${this.formatLabel(key)}:`, 'cyan');
        lines.push(`  ${label} ${this.formatValue(value)}`);
      }
    } else {
      lines.push(String(data));
    }

    return lines.join('\n');
  }

  /**
   * JSON output (pretty-printed).
   */
  formatJSON(data: unknown): string {
    return JSON.stringify(data, null, 2);
  }

  /**
   * Table output with aligned columns.
   */
  formatTable(data: Record<string, unknown>[], title?: string): string {
    if (data.length === 0) return title ? `${title}\nNo data available.\n` : 'No data available.';

    const headers = Object.keys(data[0]);
    const rows = data.map(row => headers.map(h => String(row[h] ?? '')));

    // Calculate column widths
    const widths = headers.map((h, i) => {
      const maxDataWidth = Math.max(...rows.map(r => r[i].length));
      return Math.max(h.length, maxDataWidth);
    });

    const lines: string[] = [];

    if (title) {
      lines.push(this.colorize(title, 'bold'));
      lines.push('');
    }

    // Header row
    const headerLine = '  ' + headers.map((h, i) =>
      this.colorize(h.padEnd(widths[i]), 'bold')
    ).join('  ');
    lines.push(headerLine);

    // Separator
    const separator = '  ' + widths.map(w => '─'.repeat(w)).join('  ');
    lines.push(separator);

    // Data rows
    for (const row of rows) {
      lines.push('  ' + row.map((cell, i) => cell.padEnd(widths[i])).join('  '));
    }

    lines.push('');
    lines.push(this.colorize(`  ${data.length} item(s)`, 'dim'));

    return lines.join('\n');
  }

  /**
   * Colorize a string using ANSI escape codes.
   */
  colorize(text: string, style: 'bold' | 'dim' | 'cyan' | 'green' | 'red' | 'yellow' | 'blue'): string {
    if (!this.useColor) return text;
    const codes: Record<string, string> = {
      bold: '\x1b[1m',
      dim: '\x1b[2m',
      cyan: '\x1b[36m',
      green: '\x1b[32m',
      red: '\x1b[31m',
      yellow: '\x1b[33m',
      blue: '\x1b[34m',
    };
    const reset = '\x1b[0m';
    return `${codes[style]}${text}${reset}`;
  }

  /**
   * Write to stdout.
   */
  write(data: string): void {
    process.stdout.write(data);
  }

  /**
   * Write to stderr.
   */
  writeError(message: string): void {
    process.stderr.write(this.colorize('Error: ', 'red') + message + '\n');
  }

  /**
   * Write a success message.
   */
  success(message: string): void {
    process.stdout.write(this.colorize('✓ ', 'green') + message + '\n');
  }

  /**
   * Write an info message.
   */
  info(message: string): void {
    process.stdout.write(this.colorize('ℹ ', 'cyan') + message + '\n');
  }

  /**
   * Write a warning message.
   */
  warn(message: string): void {
    process.stderr.write(this.colorize('⚠ ', 'yellow') + message + '\n');
  }

  private formatLabel(key: string): string {
    return key
      .replace(/([A-Z])/g, ' $1')
      .replace(/_/g, ' ')
      .replace(/^\s/, '')
      .split(' ')
      .map(w => w.charAt(0).toUpperCase() + w.slice(1))
      .join(' ');
  }

  private formatValue(value: unknown): string {
    if (value === null || value === undefined) return this.colorize('null', 'dim');
    if (typeof value === 'boolean') return this.colorize(String(value), value ? 'green' : 'red');
    if (typeof value === 'number') return this.colorize(String(value), 'yellow');
    if (Array.isArray(value)) return value.map(v => String(v)).join(', ');
    if (typeof value === 'object') return JSON.stringify(value);
    return String(value);
  }

  private formatItem(item: unknown): string {
    if (typeof item === 'string') return item;
    if (item !== null && typeof item === 'object') {
      return this.formatText(item);
    }
    return String(item);
  }
}

// ── CLI Error ──────────────────────────────────────────────────────

export class CLIError extends Error {
  constructor(message: string, public readonly exitCode: number = 1) {
    super(message);
    this.name = 'CLIError';
  }
}

// ── Help Formatters ────────────────────────────────────────────────

function formatProgramHelp(
  programName: string,
  version: string,
  commands: Command[],
  globalOptions: CommandOption[],
): string {
  const lines: string[] = [];
  lines.push(`Xergon CLI v${version}`);
  lines.push('Decentralized AI inference on the Xergon Network');
  lines.push('');
  lines.push('USAGE:');
  lines.push(`  ${programName} <command> [options] [arguments]`);
  lines.push('');
  lines.push('COMMANDS:');

  const maxNameLen = Math.max(...commands.map(c => c.name.length));

  for (const cmd of commands) {
    const padded = cmd.name.padEnd(maxNameLen + 2);
    lines.push(`  ${padded} ${cmd.description}`);
    if (cmd.aliases.length > 0) {
      lines.push(`  ${' '.repeat(maxNameLen + 4)}Aliases: ${cmd.aliases.join(', ')}`);
    }
  }

  if (globalOptions.length > 0) {
    lines.push('');
    lines.push('GLOBAL OPTIONS:');
    for (const opt of globalOptions) {
      const optStr = [opt.short, opt.long].filter(Boolean).join(', ');
      lines.push(`  ${optStr.padEnd(20)} ${opt.description}${opt.default ? ` (default: ${opt.default})` : ''}`);
    }
  }

  lines.push('');
  lines.push('Run `xergon <command> --help` for command-specific help.');
  lines.push('');

  return lines.join('\n');
}

function formatCommandHelp(cmd: Command, programName: string): string {
  const lines: string[] = [];
  lines.push(`COMMAND: ${cmd.name}`);
  lines.push(`  ${cmd.description}`);
  lines.push('');
  lines.push('USAGE:');
  lines.push(`  ${programName} ${cmd.name} [options]`);

  if (cmd.options.length > 0) {
    lines.push('');
    lines.push('OPTIONS:');

    for (const opt of cmd.options) {
      const optStr = [opt.short, opt.long].filter(Boolean).join(', ');
      const req = opt.required ? ' (required)' : '';
      const def = opt.default ? ` (default: ${opt.default})` : '';
      lines.push(`  ${optStr.padEnd(25)} ${opt.description}${req}${def}`);
    }
  }

  lines.push('');
  return lines.join('\n');
}
