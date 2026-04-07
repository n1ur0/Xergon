/**
 * Documentation Generator -- generates CLI docs, API references,
 * configuration docs, plugin guides, quick start guides, and
 * CLI cheatsheets from the SDK source code.
 *
 * Supports markdown, HTML, and OpenAPI formats. Can serve docs
 * locally with live reload.
 */

import * as fs from 'node:fs';
import * as path from 'node:path';
import * as http from 'node:http';

// ── Types ──────────────────────────────────────────────────────────

export interface DocConfig {
  format: 'markdown' | 'html' | 'openapi' | 'manpage';
  output: string;
  sections: string[];
  includeInternals: boolean;
  branding: {
    name: string;
    version: string;
    url?: string;
  };
}

export interface ApiEndpoint {
  method: string;
  path: string;
  description: string;
  parameters: Array<{
    name: string;
    type: string;
    required: boolean;
    description: string;
  }>;
  responses: Array<{
    code: number;
    description: string;
    schema?: any;
  }>;
  auth: boolean;
}

// ── CLI Command Registry (static snapshot) ─────────────────────────

const CLI_COMMANDS: Array<{
  name: string;
  aliases: string[];
  description: string;
  usage: string;
  options: Array<{ flag: string; desc: string }>;
  subcommands?: string[];
}> = [
  {
    name: 'chat',
    aliases: ['c'],
    description: 'Start an interactive chat session',
    usage: 'xergon chat [options]',
    options: [
      { flag: '-m, --model <model>', desc: 'Model to use' },
      { flag: '-s, --system <prompt>', desc: 'System prompt' },
      { flag: '-t, --temperature <n>', desc: 'Sampling temperature (0-2)' },
      { flag: '--stream', desc: 'Stream responses' },
    ],
  },
  {
    name: 'models',
    aliases: ['m'],
    description: 'List and manage models',
    usage: 'xergon models [subcommand] [options]',
    options: [
      { flag: '--search <term>', desc: 'Filter models by search term' },
      { flag: '-i, --interactive', desc: 'Interactive model picker' },
    ],
    subcommands: ['search', 'info', 'pull', 'remove'],
  },
  {
    name: 'model',
    aliases: ['model-registry'],
    description: 'Enhanced model registry: search, compare, recommend',
    usage: 'xergon model <subcommand> [options]',
    options: [
      { flag: '--task <type>', desc: 'Filter by task type' },
      { flag: '--provider <name>', desc: 'Filter by provider' },
      { flag: '--sort <field>', desc: 'Sort by field' },
      { flag: '--limit <n>', desc: 'Max results' },
      { flag: '--budget <amount>', desc: 'Budget for recommendations' },
    ],
    subcommands: ['list', 'info', 'search', 'versions', 'compare', 'recommend', 'popular', 'lineage', 'deprecation'],
  },
  {
    name: 'debug',
    aliases: ['diagnostics'],
    description: 'Run diagnostics and troubleshooting',
    usage: 'xergon debug [subcommand] [options]',
    options: [
      { flag: '--json', desc: 'Output as JSON' },
      { flag: '--endpoint <url>', desc: 'Test specific endpoint' },
      { flag: '--model <model>', desc: 'Check specific model' },
      { flag: '-o, --output <file>', desc: 'Save dump to file' },
      { flag: '--issue <desc>', desc: 'Describe issue for troubleshooting' },
    ],
    subcommands: ['connection', 'models', 'wallet', 'disk', 'network', 'dump', 'troubleshoot', 'system'],
  },
  {
    name: 'docs',
    aliases: [],
    description: 'Generate and serve documentation',
    usage: 'xergon docs <subcommand> [options]',
    options: [
      { flag: '--format <fmt>', desc: 'Output format (markdown, html)' },
      { flag: '--output <dir>', desc: 'Output directory' },
      { flag: '--port <n>', desc: 'Port for serve command' },
    ],
    subcommands: ['generate', 'serve', 'cheatsheet', 'api', 'config'],
  },
  {
    name: 'config',
    aliases: ['cfg'],
    description: 'View and manage configuration',
    usage: 'xergon config <subcommand>',
    options: [],
    subcommands: ['show', 'set', 'get', 'reset'],
  },
  {
    name: 'balance',
    aliases: ['bal'],
    description: 'Check ERG balance',
    usage: 'xergon balance [public-key]',
    options: [],
  },
  {
    name: 'provider',
    aliases: ['prov'],
    description: 'List and manage providers',
    usage: 'xergon provider [subcommand]',
    options: [],
    subcommands: ['list', 'info', 'leaderboard'],
  },
  {
    name: 'status',
    aliases: [],
    description: 'System health check',
    usage: 'xergon status',
    options: [{ flag: '--json', desc: 'Output as JSON' }],
  },
  {
    name: 'login',
    aliases: [],
    description: 'Authenticate with the Xergon Network',
    usage: 'xergon login',
    options: [],
  },
  {
    name: 'embed',
    aliases: [],
    description: 'Create text embeddings',
    usage: 'xergon embed <text>',
    options: [
      { flag: '-m, --model <model>', desc: 'Embedding model' },
    ],
  },
  {
    name: 'audio',
    aliases: [],
    description: 'Text-to-speech, transcription, and translation',
    usage: 'xergon audio <tts|stt|translate> <input>',
    options: [
      { flag: '-m, --model <model>', desc: 'Audio model' },
    ],
  },
  {
    name: 'upload',
    aliases: [],
    description: 'Upload files to the relay',
    usage: 'xergon upload <file>',
    options: [],
  },
  {
    name: 'bench',
    aliases: [],
    description: 'Run performance benchmarks',
    usage: 'xergon bench [options]',
    options: [
      { flag: '-m, --model <model>', desc: 'Model to benchmark' },
      { flag: '-i, --iterations <n>', desc: 'Number of iterations' },
    ],
  },
  {
    name: 'workspace',
    aliases: ['ws'],
    description: 'Manage workspaces',
    usage: 'xergon workspace <subcommand>',
    options: [],
    subcommands: ['create', 'switch', 'list', 'delete', 'set'],
  },
  {
    name: 'template',
    aliases: ['tpl'],
    description: 'Manage prompt templates',
    usage: 'xergon template <subcommand>',
    options: [],
    subcommands: ['list', 'get', 'render', 'add', 'remove'],
  },
  {
    name: 'alias',
    aliases: [],
    description: 'Manage model aliases',
    usage: 'xergon alias <subcommand>',
    options: [],
    subcommands: ['list', 'add', 'remove', 'get'],
  },
  {
    name: 'flow',
    aliases: [],
    description: 'Create and run processing pipelines',
    usage: 'xergon flow <subcommand>',
    options: [],
    subcommands: ['create', 'run', 'list'],
  },
  {
    name: 'plugin',
    aliases: [],
    description: 'Manage plugins',
    usage: 'xergon plugin <subcommand>',
    options: [],
    subcommands: ['list', 'install', 'uninstall', 'search'],
  },
  {
    name: 'deploy',
    aliases: [],
    description: 'Deploy models and endpoints',
    usage: 'xergon deploy <subcommand>',
    options: [],
    subcommands: ['create', 'list', 'logs', 'stop'],
  },
  {
    name: 'eval',
    aliases: [],
    description: 'Run evaluation benchmarks',
    usage: 'xergon eval <subcommand>',
    options: [],
    subcommands: ['run', 'list', 'compare'],
  },
  {
    name: 'canary',
    aliases: [],
    description: 'Canary deployment management',
    usage: 'xergon canary <subcommand>',
    options: [],
    subcommands: ['start', 'check', 'promote', 'rollback', 'list'],
  },
  {
    name: 'export',
    aliases: [],
    description: 'Export data and settings',
    usage: 'xergon export <scope>',
    options: [{ flag: '-f, --format <fmt>', desc: 'Export format' }],
  },
  {
    name: 'team',
    aliases: [],
    description: 'Team collaboration management',
    usage: 'xergon team <subcommand>',
    options: [],
    subcommands: ['create', 'list', 'invite', 'members'],
  },
  {
    name: 'webhook',
    aliases: [],
    description: 'Webhook management',
    usage: 'xergon webhook <subcommand>',
    options: [],
    subcommands: ['create', 'list', 'test', 'delete'],
  },
  {
    name: 'version',
    aliases: ['v'],
    description: 'Show CLI and SDK version',
    usage: 'xergon version',
    options: [],
  },
  {
    name: 'serve',
    aliases: [],
    description: 'Start a local API proxy server',
    usage: 'xergon serve [options]',
    options: [
      { flag: '-p, --port <n>', desc: 'Port to listen on' },
      { flag: '--host <addr>', desc: 'Host address' },
    ],
  },
  {
    name: 'onboard',
    aliases: [],
    description: 'Interactive first-time setup wizard',
    usage: 'xergon onboard',
    options: [],
  },
  {
    name: 'validate',
    aliases: [],
    description: 'Validate configuration and connections',
    usage: 'xergon validate',
    options: [],
  },
  {
    name: 'logs',
    aliases: [],
    description: 'View relay logs',
    usage: 'xergon logs [options]',
    options: [
      { flag: '-f, --follow', desc: 'Follow log stream' },
      { flag: '-n, --lines <n>', desc: 'Number of lines' },
    ],
  },
  {
    name: 'monitor',
    aliases: [],
    description: 'Monitor relay and provider health',
    usage: 'xergon monitor [options]',
    options: [{ flag: '--interval <ms>', desc: 'Refresh interval' }],
  },
  {
    name: 'inspect',
    aliases: [],
    description: 'Inspect request/response details',
    usage: 'xergon inspect <request-id>',
    options: [],
  },
];

