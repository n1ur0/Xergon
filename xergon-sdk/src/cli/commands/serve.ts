/**
 * CLI command: serve
 *
 * Starts a local OpenAI-compatible HTTP proxy server that forwards
 * requests to the Xergon relay.
 *
 * Usage: xergon serve [--port <port>] [--relay <url>] [--model <model>] [--api-key <key>]
 */

import * as http from 'node:http';
import type { Command, CommandOption, ParsedArgs, CLIContext } from '../mod';

const serveOptions: CommandOption[] = [
  {
    name: 'port',
    short: '-p',
    long: '--port',
    description: 'Local port to listen on',
    required: false,
    default: '8000',
    type: 'number',
  },
  {
    name: 'relay',
    short: '',
    long: '--relay',
    description: 'Xergon relay URL to proxy to',
    required: false,
    type: 'string',
  },
  {
    name: 'model',
    short: '-m',
    long: '--model',
    description: 'Override model for all requests (default: pass through)',
    required: false,
    type: 'string',
  },
  {
    name: 'apiKey',
    short: '',
    long: '--api-key',
    description: 'Xergon API key / public key for auth',
    required: false,
    type: 'string',
  },
];

/**
 * Read the full request body as a buffer/string.
 */
function readBody(req: http.IncomingMessage): Promise<string> {
  return new Promise((resolve, reject) => {
    const chunks: Buffer[] = [];
    req.on('data', (chunk: Buffer) => chunks.push(chunk));
    req.on('end', () => resolve(Buffer.concat(chunks).toString('utf-8')));
    req.on('error', reject);
  });
}

/**
 * Write a JSON response to an http.ServerResponse.
 */
function writeJson(res: http.ServerResponse, status: number, data: unknown): void {
  const body = JSON.stringify(data);
  res.writeHead(status, {
    'Content-Type': 'application/json',
    'Content-Length': Buffer.byteLength(body),
  });
  res.end(body);
}

/**
 * Build the Xergon auth headers for relay requests.
 */
function buildXergonHeaders(apiKey: string): Record<string, string> {
  const headers: Record<string, string> = {
    'Content-Type': 'application/json',
  };
  if (apiKey) {
    headers['X-Xergon-Public-Key'] = apiKey;
  }
  return headers;
}

/**
 * Override model in a JSON body if --model is specified.
 */
function maybeOverrideModel(bodyStr: string, modelOverride: string): string {
  if (!modelOverride) return bodyStr;
  try {
    const parsed = JSON.parse(bodyStr);
    if (parsed.model) {
      parsed.model = modelOverride;
      return JSON.stringify(parsed);
    }
  } catch {
    // not JSON or not parseable -- pass through unchanged
  }
  return bodyStr;
}

/**
 * Extract the model from a parsed request body (for logging).
 */
function extractModel(bodyStr: string): string {
  try {
    return JSON.parse(bodyStr).model ?? 'unknown';
  } catch {
    return 'unknown';
  }
}

