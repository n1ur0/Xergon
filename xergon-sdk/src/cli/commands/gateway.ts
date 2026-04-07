/**
 * CLI command: gateway
 *
 * Manage the Xergon API Gateway for routing, rate limiting,
 * auth, and load balancing across model endpoints.
 *
 * Usage:
 *   xergon gateway start [--config gateway.yaml]
 *   xergon gateway stop
 *   xergon gateway routes
 *   xergon gateway add-route --path /v1/chat --upstream http://localhost:8080 --model gpt-4
 *   xergon gateway remove-route --path /v1/chat
 *   xergon gateway metrics
 *   xergon gateway health
 *   xergon gateway reload
 */

import type { Command, ParsedArgs, CLIContext } from '../mod';
import * as fs from 'node:fs';
import * as path from 'node:path';
import type { GatewayConfig, GatewayRoute, GatewayMetrics, GatewayHealth } from '../../gateway';

// Persisted gateway instance across subcommands
let gatewayInstance: any = null;
const PID_FILE = path.join(require('os').tmpdir(), 'xergon-gateway.pid');

async function gatewayAction(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const sub = args.positional[0];

  if (!sub) {
    ctx.output.writeError('Usage: xergon gateway <start|stop|routes|add-route|remove-route|metrics|health|reload> [options]');
    process.exit(1);
    return;
  }

  switch (sub) {
    case 'start':
      await handleStart(args, ctx);
      break;
    case 'stop':
      await handleStop(args, ctx);
      break;
    case 'routes':
      await handleRoutes(args, ctx);
      break;
    case 'add-route':
      await handleAddRoute(args, ctx);
      break;
    case 'remove-route':
      await handleRemoveRoute(args, ctx);
      break;
    case 'metrics':
      await handleMetrics(args, ctx);
      break;
    case 'health':
      await handleHealth(args, ctx);
      break;
    case 'reload':
      await handleReload(args, ctx);
      break;
    default:
      ctx.output.writeError(`Unknown subcommand: ${sub}`);
      ctx.output.writeError('Usage: xergon gateway <start|stop|routes|add-route|remove-route|metrics|health|reload> [options]');
      process.exit(1);
  }
}

// ── start ──────────────────────────────────────────────────────────

async function handleStart(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const configPath = args.options.config ? String(args.options.config) : 'gateway.yaml';
  const port = args.options.port !== undefined ? Number(args.options.port) : 3000;

  let config: GatewayConfig;

  try {
    if (fs.existsSync(configPath)) {
      const raw = fs.readFileSync(configPath, 'utf-8');
      const parsed = typeof require !== 'undefined' ? require('js-yaml')?.load?.(raw) : null;
      if (parsed) {
        config = { ...getDefaultConfig(port), ...parsed };
      } else {
        config = getDefaultConfig(port);
      }
    } else {
      config = getDefaultConfig(port);
    }
  } catch {
    config = getDefaultConfig(port);
  }

  ctx.output.info(ctx.output.colorize('Starting Xergon Gateway...', 'cyan'));

  try {
    const { Gateway } = await import('../../gateway');
    const gw = new Gateway(config);
    await gw.start();
    gatewayInstance = gw;

    // Write PID file
    fs.writeFileSync(PID_FILE, String(process.pid));

    ctx.output.success(`Gateway running on ${config.host}:${config.port}`);
    ctx.output.write(`  Routes: ${config.routes.length}`);
    ctx.output.write(`  Rate limit: ${config.rateLimit.enabled ? 'enabled' : 'disabled'}`);
    ctx.output.write(`  Auth: ${config.auth.type}`);
    ctx.output.write(`  PID: ${process.pid}`);
    ctx.output.write('');
    ctx.output.write('Press Ctrl+C to stop.');

    // Keep alive
    process.on('SIGINT', async () => {
      ctx.output.info('Shutting down gateway...');
      await gw.stop();
      try { fs.unlinkSync(PID_FILE); } catch { /* ignore */ }
      process.exit(0);
    });
    process.on('SIGTERM', async () => {
      await gw.stop();
      try { fs.unlinkSync(PID_FILE); } catch { /* ignore */ }
      process.exit(0);
    });
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to start gateway: ${message}`);
    process.exit(1);
  }
}

// ── stop ───────────────────────────────────────────────────────────

async function handleStop(_args: ParsedArgs, ctx: CLIContext): Promise<void> {
  if (fs.existsSync(PID_FILE)) {
    const pid = parseInt(fs.readFileSync(PID_FILE, 'utf-8').trim(), 10);
    try {
      process.kill(pid, 'SIGTERM');
      ctx.output.success(`Sent SIGTERM to gateway process (PID: ${pid})`);
      try { fs.unlinkSync(PID_FILE); } catch { /* ignore */ }
    } catch {
      ctx.output.writeError(`Gateway process (PID: ${pid}) is not running`);
      try { fs.unlinkSync(PID_FILE); } catch { /* ignore */ }
    }
  } else {
    ctx.output.info('No running gateway found (no PID file)');
  }
}

// ── routes ─────────────────────────────────────────────────────────

async function handleRoutes(_args: ParsedArgs, ctx: CLIContext): Promise<void> {
  try {
    const { Gateway } = await import('../../gateway');
    const gw = gatewayInstance || new Gateway(getDefaultConfig());
    const routes = gw.getRoutes();

    if (routes.length === 0) {
      ctx.output.info('No routes configured.');
      return;
    }

    const tableData = routes.map((r: GatewayRoute) => ({
      Path: r.path,
      Methods: r.methods.join(', '),
      Upstream: r.upstream,
      Model: r.modelId,
      Timeout: `${r.timeout}ms`,
      Retry: String(r.retryPolicy.maxRetries),
      Cache: r.cache.enabled ? `${r.cache.ttlSeconds}s` : 'off',
    }));
    ctx.output.write(ctx.output.formatTable(tableData, `Gateway Routes (${routes.length})`));
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to list routes: ${message}`);
    process.exit(1);
  }
}