// ── API Endpoints (static snapshot) ────────────────────────────────

const API_ENDPOINTS: ApiEndpoint[] = [
  {
    method: 'GET', path: '/health', description: 'Liveness probe',
    parameters: [], responses: [{ code: 200, description: 'OK' }], auth: false,
  },
  {
    method: 'GET', path: '/ready', description: 'Readiness probe',
    parameters: [], responses: [{ code: 200, description: 'OK' }], auth: false,
  },
  {
    method: 'GET', path: '/v1/models', description: 'List available models',
    parameters: [], responses: [{ code: 200, description: 'Models list', schema: { type: 'object' } }], auth: false,
  },
  {
    method: 'POST', path: '/v1/chat/completions', description: 'Create chat completion',
    parameters: [
      { name: 'model', type: 'string', required: true, description: 'Model ID' },
      { name: 'messages', type: 'array', required: true, description: 'Chat messages' },
      { name: 'max_tokens', type: 'integer', required: false, description: 'Max tokens to generate' },
      { name: 'temperature', type: 'number', required: false, description: 'Sampling temperature' },
      { name: 'stream', type: 'boolean', required: false, description: 'Enable SSE streaming' },
    ],
    responses: [
      { code: 200, description: 'Chat completion response' },
      { code: 401, description: 'Unauthorized' },
    ],
    auth: true,
  },
  {
    method: 'POST', path: '/v1/embeddings', description: 'Create embeddings',
    parameters: [
      { name: 'model', type: 'string', required: true, description: 'Embedding model' },
      { name: 'input', type: 'string|array', required: true, description: 'Input text(s)' },
    ],
    responses: [{ code: 200, description: 'Embedding vectors' }],
    auth: true,
  },
  {
    method: 'GET', path: '/v1/providers', description: 'List active providers',
    parameters: [], responses: [{ code: 200, description: 'Providers list' }], auth: false,
  },
  {
    method: 'GET', path: '/v1/balance/{publicKey}', description: 'Get ERG balance',
    parameters: [{ name: 'publicKey', type: 'string', required: true, description: 'Ergo public key' }],
    responses: [{ code: 200, description: 'Balance info' }],
    auth: false,
  },
  {
    method: 'GET', path: '/v1/auth/status', description: 'Check auth status',
    parameters: [], responses: [{ code: 200, description: 'Auth status' }], auth: true,
  },
  {
    method: 'POST', path: '/v1/audio/speech', description: 'Text-to-speech',
    parameters: [
      { name: 'model', type: 'string', required: true, description: 'TTS model' },
      { name: 'input', type: 'string', required: true, description: 'Text to speak' },
    ],
    responses: [{ code: 200, description: 'Audio data' }],
    auth: true,
  },
  {
    method: 'POST', path: '/v1/audio/transcriptions', description: 'Speech-to-text',
    parameters: [
      { name: 'model', type: 'string', required: true, description: 'STT model' },
      { name: 'file', type: 'file', required: true, description: 'Audio file' },
    ],
    responses: [{ code: 200, description: 'Transcription text' }],
    auth: true,
  },
];

