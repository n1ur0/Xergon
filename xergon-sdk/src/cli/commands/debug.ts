/**
 * CLI command: debug
 *
 * Comprehensive diagnostic and troubleshooting tools.
 *
 * Usage:
 *   xergon debug              -- run all diagnostics
 *   xergon debug connection   -- test connections
 *   xergon debug models       -- check model availability
 *   xergon debug wallet       -- verify wallet
 *   xergon debug disk         -- check disk space
 *   xergon debug network      -- measure latency
 *   xergon debug dump         -- full debug dump
 *   xergon debug troubleshoot -- guided troubleshooting wizard
 *   xergon debug system       -- system info
 */

import type { Command, CommandOption, ParsedArgs, CLIContext } from '../mod';
import {
  runDiagnostics,
  runDiagnostic,
  generateDebugDump,
  troubleshoot,
  checkConnectionToEndpoint,
  measureNetworkLatency,
  getSystemInfo,
  exportDiagnostics,
  type DiagnosticResult,
  type DebugDump,
  type DiagnosticCategory,
} from '../../debug';
import * as fs from 'node:fs';

// ── Options ────────────────────────────────────────────────────────

const debugOptions: CommandOption[] = [
  {
    name: 'json',
    short: '',
    long: '--json',
    description: 'Output as JSON',
    required: false,
    type: 'boolean',
  },
  {
    name: 'endpoint',
    short: '',
    long: '--endpoint',
    description: 'Custom endpoint to test connectivity',
    required: false,
    type: 'string',
  },
  {
    name: 'model',
    short: '',
    long: '--model',
    description: 'Specific model to check availability',
    required: false,
    type: 'string',
  },
  {
    name: 'output',
    short: '-o',
    long: '--output',
    description: 'Save debug dump to file',
    required: false,
    type: 'string',
  },
  {
    name: 'issue',
    short: '',
    long: '--issue',
    description: 'Describe the issue for guided troubleshooting',
    required: false,
    type: 'string',
  },
];

// ── Helpers ────────────────────────────────────────────────────────

function renderDiagnostics(results: DiagnosticResult[], output: any): string {
  const lines: string[] = [];

  lines.push(output.colorize('Diagnostic Results', 'bold'));
  lines.push(output.colorize('\u2500'.repeat(60), 'dim'));
  lines.push('');

  const statusIcons: Record<string, string> = {
    healthy: output.colorize('  OK  ', 'green'),
    degraded: output.colorize(' WARN ', 'yellow'),
    unhealthy: output.colorize(' ERR  ', 'red'),
    unknown: output.colorize('  ?  ', 'dim'),
  };

  let ok = 0, warn = 0, err = 0;
  const maxName = Math.max(...results.map(r => `${r.category}/${r.name}`.length));

  for (const r of results) {
    const name = `${r.category}/${r.name}`.padEnd(maxName + 2);
    const icon = statusIcons[r.status] ?? statusIcons.unknown;
    const msg = r.status === 'healthy' ? r.message : output.colorize(r.message, r.status === 'unhealthy' ? 'red' : 'yellow');
    lines.push(`  ${name} ${icon}  ${msg}`);

    if (r.status === 'healthy') ok++;
    else if (r.status === 'degraded') warn++;
    else if (r.status === 'unhealthy') err++;
  }

  lines.push('');
  lines.push(output.colorize('\u2500'.repeat(60), 'dim'));

  const parts: string[] = [];
  if (ok > 0) parts.push(output.colorize(`${ok} OK`, 'green'));
  if (warn > 0) parts.push(output.colorize(`${warn} WARN`, 'yellow'));
  if (err > 0) parts.push(output.colorize(`${err} ERR`, 'red'));
  lines.push(`  Summary: ${parts.join('  |  ')}`);
  lines.push('');

  return lines.join('\n');
}

function renderSystemInfo(info: Record<string, any>, output: any): string {
  const lines: string[] = [];
  lines.push(output.colorize('System Information', 'bold'));
  lines.push(output.colorize('\u2500'.repeat(60), 'dim'));
  lines.push('');

  const displayFields: Array<[string, string]> = [
    ['Platform', `${info.platform} ${info.arch} (${info.release})`],
    ['Node.js', info.nodeVersion],
    ['CPU', `${info.cpuModel} (${info.cpuCount} cores, ${info.cpuSpeed})`],
    ['Memory', `${info.usedMemory} used / ${info.totalMemory} total (${info.freeMemory} free)`],
    ['Uptime', info.uptime],
    ['Hostname', info.hostname],
    ['Home', info.homeDir],
    ['Temp Dir', info.tmpDir],
    ['PID', String(info.pid)],
  ];

  const maxLabel = Math.max(...displayFields.map(([l]) => l.length));
  for (const [label, value] of displayFields) {
    const labelStr = output.colorize(`${label}:`.padEnd(maxLabel + 2), 'cyan');
    lines.push(`  ${labelStr} ${value}`);
  }

  if (info.interfaces && info.interfaces.length > 0) {
    lines.push(`  ${output.colorize('Network Interfaces:'.padEnd(maxLabel + 2), 'cyan')} ${info.interfaces.join(', ')}`);
  }

  lines.push('');
  return lines.join('\n');
}

// ── Subcommand Handlers ───────────────────────────────────────────

