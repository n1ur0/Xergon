/**
 * CLI command: docs
 *
 * Generate, serve, and view documentation.
 *
 * Usage:
 *   xergon docs generate --format markdown --output ./docs
 *   xergon docs serve --port 8080
 *   xergon docs cheatsheet
 *   xergon docs api
 *   xergon docs config
 */

import type { Command, CommandOption, ParsedArgs, CLIContext } from '../mod';
import {
  generateCLIDocs,
  generateAPIDocs,
  generateConfigDocs,
  generatePluginDocs,
  generateQuickStart,
  generateCheatsheet,
  serveDocs,
  type DocConfig,
} from '../../docs-generator';
import * as fs from 'node:fs';
import * as path from 'node:path';

// ── Options ────────────────────────────────────────────────────────

const docsOptions: CommandOption[] = [
  {
    name: 'format',
    short: '',
    long: '--format',
    description: 'Output format (markdown, html, openapi, manpage)',
    required: false,
    default: 'markdown',
    type: 'string',
  },
  {
    name: 'output',
    short: '-o',
    long: '--output',
    description: 'Output directory for generated docs',
    required: false,
    default: './docs',
    type: 'string',
  },
  {
    name: 'port',
    short: '-p',
    long: '--port',
    description: 'Port for local docs server',
    required: false,
    default: '8080',
    type: 'number',
  },
  {
    name: 'sections',
    short: '',
    long: '--sections',
    description: 'Comma-separated list of sections to include',
    required: false,
    type: 'string',
  },
  {
    name: 'internals',
    short: '',
    long: '--internals',
    description: 'Include internal documentation',
    required: false,
    type: 'boolean',
  },
];

// ── Subcommand Handlers ───────────────────────────────────────────

async function handleGenerate(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const format = String(args.options.format ?? 'markdown') as DocConfig['format'];
  const outputDir = String(args.options.output ?? './docs');
  const sectionsStr = args.options.sections as string | undefined;
  const sections = sectionsStr ? sectionsStr.split(',').map(s => s.trim()) : [];
  const includeInternals = args.options.internals === true;

  const docConfig: DocConfig = {
    format,
    output: outputDir,
    sections,
    includeInternals,
    branding: {
      name: 'Xergon SDK',
      version: '0.1.0',
      url: 'https://xergon.gg',
    },
  };

  // Ensure output directory exists
  if (!fs.existsSync(outputDir)) {
    fs.mkdirSync(outputDir, { recursive: true });
  }

  try {
    // Generate CLI docs
    const cliDocs = generateCLIDocs(docConfig);
    const cliExt = format === 'openapi' ? 'json' : format === 'manpage' ? '1' : format === 'html' ? 'html' : 'md';
    fs.writeFileSync(path.join(outputDir, `cli.${cliExt}`), cliDocs);

    // Generate other docs
    const quickStart = generateQuickStart();
    fs.writeFileSync(path.join(outputDir, 'quickstart.md'), quickStart);

    const cheatsheet = generateCheatsheet();
    fs.writeFileSync(path.join(outputDir, 'cheatsheet.md'), cheatsheet);

    const configDocs = generateConfigDocs();
    fs.writeFileSync(path.join(outputDir, 'configuration.md'), configDocs);

    const pluginDocs = generatePluginDocs();
    fs.writeFileSync(path.join(outputDir, 'plugins.md'), pluginDocs);

    // Generate API docs in both markdown and OpenAPI
    const apiMarkdown = generateAPIDocs('markdown');
    fs.writeFileSync(path.join(outputDir, 'api-reference.md'), apiMarkdown);

    const apiOpenAPI = generateAPIDocs('openapi');
    fs.writeFileSync(path.join(outputDir, 'openapi.json'), apiOpenAPI);

    // Generate index
    const index = `# Xergon SDK Documentation\n\n- [CLI Reference](cli.${cliExt})\n- [Quick Start](quickstart.md)\n- [API Reference](api-reference.md)\n- [Configuration](configuration.md)\n- [Plugins](plugins.md)\n- [CLI Cheatsheet](cheatsheet.md)\n- [OpenAPI Spec](openapi.json)\n`;
    fs.writeFileSync(path.join(outputDir, 'README.md'), index);

    ctx.output.success(`Documentation generated in ${outputDir}/`);
    ctx.output.info(`Files: cli.${cliExt}, quickstart.md, api-reference.md, configuration.md, plugins.md, cheatsheet.md, openapi.json`);
  } catch (err) {
    ctx.output.writeError(`Failed to generate docs: ${err instanceof Error ? err.message : String(err)}`);
    process.exit(1);
  }
}

async function handleServe(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const port = Number(args.options.port ?? 8080);

  ctx.output.info(`Starting documentation server on port ${port}...`);

  const { url, close } = serveDocs(port);

  ctx.output.success(`Documentation server running at ${url}`);
  ctx.output.info('Available pages:');
  ctx.output.info(`  ${url}          - Home`);
  ctx.output.info(`  ${url}/quickstart - Quick Start Guide`);
  ctx.output.info(`  ${url}/api       - API Reference`);
  ctx.output.info(`  ${url}/config    - Configuration`);
  ctx.output.info(`  ${url}/cheatsheet - CLI Cheatsheet`);
  ctx.output.info('');
  ctx.output.info('Press Ctrl+C to stop.');

  // Handle graceful shutdown
  const cleanup = () => {
    close();
    ctx.output.info('Documentation server stopped.');
    process.exit(0);
  };
  process.on('SIGINT', cleanup);
  process.on('SIGTERM', cleanup);

  // Keep the process alive
  await new Promise(() => {});
}

async function handleCheatsheet(_args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const cheatsheet = generateCheatsheet();
  ctx.output.write(cheatsheet);
}

async function handleApi(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const format = String(args.options.format ?? 'markdown') as 'markdown' | 'openapi';
  const apiDocs = generateAPIDocs(format);
  ctx.output.write(apiDocs);
}

async function handleConfig(_args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const configDocs = generateConfigDocs();
  ctx.output.write(configDocs);
}

// ── Command Definition ─────────────────────────────────────────────

async function docsAction(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const sub = args.positional[0];

  switch (sub) {
    case 'generate':
    case 'gen':
      await handleGenerate(args, ctx);
      break;
    case 'serve':
      await handleServe(args, ctx);
      break;
    case 'cheatsheet':
    case 'cheat':
      await handleCheatsheet(args, ctx);
      break;
    case 'api':
      await handleApi(args, ctx);
      break;
    case 'config':
    case 'cfg':
      await handleConfig(args, ctx);
      break;
    default:
      ctx.output.writeError(`Unknown subcommand: ${sub ?? '(none)'}`);
      ctx.output.info('Available subcommands: generate, serve, cheatsheet, api, config');
      process.exit(1);
      break;
  }
}

export const docsCommand: Command = {
  name: 'docs',
  description: 'Generate and serve documentation',
  aliases: ['doc', 'documentation'],
  options: docsOptions,
  action: docsAction,
};