// ── Configuration Reference ────────────────────────────────────────

const CONFIG_ENTRIES: Array<{ key: string; type: string; default: string; description: string; envVar: string }> = [
  { key: 'baseUrl', type: 'string', default: 'https://relay.xergon.gg', description: 'Relay base URL', envVar: 'XERGON_BASE_URL' },
  { key: 'apiKey', type: 'string', default: '(none)', description: 'Public key for authentication', envVar: 'XERGON_API_KEY' },
  { key: 'defaultModel', type: 'string', default: 'llama-3.3-70b', description: 'Default model for chat', envVar: 'XERGON_DEFAULT_MODEL' },
  { key: 'outputFormat', type: 'text|json|table', default: 'text', description: 'CLI output format', envVar: 'XERGON_OUTPUT_FORMAT' },
  { key: 'color', type: 'boolean', default: 'true', description: 'Enable colored output', envVar: 'NO_COLOR' },
  { key: 'timeout', type: 'number', default: '30000', description: 'Request timeout (ms)', envVar: 'XERGON_TIMEOUT' },
  { key: 'agentUrl', type: 'string', default: '(none)', description: 'Local agent URL', envVar: '' },
];

// ── Generator Functions ────────────────────────────────────────────

/**
 * Generate CLI documentation in the specified format.
 */
export function generateCLIDocs(config: DocConfig): string {
  const { branding } = config;

  if (config.format === 'markdown' || config.format === 'html') {
    return generateMarkdownCLIDocs(branding.name, branding.version, branding.url, config.sections, config.includeInternals);
  }

  if (config.format === 'manpage') {
    return generateManpage(branding.name, branding.version);
  }

  // openapi -- not applicable for CLI docs, return empty
  return '';
}