async function serveAction(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const port = Number(args.options.port) || 8000;
  const relay = String(args.options.relay || ctx.config.baseUrl || 'https://relay.xergon.gg');
  const modelOverride = args.options.model ? String(args.options.model) : '';
  const apiKey = String(args.options.apiKey || ctx.config.apiKey || '');

  // Strip trailing slash and ensure /v1 base
  const relayBase = relay.replace(/\/+$/, '');

  const server = http.createServer(async (req: http.IncomingMessage, res: http.ServerResponse) => {
    const startMs = Date.now();
    const url = new URL(req.url || '/', `http://localhost:${port}`);
    const pathname = url.pathname;

    // ── Health check ───────────────────────────────────────────
    if (req.method === 'GET' && pathname === '/health') {
      writeJson(res, 200, { status: 'ok', relay: relayBase, uptime: process.uptime() });
      ctx.output.info(`GET ${pathname} 200 ${Date.now() - startMs}ms`);
      return;
    }

    // ── GET /v1/models ────────────────────────────────────────
    if (req.method === 'GET' && pathname === '/v1/models') {
      try {
        const upstream = `${relayBase}/v1/models${url.search}`;
        const upstreamRes = await fetch(upstream);
        const data = await upstreamRes.json();
        writeJson(res, upstreamRes.status, data);
        ctx.output.info(`GET ${pathname} ${upstreamRes.status} ${Date.now() - startMs}ms`);
      } catch (err) {
        writeJson(res, 502, { error: { message: 'Relay unreachable', type: 'proxy_error' } });
        ctx.output.writeError(`GET ${pathname} 502 ${Date.now() - startMs}ms -- ${err instanceof Error ? err.message : String(err)}`);
      }
      return;
    }

    // ── GET /v1/models/:id ────────────────────────────────────
    const modelsIdMatch = pathname.match(/^\/v1\/models\/([^/]+)$/);
    if (req.method === 'GET' && modelsIdMatch) {
      const modelId = decodeURIComponent(modelsIdMatch[1]);
      try {
        const upstream = `${relayBase}/v1/models/${encodeURIComponent(modelId)}`;
        const upstreamRes = await fetch(upstream);
        const data = await upstreamRes.json();
        writeJson(res, upstreamRes.status, data);
        ctx.output.info(`GET ${pathname} ${upstreamRes.status} ${Date.now() - startMs}ms`);
      } catch (err) {
        writeJson(res, 502, { error: { message: 'Relay unreachable', type: 'proxy_error' } });
        ctx.output.writeError(`GET ${pathname} 502 ${Date.now() - startMs}ms -- ${err instanceof Error ? err.message : String(err)}`);
      }
      return;
    }

    // ── POST /v1/chat/completions ─────────────────────────────
    if (req.method === 'POST' && pathname === '/v1/chat/completions') {
      try {
        const bodyStr = await readBody(req);
        const finalBody = maybeOverrideModel(bodyStr, modelOverride);
        const model = extractModel(finalBody);

        const xergonHeaders = buildXergonHeaders(apiKey);
        // Forward any upstream authorization from the client as well
        const clientAuth = req.headers['authorization'];
        if (clientAuth) {
          xergonHeaders['Authorization'] = clientAuth;
        }

        const upstream = `${relayBase}/v1/chat/completions`;
        const upstreamRes = await fetch(upstream, {
          method: 'POST',
          headers: xergonHeaders,
          body: finalBody,
        });

        // Check if the upstream wants to stream
        const contentType = upstreamRes.headers.get('content-type') || '';
        const isStream = contentType.includes('text/event-stream') ||
          (finalBody.includes('"stream":true') && contentType.includes('text/event'));

        if (isStream || contentType.includes('text/event-stream')) {
          // Stream SSE through to client
          res.writeHead(upstreamRes.status, {
            'Content-Type': 'text/event-stream',
            'Cache-Control': 'no-cache',
            'Connection': 'keep-alive',
          });

          if (upstreamRes.body) {
            const reader = upstreamRes.body.getReader();
            try {
              while (true) {
                const { done, value } = await reader.read();
                if (done) break;
                res.write(value);
              }
            } finally {
              reader.releaseLock();
            }
          }

          res.end();
          ctx.output.info(`POST ${pathname} ${upstreamRes.status} ${Date.now() - startMs}ms [stream] model=${model}`);
        } else {
          // Non-streaming JSON response
          const data = await upstreamRes.json();
          writeJson(res, upstreamRes.status, data);
          ctx.output.info(`POST ${pathname} ${upstreamRes.status} ${Date.now() - startMs}ms model=${model}`);
        }
      } catch (err) {
        writeJson(res, 502, { error: { message: 'Relay unreachable', type: 'proxy_error' } });
        ctx.output.writeError(`POST ${pathname} 502 ${Date.now() - startMs}ms -- ${err instanceof Error ? err.message : String(err)}`);
      }
      return;
    }

    // ── Catch-all: return OpenAI-style 404 ────────────────────
    writeJson(res, 404, {
      error: {
        message: `Unknown endpoint: ${req.method} ${pathname}`,
        type: 'invalid_request_error',
        code: 'not_found',
      },
    });
    ctx.output.info(`${req.method} ${pathname} 404 ${Date.now() - startMs}ms`);
  });

  // ── Graceful shutdown ────────────────────────────────────────
  const shutdown = (signal: string) => {
    ctx.output.info(`\nReceived ${signal}, shutting down...`);
    server.close(() => {
      ctx.output.success('Proxy server stopped.');
      process.exit(0);
    });
    // Force exit after 5s if connections linger
    setTimeout(() => process.exit(1), 5000);
  };

  process.on('SIGINT', () => shutdown('SIGINT'));
  process.on('SIGTERM', () => shutdown('SIGTERM'));

  // ── Start listening ──────────────────────────────────────────
  await new Promise<void>((resolve) => {
    server.listen(port, () => {
      resolve();
    });
  });

  ctx.output.write('');
  ctx.output.success(`Xergon proxy running on http://localhost:${port} (OpenAI-compatible)`);
  ctx.output.info(`Relay: ${relayBase}`);
  if (modelOverride) {
    ctx.output.info(`Model override: ${modelOverride}`);
  }
  if (apiKey) {
    ctx.output.info(`API key: ${apiKey.substring(0, 8)}...`);
  } else {
    ctx.output.warn('No API key set. Set XERGON_API_KEY or use --api-key.');
  }
  ctx.output.info('');
  ctx.output.info('Endpoints:');
  ctx.output.info(`  POST /v1/chat/completions  ->  ${relayBase}/v1/chat/completions`);
  ctx.output.info(`  GET  /v1/models             ->  ${relayBase}/v1/models`);
  ctx.output.info(`  GET  /v1/models/:id         ->  ${relayBase}/v1/models/:id`);
  ctx.output.info(`  GET  /health                ->  health check`);
  ctx.output.info('');
  ctx.output.info('Usage:');
  ctx.output.info(`  OPENAI_BASE_URL=http://localhost:${port} python my_app.py`);
  ctx.output.write('');

  // Keep the process alive -- the server is listening
  await new Promise(() => {}); // never resolves
}

export const serveCommand: Command = {
  name: 'serve',
  description: 'Start a local OpenAI-compatible proxy server',
  aliases: ['proxy'],
  options: serveOptions,
  action: serveAction,
};