// ── add-route ──────────────────────────────────────────────────────

async function handleAddRoute(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const routePath = args.options.path ? String(args.options.path) : undefined;
  const upstream = args.options.upstream ? String(args.options.upstream) : undefined;
  const model = args.options.model ? String(args.options.model) : undefined;
  const timeout = args.options.timeout !== undefined ? Number(args.options.timeout) : 30000;

  if (!routePath || !upstream || !model) {
    ctx.output.writeError('Usage: xergon gateway add-route --path <path> --upstream <url> --model <modelId>');
    process.exit(1);
    return;
  }

  try {
    const { Gateway } = await import('../../gateway');
    const gw = gatewayInstance || new Gateway(getDefaultConfig());

    const route: GatewayRoute = {
      path: routePath,
      methods: ['GET', 'POST', 'PUT', 'DELETE', 'OPTIONS'],
      upstream,
      modelId: model,
      timeout,
      retryPolicy: { maxRetries: 2, retryDelay: 500, retryOn: [502, 503, 504], backoffMultiplier: 2 },
      cache: { enabled: false, ttlSeconds: 60, maxSize: 1000 },
    };

    gw.addRoute(route);
    ctx.output.success(`Route added: ${routePath} -> ${upstream} (${model})`);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to add route: ${message}`);
    process.exit(1);
  }
}

// ── remove-route ───────────────────────────────────────────────────

async function handleRemoveRoute(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const routePath = args.options.path ? String(args.options.path) : undefined;

  if (!routePath) {
    ctx.output.writeError('Usage: xergon gateway remove-route --path <path>');
    process.exit(1);
    return;
  }

  try {
    const { Gateway } = await import('../../gateway');
    const gw = gatewayInstance || new Gateway(getDefaultConfig());
    const removed = gw.removeRoute(routePath);

    if (removed) {
      ctx.output.success(`Route removed: ${routePath}`);
    } else {
      ctx.output.writeError(`Route not found: ${routePath}`);
      process.exit(1);
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to remove route: ${message}`);
    process.exit(1);
  }
}

// ── metrics ────────────────────────────────────────────────────────