function generateMarkdownCLIDocs(
  name: string,
  version: string,
  url?: string,
  sections?: string[],
  includeInternals?: boolean,
): string {
  const lines: string[] = [];

  lines.push(`# ${name} CLI Documentation`);
  lines.push('');
  lines.push(`> Version ${version}${url ? ` | [${url}](${url})` : ''}`);
  lines.push('');
  lines.push('Table of Contents');
  lines.push('==================');
  lines.push('');
  lines.push('- [Installation](#installation)');
  lines.push('- [Quick Start](#quick-start)');
  lines.push('- [Commands](#commands)');
  lines.push('- [Configuration](#configuration)');
  lines.push('- [Environment Variables](#environment-variables)');
  if (includeInternals) {
    lines.push('- [Internals](#internals)');
  }
  lines.push('');

  // Installation
  lines.push('## Installation');
  lines.push('');
  lines.push('```bash');
  lines.push('npm install -g @xergon/sdk');
  lines.push('```');
  lines.push('');
  lines.push('Or clone and build:');
  lines.push('```bash');
  lines.push('git clone https://github.com/xergon-network/xergon-sdk');
  lines.push('cd xergon-sdk');
  lines.push('npm install');
  lines.push('npm run build');
  lines.push('```');
  lines.push('');

  // Quick Start
  lines.push('## Quick Start');
  lines.push('');
  lines.push('```bash');
  lines.push('# Check system status');
  lines.push(`${name} status`);
  lines.push('');
  lines.push('# Start a chat session');
  lines.push(`${name} chat`);
  lines.push('');
  lines.push('# List available models');
  lines.push(`${name} models`);
  lines.push('');
  lines.push('# Search for a specific model');
  lines.push(`${name} model search llama`);
  lines.push('');
  lines.push('# Compare two models');
  lines.push(`${name} model compare llama-3.3-70b llama-3.1-8b`);
  lines.push('');
  lines.push('# Get model recommendations');
  lines.push(`${name} model recommend --task code`);
  lines.push('');
  lines.push('# Run diagnostics');
  lines.push(`${name} debug`);
  lines.push('```');
  lines.push('');

  // Commands
  lines.push('## Commands');
  lines.push('');

  const filteredCommands = CLI_COMMANDS.filter(cmd => {
    if (sections && sections.length > 0) {
      return sections.includes(cmd.name);
    }
    return true;
  });

  for (const cmd of filteredCommands) {
    lines.push(`### \`${cmd.name}\``);
    lines.push('');
    lines.push(cmd.description);
    lines.push('');
    lines.push('```');
    lines.push(cmd.usage);
    lines.push('```');
    lines.push('');

    if (cmd.aliases.length > 0) {
      lines.push(`**Aliases:** ${cmd.aliases.map(a => `\`${a}\``).join(', ')}`);
      lines.push('');
    }

    if (cmd.subcommands && cmd.subcommands.length > 0) {
      lines.push('**Subcommands:** ' + cmd.subcommands.map(s => `\`${s}\``).join(', '));
      lines.push('');
    }

    if (cmd.options.length > 0) {
      lines.push('| Option | Description |');
      lines.push('|--------|-------------|');
      for (const opt of cmd.options) {
        lines.push(`| \`${opt.flag}\` | ${opt.desc} |`);
      }
      lines.push('');
    }
  }

  // Configuration
  lines.push('## Configuration');
  lines.push('');
  lines.push('Configuration is stored in `~/.xergon/config.json`. You can also use environment variables.');
  lines.push('');
  lines.push('| Key | Type | Default | Env Var | Description |');
  lines.push('|-----|------|---------|---------|-------------|');
  for (const entry of CONFIG_ENTRIES) {
    lines.push(`| \`${entry.key}\` | ${entry.type} | \`${entry.default}\` | \`${entry.envVar}\` | ${entry.description} |`);
  }
  lines.push('');

  // Environment Variables
  lines.push('## Environment Variables');
  lines.push('');
  lines.push('| Variable | Description |');
  lines.push('|----------|-------------|');
  for (const entry of CONFIG_ENTRIES) {
    if (entry.envVar) {
      lines.push(`| \`${entry.envVar}\` | ${entry.description} |`);
    }
  }
  lines.push('');

  return lines.join('\n');
}

function generateManpage(name: string, version: string): string {
  const lines: string[] = [];
  lines.push('.TH "XERGON" "1" "" "' + version + '" "Xergon CLI Manual"');
  lines.push('.SH NAME');
  lines.push(`${name} \\- Decentralized AI inference on the Xergon Network`);
  lines.push('.SH SYNOPSIS');
  lines.push(name + ' <command> [options] [arguments]');
  lines.push('.SH DESCRIPTION');
  lines.push(`${name} is a command-line interface for the Xergon Network SDK.`);
  lines.push('It provides access to decentralized AI inference, model management,');
  lines.push('GPU rental, and blockchain-based payments on the Ergo platform.');
  lines.push('.SH COMMANDS');

  for (const cmd of CLI_COMMANDS) {
    lines.push('.TP');
    lines.push(`.B ${cmd.name}`);
    if (cmd.aliases.length > 0) {
      lines.push(`(${cmd.aliases.join(', ')})`);
    }
    lines.push(cmd.description);
  }

  lines.push('.SH CONFIGURATION');
  lines.push('Configuration is stored in ~/.xergon/config.json.');
  lines.push('.TP');
  lines.push('.B XERGON_BASE_URL');
  lines.push('Relay base URL (default: https://relay.xergon.gg)');
  lines.push('.TP');
  lines.push('.B XERGON_API_KEY');
  lines.push('Public key for authentication');
  lines.push('.TP');
  lines.push('.B XERGON_DEFAULT_MODEL');
  lines.push('Default model for chat completions');
  lines.push('.SH FILES');
  lines.push('.TP');
  lines.push('.B ~/.xergon/config.json');
  lines.push('User configuration file');
  lines.push('.TP');
  lines.push('.B ~/.xergon/profiles.json');
  lines.push('Configuration profiles');

  return lines.join('\n');
}

/**
 * Generate API reference documentation.
 */