async function handleAllDiagnostics(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const outputJson = args.options.json === true;
  const baseUrl = ctx.config.baseUrl;
  const apiKey = ctx.config.apiKey;

  const results = await runDiagnostics({ baseUrl, apiKey });

  if (outputJson) {
    ctx.output.setFormat('json');
    ctx.output.write(ctx.output.formatOutput(results));
  } else {
    ctx.output.write(renderDiagnostics(results, ctx.output));
  }
}

async function handleConnection(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const baseUrl = ctx.config.baseUrl;
  const endpoint = args.options.endpoint as string | undefined;

  let results: DiagnosticResult[];

  if (endpoint) {
    const r = await checkConnectionToEndpoint(endpoint);
    results = [r];
  } else {
    results = await runDiagnostic('connection', { baseUrl, apiKey: ctx.config.apiKey });
  }

  if (args.options.json === true) {
    ctx.output.setFormat('json');
    ctx.output.write(ctx.output.formatOutput(results));
  } else {
    ctx.output.write(renderDiagnostics(results, ctx.output));
  }
}

async function handleModels(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const baseUrl = ctx.config.baseUrl;
  const model = args.options.model as string | undefined;

  const results = await runDiagnostic('models', { baseUrl, model });

  if (args.options.json === true) {
    ctx.output.setFormat('json');
    ctx.output.write(ctx.output.formatOutput(results));
  } else {
    ctx.output.write(renderDiagnostics(results, ctx.output));
  }
}

async function handleWallet(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const results = await runDiagnostic('wallet', {
    baseUrl: ctx.config.baseUrl,
    apiKey: ctx.config.apiKey,
  });

  if (args.options.json === true) {
    ctx.output.setFormat('json');
    ctx.output.write(ctx.output.formatOutput(results));
  } else {
    ctx.output.write(renderDiagnostics(results, ctx.output));
  }
}

async function handleDisk(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const results = await runDiagnostic('disk');

  if (args.options.json === true) {
    ctx.output.setFormat('json');
    ctx.output.write(ctx.output.formatOutput(results));
  } else {
    ctx.output.write(renderDiagnostics(results, ctx.output));
  }
}

async function handleNetwork(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const baseUrl = ctx.config.baseUrl;
  const result = await measureNetworkLatency(baseUrl);

  if (args.options.json === true) {
    ctx.output.setFormat('json');
    ctx.output.write(ctx.output.formatOutput([result]));
  } else {
    ctx.output.write(renderDiagnostics([result], ctx.output));
  }
}

async function handleDump(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const dump = await generateDebugDump({
    baseUrl: ctx.config.baseUrl,
    apiKey: ctx.config.apiKey,
  });

  const dumpStr = JSON.stringify(dump, null, 2);

  // Save to file if requested
  const outputPath = args.options.output as string | undefined;
  if (outputPath) {
    try {
      fs.writeFileSync(outputPath, dumpStr + '\n');
      ctx.output.success(`Debug dump saved to ${outputPath}`);
    } catch (err) {
      ctx.output.writeError(`Failed to write dump: ${err instanceof Error ? err.message : String(err)}`);
      process.exit(1);
      return;
    }
  }

  // Always output the dump
  ctx.output.setFormat('json');
  ctx.output.write(dumpStr);
}

async function handleTroubleshoot(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const issue = args.options.issue as string | undefined;

  if (!issue) {
    // Interactive: prompt for issue description
    const readline = await import('node:readline');
    const rl = readline.createInterface({ input: process.stdin, output: process.stdout });

    const answer = await new Promise<string>((resolve) => {
      rl.question(ctx.output.colorize('Describe the issue you are experiencing: ', 'yellow'), (ans: string) => {
        rl.close();
        resolve(ans.trim());
      });
    });

    if (!answer) {
      ctx.output.info('No issue provided. Exiting troubleshooter.');
      return;
    }

    const steps = troubleshoot(answer);
    ctx.output.write(steps.join('\n'));
    return;
  }

  const steps = troubleshoot(issue);
  ctx.output.write(steps.join('\n'));
}

async function handleSystem(_args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const info = getSystemInfo();

  if (_args.options.json === true) {
    ctx.output.setFormat('json');
    ctx.output.write(ctx.output.formatOutput(info));
  } else {
    ctx.output.write(renderSystemInfo(info, ctx.output));
  }
}

// ── Command Definition ─────────────────────────────────────────────

async function debugAction(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const sub = args.positional[0];

  switch (sub) {
    case 'connection':
    case 'conn':
      await handleConnection(args, ctx);
      break;
    case 'models':
      await handleModels(args, ctx);
      break;
    case 'wallet':
      await handleWallet(args, ctx);
      break;
    case 'disk':
      await handleDisk(args, ctx);
      break;
    case 'network':
    case 'net':
      await handleNetwork(args, ctx);
      break;
    case 'dump':
      await handleDump(args, ctx);
      break;
    case 'troubleshoot':
    case 'trouble':
      await handleTroubleshoot(args, ctx);
      break;
    case 'system':
    case 'sys':
    case 'info':
      await handleSystem(args, ctx);
      break;
    default:
      // No subcommand: run all diagnostics
      await handleAllDiagnostics(args, ctx);
      break;
  }
}

export const debugCommand: Command = {
  name: 'debug',
  description: 'Run diagnostics, troubleshoot issues, and generate debug dumps',
  aliases: ['diagnostics'],
  options: debugOptions,
  action: debugAction,
};