async function handleMetrics(_args: ParsedArgs, ctx: CLIContext): Promise<void> {
  try {
    const { Gateway } = await import('../../gateway');
    const gw = gatewayInstance || new Gateway(getDefaultConfig());
    const m: GatewayMetrics = gw.getMetrics();

    ctx.output.write(ctx.output.colorize('Gateway Metrics', 'bold'));
    ctx.output.write('');
    ctx.output.write(ctx.output.formatText({
      'Total Requests': String(m.totalRequests),
      'Active Requests': String(m.activeRequests),
      'Total Errors': String(m.totalErrors),
      'Avg Latency': `${m.avgLatencyMs}ms`,
      'P50 Latency': `${m.p50Latency}ms`,
      'P99 Latency': `${m.p99Latency}ms`,
      'Rate Limit Hits': String(m.rateLimitHits),
      'Cache Hits': String(m.cacheHits),
      'Cache Misses': String(m.cacheMisses),
      'Requests/min': String(m.requestsPerMinute),
      'Uptime': `${m.uptimeSeconds}s`,
    }, 'Metrics'));
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to get metrics: ${message}`);
    process.exit(1);
  }
}

// ── health ─────────────────────────────────────────────────────────

async function handleHealth(_args: ParsedArgs, ctx: CLIContext): Promise<void> {
  try {
    const { Gateway } = await import('../../gateway');
    const gw = gatewayInstance || new Gateway(getDefaultConfig());
    const h: GatewayHealth = gw.getHealth();

    const statusColor = h.status === 'healthy' ? 'green' : h.status === 'degraded' ? 'yellow' : 'red';
    ctx.output.write(ctx.output.colorize(`Gateway Health: ${h.status.toUpperCase()}`, statusColor as 'green'));
    ctx.output.write('');
    ctx.output.write(ctx.output.formatText({
      Status: h.status,
      Uptime: `${h.uptime}s`,
      'Total Routes': String(h.totalRoutes),
      'Healthy Routes': String(h.healthyRoutes),
      'Active Connections': String(h.activeConnections),
      'Error Rate': `${h.errorRate}%`,
      'Avg Latency': `${h.avgLatencyMs}ms`,
      Version: h.version,
    }, 'Health'));
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to get health: ${message}`);
    process.exit(1);
  }
}

// ── reload ─────────────────────────────────────────────────────────

async function handleReload(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const configPath = args.options.config ? String(args.options.config) : 'gateway.yaml';

  try {
    const { Gateway } = await import('../../gateway');
    if (!gatewayInstance) {
      ctx.output.writeError('No running gateway instance found. Start one first with: xergon gateway start');
      process.exit(1);
      return;
    }

    let config: GatewayConfig | null = null;
    if (fs.existsSync(configPath)) {
      const raw = fs.readFileSync(configPath, 'utf-8');
      // Try YAML parse, fall back to JSON
      try {
        const yaml = require('js-yaml');
        config = yaml.load(raw) as GatewayConfig;
      } catch {
        try { config = JSON.parse(raw); } catch { /* keep null */ }
      }
    }

    if (!config) {
      ctx.output.writeError(`Could not load config from ${configPath}`);
      process.exit(1);
      return;
    }

    gatewayInstance.reloadConfig(config);
    ctx.output.success('Gateway configuration reloaded');
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to reload config: ${message}`);
    process.exit(1);
  }
}

// ── Defaults ───────────────────────────────────────────────────────

function getDefaultConfig(port: number = 3000): GatewayConfig {
  return {
    host: '0.0.0.0',
    port,
    routes: [],
    rateLimit: {
      enabled: true,
      requestsPerMinute: 60,
      requestsPerHour: 1000,
      burstSize: 10,
      strategy: 'sliding-window',
    },
    auth: {
      enabled: true,
      type: 'api-key',
      apiKeyHeader: 'Authorization',
    },
    loadBalancer: {
      strategy: 'round-robin',
      healthCheckInterval: 15000,
      maxRetries: 3,
      retryDelay: 1000,
      failoverEnabled: true,
    },
    logging: {
      enabled: true,
      level: 'info',
      format: 'text',
    },
    cors: {
      enabled: true,
      origins: ['*'],
      methods: ['GET', 'POST', 'PUT', 'DELETE', 'OPTIONS'],
      headers: ['Content-Type', 'Authorization'],
      maxAge: 86400,
    },
  };
}

// ── Command export ─────────────────────────────────────────────────

export const gatewayCommand: Command = {
  name: 'gateway',
  description: 'Manage the Xergon API Gateway for routing, rate limiting, auth, and load balancing',
  aliases: ['gw'],
  options: [
    {
      name: 'config',
      short: '-c',
      long: '--config',
      description: 'Path to gateway config file (YAML or JSON)',
      required: false,
      type: 'string',
    },
    {
      name: 'port',
      short: '-p',
      long: '--port',
      description: 'Gateway listen port (default: 3000)',
      required: false,
      type: 'number',
    },
    {
      name: 'path',
      short: '',
      long: '--path',
      description: 'Route path (e.g. /v1/chat/completions)',
      required: false,
      type: 'string',
    },
    {
      name: 'upstream',
      short: '-u',
      long: '--upstream',
      description: 'Upstream endpoint URL',
      required: false,
      type: 'string',
    },
    {
      name: 'model',
      short: '-m',
      long: '--model',
      description: 'Model identifier for the route',
      required: false,
      type: 'string',
    },
    {
      name: 'timeout',
      short: '-t',
      long: '--timeout',
      description: 'Request timeout in milliseconds (default: 30000)',
      required: false,
      type: 'number',
    },
  ],
  action: gatewayAction,
};