export function generateAPIDocs(format: 'markdown' | 'openapi' = 'markdown'): string {
  if (format === 'openapi') {
    return generateOpenAPISpec();
  }

  return generateMarkdownAPIDocs();
}

function generateMarkdownAPIDocs(): string {
  const lines: string[] = [];

  lines.push('# API Reference');
  lines.push('');
  lines.push('Base URL: `https://relay.xergon.gg`');
  lines.push('');
  lines.push('All API endpoints accept and return JSON. Authenticated endpoints require an `X-Xergon-Public-Key` header with HMAC signature.');
  lines.push('');

  for (const endpoint of API_ENDPOINTS) {
    const methodColor = endpoint.method === 'GET' ? '🟢' : endpoint.method === 'POST' ? '🟡' : '🔵';
    lines.push(`## ${methodColor} \`${endpoint.method} ${endpoint.path}\``);
    lines.push('');
    lines.push(endpoint.description);
    lines.push('');

    if (endpoint.auth) {
      lines.push('**Authentication:** Required');
      lines.push('');
    }

    if (endpoint.parameters.length > 0) {
      lines.push('**Parameters:**');
      lines.push('');
      lines.push('| Name | Type | Required | Description |');
      lines.push('|------|------|----------|-------------|');
      for (const p of endpoint.parameters) {
        lines.push(`| \`${p.name}\` | ${p.type} | ${p.required ? 'Yes' : 'No'} | ${p.description} |`);
      }
      lines.push('');
    }

    lines.push('**Responses:**');
    lines.push('');
    for (const r of endpoint.responses) {
      lines.push(`- \`${r.code}\` - ${r.description}`);
    }
    lines.push('');
  }

  return lines.join('\n');
}

function generateOpenAPISpec(): string {
  const spec: any = {
    openapi: '3.0.0',
    info: {
      title: 'Xergon Relay API',
      version: '0.1.0',
      description: 'Decentralized AI inference relay on Ergo',
    },
    servers: [{ url: 'https://relay.xergon.gg' }],
    paths: {},
  };

  for (const endpoint of API_ENDPOINTS) {
    const pathKey = endpoint.path;
    if (!spec.paths[pathKey]) {
      spec.paths[pathKey] = {};
    }

    const operation: any = {
      summary: endpoint.description,
      operationId: `${endpoint.method.toLowerCase()}${endpoint.path.replace(/\//g, '_')}`,
      responses: {},
    };

    if (endpoint.auth) {
      operation.security = [{ hmacAuth: [] }];
    }

    if (endpoint.parameters.length > 0) {
      operation.requestBody = {
        content: {
          'application/json': {
            schema: {
              type: 'object',
              properties: Object.fromEntries(
                endpoint.parameters.map(p => [p.name, { type: p.type.split('|')[0].trim(), description: p.description }]),
              ),
              required: endpoint.parameters.filter(p => p.required).map(p => p.name),
            },
          },
        },
      };
    }

    for (const r of endpoint.responses) {
      operation.responses[r.code] = { description: r.description };
    }

    spec.paths[pathKey][endpoint.method.toLowerCase()] = operation;
  }

  spec.components = {
    securitySchemes: {
      hmacAuth: {
        type: 'apiKey',
        in: 'header',
        name: 'X-Xergon-Public-Key',
        description: 'HMAC authentication using Ergo keypair',
      },
    },
  };

  return JSON.stringify(spec, null, 2);
}

/**
 * Generate configuration reference documentation.
 */
export function generateConfigDocs(): string {
  const lines: string[] = [];

  lines.push('# Configuration Reference');
  lines.push('');
  lines.push('The Xergon SDK can be configured via config file, environment variables, or profiles.');
  lines.push('');
  lines.push('## Config File');
  lines.push('');
  lines.push('Location: `~/.xergon/config.json`');
  lines.push('');
  lines.push('```json');
  lines.push('{');
  lines.push('  "baseUrl": "https://relay.xergon.gg",');
  lines.push('  "apiKey": "your-public-key",');
  lines.push('  "defaultModel": "llama-3.3-70b",');
  lines.push('  "outputFormat": "text",');
  lines.push('  "color": true,');
  lines.push('  "timeout": 30000');
  lines.push('}');
  lines.push('```');
  lines.push('');
  lines.push('## Configuration Options');
  lines.push('');
  lines.push('| Key | Type | Default | Environment Variable | Description |');
  lines.push('|-----|------|---------|---------------------|-------------|');

  for (const entry of CONFIG_ENTRIES) {
    lines.push(`| \`${entry.key}\` | \`${entry.type}\` | \`${entry.default}\` | \`${entry.envVar || '(none)'}\` | ${entry.description} |`);
  }
  lines.push('');

  lines.push('## Profiles');
  lines.push('');
  lines.push('You can define multiple configuration profiles in `~/.xergon/profiles.json`:');
  lines.push('');
  lines.push('```json');
  lines.push('{');
  lines.push('  "activeProfile": "production",');
  lines.push('  "profiles": {');
  lines.push('    "development": {');
  lines.push('      "baseUrl": "http://localhost:3000",');
  lines.push('      "defaultModel": "llama-3.1-8b"');
  lines.push('    },');
  lines.push('    "production": {');
  lines.push('      "baseUrl": "https://relay.xergon.gg",');
  lines.push('      "defaultModel": "llama-3.3-70b"');
  lines.push('    }');
  lines.push('  }');
  lines.push('}');
  lines.push('```');
  lines.push('');
  lines.push('## Priority Order');
  lines.push('');
  lines.push('Configuration values are resolved in the following priority (highest first):');
  lines.push('');
  lines.push('1. Environment variables');
  lines.push('2. Config file (`~/.xergon/config.json`)');
  lines.push('3. Active profile');
  lines.push('4. Hardcoded defaults');
  lines.push('');

  return lines.join('\n');
}

