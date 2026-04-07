/**
 * CLI command: deploy
 *
 * Deploy models as services on the Xergon Network.
 *
 * Usage:
 *   xergon deploy <model> --port 8080 --gpu 0
 *   xergon deploy list
 *   xergon deploy stop <id>
 *   xergon deploy logs <id>
 */

import type { Command, CommandOption, ParsedArgs, CLIContext } from '../mod';
import type { Deployment, DeploymentLog } from '../../deploy';

async function deployAction(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const sub = args.positional[0];

  if (!sub) {
    ctx.output.writeError('Usage: xergon deploy <model|list|stop|logs> [options]');
    process.exit(1);
    return;
  }

  switch (sub) {
    case 'list':
      await handleList(args, ctx);
      break;
    case 'stop':
      await handleStop(args, ctx);
      break;
    case 'logs':
      await handleLogs(args, ctx);
      break;
    default:
      // Treat as model name to deploy
      await handleDeploy(sub, args, ctx);
      break;
  }
}

// ── deploy model ───────────────────────────────────────────────────

async function handleDeploy(model: string, args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const port = args.options.port !== undefined ? Number(args.options.port) : undefined;
  const gpu = args.options.gpu !== undefined ? Number(args.options.gpu) : undefined;
  const memoryLimit = args.options.memory_limit ? String(args.options.memory_limit) : undefined;
  const envVars: Record<string, string> = {};

  // Parse --env flags (can be multiple): --env KEY=VALUE
  if (args.options.env) {
    const envStr = String(args.options.env);
    const eqIdx = envStr.indexOf('=');
    if (eqIdx > 0) {
      envVars[envStr.substring(0, eqIdx).trim()] = envStr.substring(eqIdx + 1).trim();
    }
  }

  const thinkingMsg = ctx.output.colorize('Deploying model', 'cyan');
  process.stderr.write(`${thinkingMsg}...\r`);

  try {
    const { deploy } = await import('../../deploy');
    const deployment = await deploy(ctx.client._core || ctx.client.core, {
      model,
      port,
      gpu,
      memory_limit: memoryLimit,
      env: Object.keys(envVars).length > 0 ? envVars : undefined,
    });

    process.stderr.write(' '.repeat(40) + '\r');

    ctx.output.success('Model deployed successfully');
    ctx.output.write('');
    ctx.output.write(ctx.output.formatText({
      ID: deployment.id,
      Model: deployment.model,
      Status: deployment.status,
      URL: deployment.url,
      Port: String(deployment.port),
      GPU: deployment.gpu !== undefined ? String(deployment.gpu) : 'auto',
      'Memory Limit': deployment.memory_limit || 'default',
    }, 'Deployment'));
  } catch (err) {
    process.stderr.write(' '.repeat(40) + '\r');
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to deploy model: ${message}`);
    process.exit(1);
  }
}

// ── list deployments ───────────────────────────────────────────────

async function handleList(_args: ParsedArgs, ctx: CLIContext): Promise<void> {
  try {
    const { listDeployments } = await import('../../deploy');
    const deployments = await listDeployments(ctx.client._core || ctx.client.core);

    if (deployments.length === 0) {
      ctx.output.info('No active deployments found.');
      return;
    }

    const tableData = deployments.map((d: Deployment) => ({
      ID: d.id,
      Model: d.model,
      Status: d.status,
      URL: d.url,
      Port: String(d.port),
      GPU: d.gpu !== undefined ? String(d.gpu) : '-',
      Created: new Date(d.created_at).toISOString().slice(0, 19),
    }));
    ctx.output.write(ctx.output.formatTable(tableData, `Deployments (${deployments.length})`));
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to list deployments: ${message}`);
    process.exit(1);
  }
}

// ── stop deployment ────────────────────────────────────────────────

async function handleStop(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const deployId = args.positional[1];

  if (!deployId) {
    ctx.output.writeError('No deployment ID specified. Use: xergon deploy stop <id>');
    process.exit(1);
    return;
  }

  try {
    const { stopDeployment } = await import('../../deploy');
    const deployment = await stopDeployment(ctx.client._core || ctx.client.core, deployId);

    ctx.output.success(`Deployment ${deployId} stopped successfully`);
    ctx.output.write(`  Status: ${deployment.status}`);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to stop deployment: ${message}`);
    process.exit(1);
  }
}

// ── logs ───────────────────────────────────────────────────────────

async function handleLogs(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const deployId = args.positional[1];
  const limit = args.options.limit !== undefined ? Number(args.options.limit) : 50;
  const level = args.options.level ? String(args.options.level) : undefined;

  if (!deployId) {
    ctx.output.writeError('No deployment ID specified. Use: xergon deploy logs <id>');
    process.exit(1);
    return;
  }

  try {
    const { getDeploymentLogs } = await import('../../deploy');
    const logs = await getDeploymentLogs(ctx.client._core || ctx.client.core, deployId, {
      limit,
      level,
    });

    if (logs.length === 0) {
      ctx.output.info('No logs found for this deployment.');
      return;
    }

    const levelColors: Record<string, string> = {
      info: 'cyan',
      warn: 'yellow',
      error: 'red',
      debug: 'dim',
    };

    ctx.output.write(ctx.output.colorize(`Deployment Logs (${deployId})`, 'bold'));
    ctx.output.write('');

    for (const log of logs) {
      const ts = new Date(log.timestamp).toISOString().slice(11, 19);
      const lvl = ctx.output.colorize(log.level.toUpperCase().padEnd(5), (levelColors[log.level] || 'dim') as 'dim');
      const msg = log.message;
      ctx.output.write(`  ${ts}  ${lvl}  ${msg}`);
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to get deployment logs: ${message}`);
    process.exit(1);
  }
}

export const deployCommand: Command = {
  name: 'deploy',
  description: 'Deploy models as services on the Xergon Network',
  aliases: ['serve-model'],
  options: [
    {
      name: 'port',
      short: '-p',
      long: '--port',
      description: 'Port for the deployed service',
      required: false,
      type: 'number',
    },
    {
      name: 'gpu',
      short: '-g',
      long: '--gpu',
      description: 'GPU device index (e.g., 0, 1)',
      required: false,
      type: 'number',
    },
    {
      name: 'memory_limit',
      short: '',
      long: '--memory-limit',
      description: 'Memory limit for the deployment (e.g., 8Gi)',
      required: false,
      type: 'string',
    },
    {
      name: 'env',
      short: '',
      long: '--env',
      description: 'Environment variable (KEY=VALUE, can be repeated)',
      required: false,
      type: 'string',
    },
    {
      name: 'limit',
      short: '-n',
      long: '--limit',
      description: 'Number of log entries to show (default: 50)',
      required: false,
      type: 'number',
    },
    {
      name: 'level',
      short: '',
      long: '--level',
      description: 'Filter logs by level: info, warn, error, debug',
      required: false,
      type: 'string',
    },
  ],
  action: deployAction,
};