/**
 * Generate plugin development guide.
 */
export function generatePluginDocs(): string {
  const lines: string[] = [];

  lines.push('# Plugin Development Guide');
  lines.push('');
  lines.push('Plugins extend the Xergon SDK with custom hooks and middleware.');
  lines.push('');
  lines.push('## Creating a Plugin');
  lines.push('');
  lines.push('```typescript');
  lines.push("import type { Plugin, PluginManifest, PluginHooks } from '@xergon/sdk';");
  lines.push('');
  lines.push('const myPlugin: Plugin = {');
  lines.push('  manifest: {');
  lines.push("    name: 'my-plugin',");
  lines.push("    version: '1.0.0',");
  lines.push("    description: 'A custom plugin',");
  lines.push('  } as PluginManifest,');
  lines.push('');
  lines.push('  hooks: {');
  lines.push("    'beforeRequest': async (request) => {");
  lines.push('      // Modify request before sending');
  lines.push('      console.log(`Sending to: ${request.url}`);');
  lines.push('      return request;');
  lines.push('    },');
  lines.push('');
  lines.push("    'afterResponse': async (response) => {");
  lines.push('      // Process response after receiving');
  lines.push('      console.log(`Status: ${response.status}`);');
  lines.push('      return response;');
  lines.push('    },');
  lines.push('  } as PluginHooks,');
  lines.push('};');
  lines.push('```');
  lines.push('');
  lines.push('## Registering a Plugin');
  lines.push('');
  lines.push('```typescript');
  lines.push("import { PluginManager } from '@xergon/sdk';");
  lines.push('');
  lines.push('const manager = new PluginManager();');
  lines.push('manager.register(myPlugin);');
  lines.push('```');
  lines.push('');
  lines.push('## Built-in Plugins');
  lines.push('');
  lines.push('| Plugin | Description |');
  lines.push('|--------|-------------|');
  lines.push('| `loggingPlugin` | Logs all requests and responses |');
  lines.push('| `retryPlugin` | Automatically retries failed requests |');
  lines.push('| `cachePlugin` | Caches GET request responses |');
  lines.push('| `rateLimitDisplayPlugin` | Displays rate limit information |');
  lines.push('');

  return lines.join('\n');
}

/**
 * Generate quick start guide.
 */
export function generateQuickStart(): string {
  const lines: string[] = [];

  lines.push('# Quick Start Guide');
  lines.push('');
  lines.push('Get started with the Xergon SDK in under 5 minutes.');
  lines.push('');
  lines.push('## 1. Install');
  lines.push('');
  lines.push('```bash');
  lines.push('npm install @xergon/sdk');
  lines.push('```');
  lines.push('');
  lines.push('## 2. Initialize');
  lines.push('');
  lines.push('```bash');
  lines.push('npx xergon onboard');
  lines.push('```');
  lines.push('');
  lines.push('Or manually create your config:');
  lines.push('```bash');
  lines.push('mkdir -p ~/.xergon');
  lines.push("echo '{\"baseUrl\": \"https://relay.xergon.gg\"}' > ~/.xergon/config.json");
  lines.push('```');
  lines.push('');
  lines.push('## 3. Authenticate');
  lines.push('');
  lines.push('```bash');
  lines.push('npx xergon login');
  lines.push('```');
  lines.push('');
  lines.push('## 4. Chat');
  lines.push('');
  lines.push('```bash');
  lines.push('# Interactive chat');
  lines.push('npx xergon chat');
  lines.push('');
  lines.push('# One-shot completion');
  lines.push('npx xergon chat --prompt "Hello, Xergon!"');
  lines.push('```');
  lines.push('');
  lines.push('## 5. Use the SDK in Code');
  lines.push('');
  lines.push('```typescript');
  lines.push("import { XergonClient } from '@xergon/sdk';");
  lines.push('');
  lines.push('const client = new XergonClient({');
  lines.push("  baseUrl: 'https://relay.xergon.gg',");
  lines.push("  publicKey: 'your-ergo-public-key',");
  lines.push('});');
  lines.push('');
  lines.push('// List models');
  lines.push('const models = await client.models.list();');
  lines.push('console.log(models);');
  lines.push('');
  lines.push('// Chat completion');
  lines.push('const response = await client.chat.completions.create({');
  lines.push("  model: 'llama-3.3-70b',");
  lines.push('  messages: [{ role: \'user\', content: \'Hello!\' }],');
  lines.push('});');
  lines.push('console.log(response.choices[0].message.content);');
  lines.push('```');
  lines.push('');
  lines.push('## 6. Run Diagnostics');
  lines.push('');
  lines.push('```bash');
  lines.push('npx xergon status    # Quick health check');
  lines.push('npx xergon debug     # Full diagnostics');
  lines.push('npx xergon debug dump  # Generate support dump');
  lines.push('```');
  lines.push('');
  lines.push('## Next Steps');
  lines.push('');
  lines.push('- [Full CLI Reference](#)');
  lines.push('- [API Reference](#)');
  lines.push('- [Configuration Guide](#)');
  lines.push('- [Plugin Development](#)');
  lines.push('');

  return lines.join('\n');
}

/**
 * Generate a CLI cheatsheet (markdown table format).
 */
export function generateCheatsheet(): string {
  const lines: string[] = [];

  lines.push('# Xergon CLI Cheatsheet');
  lines.push('');
  lines.push('## Common Commands');
  lines.push('');
  lines.push('| Command | Description |');
  lines.push('|---------|-------------|');

  const cheats: Array<[string, string]> = [
    ['xergon status', 'System health check'],
    ['xergon chat', 'Interactive chat session'],
    ['xergon chat -m llama-3.1-8b', 'Chat with specific model'],
    ['xergon models', 'List available models'],
    ['xergon model search <query>', 'Search models'],
    ['xergon model info <id>', 'Model details'],
    ['xergon model compare <id1> <id2>', 'Compare models'],
    ['xergon model recommend --task code', 'Get recommendations'],
    ['xergon model popular', 'Popular models'],
    ['xergon model versions <id>', 'Model version history'],
    ['xergon model lineage <id>', 'Model lineage tree'],
    ['xergon balance', 'Check ERG balance'],
    ['xergon provider list', 'List providers'],
    ['xergon config show', 'Show configuration'],
    ['xergon config set <key> <val>', 'Set config value'],
    ['xergon login', 'Authenticate'],
    ['xergon debug', 'Run all diagnostics'],
    ['xergon debug connection', 'Test connectivity'],
    ['xergon debug models', 'Check model availability'],
    ['xergon debug wallet', 'Verify wallet'],
    ['xergon debug disk', 'Check disk space'],
    ['xergon debug network', 'Measure latency'],
    ['xergon debug system', 'System information'],
    ['xergon debug dump', 'Full debug dump'],
    ['xergon debug troubleshoot', 'Guided troubleshooting'],
    ['xergon embed <text>', 'Create embeddings'],
    ['xergon audio tts <text>', 'Text-to-speech'],
    ['xergon audio stt <file>', 'Speech-to-text'],
    ['xergon upload <file>', 'Upload file'],
    ['xergon bench -m <model>', 'Run benchmark'],
    ['xergon workspace create <name>', 'Create workspace'],
    ['xergon template list', 'List templates'],
    ['xergon alias list', 'List model aliases'],
    ['xergon plugin list', 'List installed plugins'],
    ['xergon deploy list', 'List deployments'],
    ['xergon eval run', 'Run evaluation'],
    ['xergon export <scope>', 'Export data'],
    ['xergon serve -p 8080', 'Start local API proxy'],
    ['xergon version', 'Show version'],
    ['xergon docs cheatsheet', 'This cheatsheet'],
  ];

  for (const [cmd, desc] of cheats) {
    lines.push(`| \`${cmd}\` | ${desc} |`);
  }
  lines.push('');

  lines.push('## Global Options');
  lines.push('');
  lines.push('| Flag | Description |');
  lines.push('|------|-------------|');
  lines.push('| `-j, --json` | Output in JSON format |');
  lines.push('| `--config <path>` | Path to config file |');
  lines.push('| `-h, --help` | Show help |');
  lines.push('');

  lines.push('## Environment Variables');
  lines.push('');
  lines.push('| Variable | Description |');
  lines.push('|----------|-------------|');
  lines.push('| `XERGON_BASE_URL` | Relay base URL |');
  lines.push('| `XERGON_API_KEY` | Public key for auth |');
  lines.push('| `XERGON_DEFAULT_MODEL` | Default model |');
  lines.push('| `XERGON_OUTPUT_FORMAT` | Output format (text/json/table) |');
  lines.push('| `XERGON_TIMEOUT` | Request timeout (ms) |');
  lines.push('| `NO_COLOR` | Disable colored output |');
  lines.push('');

  return lines.join('\n');
}

/**
 * Serve documentation locally with live reload.
 * Returns a cleanup function to stop the server.
 */
export function serveDocs(port: number = 8080): { server: http.Server; url: string; close: () => void } {
  const cheatsheet = generateCheatsheet();
  const quickStart = generateQuickStart();
  const apiDocs = generateAPIDocs('markdown');
  const configDocs = generateConfigDocs();

  const indexHtml = `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>Xergon SDK Documentation</title>
  <style>
    body { font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; max-width: 900px; margin: 0 auto; padding: 20px; background: #0d1117; color: #c9d1d9; }
    a { color: #58a6ff; text-decoration: none; }
    a:hover { text-decoration: underline; }
    h1, h2, h3 { color: #f0f6fc; border-bottom: 1px solid #30363d; padding-bottom: 8px; }
    code { background: #161b22; padding: 2px 6px; border-radius: 4px; font-size: 0.9em; }
    pre { background: #161b22; padding: 16px; border-radius: 8px; overflow-x: auto; }
    pre code { background: none; padding: 0; }
    table { width: 100%; border-collapse: collapse; }
    th, td { padding: 8px 12px; text-align: left; border-bottom: 1px solid #30363d; }
    th { color: #f0f6fc; }
    nav { background: #161b22; padding: 16px; border-radius: 8px; margin-bottom: 24px; }
    nav a { margin-right: 16px; }
  </style>
</head>
<body>
  <h1>Xergon SDK Documentation</h1>
  <nav>
    <a href="/">Home</a>
    <a href="/quickstart">Quick Start</a>
    <a href="/api">API Reference</a>
    <a href="/config">Configuration</a>
    <a href="/cheatsheet">CLI Cheatsheet</a>
  </nav>
  <div id="content">
    <h2>Welcome</h2>
    <p>Use the navigation above to browse documentation.</p>
    <h2>Quick Links</h2>
    <ul>
      <li><a href="/quickstart">Quick Start Guide</a></li>
      <li><a href="/api">API Reference</a></li>
      <li><a href="/config">Configuration Reference</a></li>
      <li><a href="/cheatsheet">CLI Cheatsheet</a></li>
    </ul>
  </div>
</body>
</html>`;

  const routes: Record<string, string> = {
    '/': indexHtml,
    '/quickstart': markdownToHtml(quickStart),
    '/api': markdownToHtml(apiDocs),
    '/config': markdownToHtml(configDocs),
    '/cheatsheet': markdownToHtml(cheatsheet),
  };

  const server = http.createServer((req: http.IncomingMessage, res: http.ServerResponse) => {
    const urlPath = req.url?.split('?')[0] ?? '/';
    const content = routes[urlPath];

    if (content) {
      res.writeHead(200, { 'Content-Type': 'text/html; charset=utf-8' });
      res.end(content);
    } else {
      res.writeHead(404, { 'Content-Type': 'text/plain' });
      res.end('Not Found');
    }
  });

  server.listen(port);
  const url = `http://localhost:${port}`;

  return {
    server,
    url,
    close: () => { server.close(); },
  };
}

/**
 * Simple markdown-to-HTML converter (no external deps).
 */
function markdownToHtml(md: string): string {
  let html = md
    // Headers
    .replace(/^### (.+)$/gm, '<h3>$1</h3>')
    .replace(/^## (.+)$/gm, '<h2>$1</h2>')
    .replace(/^# (.+)$/gm, '<h1>$1</h1>')
    // Code blocks
    .replace(/```(\w*)\n([\s\S]*?)```/g, '<pre><code>$2</code></pre>')
    // Inline code
    .replace(/`([^`]+)`/g, '<code>$1</code>')
    // Bold
    .replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>')
    // Italic
    .replace(/\*([^*]+)\*/g, '<em>$1</em>')
    // Links
    .replace(/\[([^\]]+)\]\(([^)]+)\)/g, '<a href="$2">$1</a>')
    // Blockquotes
    .replace(/^> (.+)$/gm, '<blockquote>$1</blockquote>')
    // Unordered lists
    .replace(/^- (.+)$/gm, '<li>$1</li>')
    // Table rows
    .replace(/^\|(.+)\|$/gm, (match) => {
      const cells = match.split('|').filter(c => c.trim() !== '');
      if (cells.every(c => /^[\s-:]+$/.test(c))) return '';
      const isHeader = false;
      const tag = isHeader ? 'th' : 'td';
      return '<tr>' + cells.map(c => `<${tag}>${c.trim()}</${tag}>`).join('') + '</tr>';
    })
    // Paragraphs (double newlines)
    .replace(/\n\n/g, '</p><p>')
    // Line breaks
    .replace(/\n/g, '<br>');

  // Wrap in HTML
  return `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>Xergon SDK Documentation</title>
  <style>
    body { font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; max-width: 900px; margin: 0 auto; padding: 20px; background: #0d1117; color: #c9d1d9; line-height: 1.6; }
    a { color: #58a6ff; text-decoration: none; }
    a:hover { text-decoration: underline; }
    h1, h2, h3 { color: #f0f6fc; border-bottom: 1px solid #30363d; padding-bottom: 8px; margin-top: 24px; }
    code { background: #161b22; padding: 2px 6px; border-radius: 4px; font-size: 0.9em; }
    pre { background: #161b22; padding: 16px; border-radius: 8px; overflow-x: auto; }
    pre code { background: none; padding: 0; }
    table { width: 100%; border-collapse: collapse; margin: 16px 0; }
    th, td { padding: 8px 12px; text-align: left; border-bottom: 1px solid #30363d; }
    th { color: #f0f6fc; }
    blockquote { border-left: 3px solid #58a6ff; padding-left: 16px; color: #8b949e; margin: 16px 0; }
    li { margin: 4px 0; }
    nav { background: #161b22; padding: 16px; border-radius: 8px; margin-bottom: 24px; }
    nav a { margin-right: 16px; font-weight: 500; }
    a.back { display: inline-block; margin-bottom: 16px; }
  </style>
</head>
<body>
  <a class="back" href="/">&larr; Back to Home</a>
  <p>${html}</p>
</body>
</html>`;
}
