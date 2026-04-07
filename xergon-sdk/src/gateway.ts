/**
 * Xergon SDK -- API Gateway Module
 *
 * Unified entry point for routing, rate limiting, authentication,
 * load balancing, and caching across model endpoints.
 *
 * @example
 * ```ts
 * import { Gateway } from '@xergon/sdk';
 *
 * const gw = new Gateway({
 *   host: '0.0.0.0',
 *   port: 3000,
 *   routes: [
 *     {
 *       path: '/v1/chat/completions',
 *       methods: ['POST'],
 *       upstream: 'http://localhost:8080',
 *       modelId: 'llama-3.3-70b',
 *       timeout: 30000,
 *       retryPolicy: { maxRetries: 2, retryDelay: 500, retryOn: [502, 503], backoffMultiplier: 2 },
 *       cache: { enabled: true, ttlSeconds: 60, maxSize: 1000 },
 *     },
 *   ],
 *   rateLimit: { enabled: true, requestsPerMinute: 60, requestsPerHour: 1000, burstSize: 10, strategy: 'sliding-window' },
 *   auth: { enabled: true, type: 'api-key', apiKeyHeader: 'Authorization' },
 *   loadBalancer: { strategy: 'round-robin', healthCheckInterval: 15000, maxRetries: 3, retryDelay: 1000, failoverEnabled: true },
 *   logging: { enabled: true, level: 'info', format: 'json' },
 *   cors: { enabled: true, origins: ['*'], methods: ['GET', 'POST', 'PUT', 'DELETE', 'OPTIONS'], headers: ['Content-Type', 'Authorization'], maxAge: 86400 },
 * });
 *
 * await gw.start();
 * ```
 */

import * as http from 'node:http';
import * as https from 'node:https';
import * as url from 'node:url';

// ── Types ───────────────────────────────────────────────────────────

export interface GatewayConfig {
  host: string;
  port: number;
  routes: GatewayRoute[];
  rateLimit: RateLimitConfig;
  auth: GatewayAuthConfig;
  loadBalancer: LoadBalancerConfig;
  logging: GatewayLoggingConfig;
  cors: CorsConfig;
}

export interface GatewayRoute {
  path: string;
  methods: string[];
  upstream: string;
  modelId: string;
  timeout: number;
  retryPolicy: RetryPolicy;
  cache: CacheConfig;
}

export interface RateLimitConfig {
  enabled: boolean;
  requestsPerMinute: number;
  requestsPerHour: number;
  burstSize: number;
  strategy: 'sliding-window' | 'token-bucket' | 'fixed-window';
}

export interface GatewayAuthConfig {
  enabled: boolean;
  type: 'api-key' | 'jwt' | 'oauth2' | 'none';
  apiKeyHeader: string;
  jwtSecret?: string;
  jwtExpiry?: number;
}

export interface LoadBalancerConfig {
  strategy: 'round-robin' | 'least-connections' | 'weighted' | 'random';
  healthCheckInterval: number;
  maxRetries: number;
  retryDelay: number;
  failoverEnabled: boolean;
}

export interface RetryPolicy {
  maxRetries: number;
  retryDelay: number;
  retryOn: number[];
  backoffMultiplier: number;
}

export interface CacheConfig {
  enabled: boolean;
  ttlSeconds: number;
  maxSize: number;
  keyFn?: string;
}

export interface GatewayLoggingConfig {
  enabled: boolean;
  level: 'debug' | 'info' | 'warn' | 'error';
  format: 'json' | 'text';
}

export interface CorsConfig {
  enabled: boolean;
  origins: string[];
  methods: string[];
  headers: string[];
  maxAge: number;
}

export interface GatewayMetrics {
  totalRequests: number;
  activeRequests: number;
  totalErrors: number;
  avgLatencyMs: number;
  p50Latency: number;
  p99Latency: number;
  rateLimitHits: number;
  cacheHits: number;
  cacheMisses: number;
  upstreamLatencies: Record<string, number>;
  requestsPerMinute: number;
  startTime: number;
  uptimeSeconds: number;
}

export interface GatewayHealth {
  status: 'healthy' | 'degraded' | 'unhealthy';
  uptime: number;
  totalRoutes: number;
  healthyRoutes: number;
  activeConnections: number;
  errorRate: number;
  avgLatencyMs: number;
  version: string;
}

export type GatewayMiddlewareType = 'auth' | 'rate-limit' | 'cors' | 'logging' | 'cache' | 'timeout';

export type GatewayLogLevel = 'debug' | 'info' | 'warn' | 'error';

export interface GatewayLogEntry {
  timestamp: string;
  level: GatewayLogLevel;
  message: string;
  route?: string;
  method?: string;
  statusCode?: number;
  latencyMs?: number;
  clientIp?: string;
  [key: string]: unknown;
}

// ── Helpers ─────────────────────────────────────────────────────────

function now(): number {
  return Date.now();
}

function clone<T>(obj: T): T {
  return JSON.parse(JSON.stringify(obj));
}

function parseOrigin(origin: string): { protocol: string; hostname: string; port: string } {
  const parsed = new url.URL(origin);
  return {
    protocol: parsed.protocol,
    hostname: parsed.hostname,
    port: parsed.port || (parsed.protocol === 'https:' ? '443' : '80'),
  };
}

function matchRoute(path: string, routePath: string): boolean {
  // Support simple wildcards like /v1/*
  if (routePath.endsWith('/*')) {
    const prefix = routePath.slice(0, -2);
    return path === prefix || path.startsWith(prefix + '/');
  }
  return path === routePath;
}

function percentile(sorted: number[], p: number): number {
  if (sorted.length === 0) return 0;
  const idx = Math.ceil((p / 100) * sorted.length) - 1;
  return sorted[Math.max(0, Math.min(idx, sorted.length - 1))];
}

// ── Cache Store ─────────────────────────────────────────────────────

class LRUCache {
  private store = new Map<string, { value: Buffer; expiresAt: number }>();
  private maxSize: number;

  constructor(maxSize: number) {
    this.maxSize = maxSize;
  }

  get(key: string): Buffer | undefined {
    const entry = this.store.get(key);
    if (!entry) return undefined;
    if (now() > entry.expiresAt) {
      this.store.delete(key);
      return undefined;
    }
    // Move to end (most recently used)
    this.store.delete(key);
    this.store.set(key, entry);
    return entry.value;
  }

  set(key: string, value: Buffer, ttlMs: number): void {
    if (this.store.has(key)) {
      this.store.delete(key);
    } else if (this.store.size >= this.maxSize) {
      // Evict least recently used (first entry)
      const firstKey = this.store.keys().next().value;
      if (firstKey !== undefined) this.store.delete(firstKey);
    }
    this.store.set(key, { value, expiresAt: now() + ttlMs });
  }

  clear(): void {
    this.store.clear();
  }

  get size(): number {
    return this.store.size;
  }
}

// ── Rate Limiter ────────────────────────────────────────────────────

class RateLimiter {
  private windows = new Map<string, { timestamps: number[]; tokens: number; lastRefill: number }>();
  private config: RateLimitConfig;

  constructor(config: RateLimitConfig) {
    this.config = config;
  }

  check(clientId: string): { allowed: boolean; remaining: number; resetAt: number } {
    if (!this.config.enabled) {
      return { allowed: true, remaining: Infinity, resetAt: 0 };
    }

    const nowMs = now();
    let entry = this.windows.get(clientId);

    if (!entry) {
      entry = { timestamps: [], tokens: this.config.burstSize, lastRefill: nowMs };
      this.windows.set(clientId, entry);
    }

    switch (this.config.strategy) {
      case 'sliding-window':
        return this.slidingWindowCheck(entry, nowMs);
      case 'token-bucket':
        return this.tokenBucketCheck(entry, nowMs);
      case 'fixed-window':
        return this.fixedWindowCheck(entry, nowMs);
      default:
        return this.slidingWindowCheck(entry, nowMs);
    }
  }

  private slidingWindowCheck(entry: { timestamps: number[] }, nowMs: number) {
    const windowMs = 60_000;
    const cutoff = nowMs - windowMs;
    entry.timestamps = entry.timestamps.filter(ts => ts > cutoff);

    if (entry.timestamps.length >= this.config.requestsPerMinute) {
      const oldestInWindow = entry.timestamps[0] || nowMs;
      return { allowed: false, remaining: 0, resetAt: oldestInWindow + windowMs };
    }

    entry.timestamps.push(nowMs);
    return { allowed: true, remaining: this.config.requestsPerMinute - entry.timestamps.length, resetAt: nowMs + windowMs };
  }

  private tokenBucketCheck(entry: { tokens: number; lastRefill: number }, nowMs: number) {
    const elapsed = nowMs - entry.lastRefill;
    const refillRate = this.config.requestsPerMinute / 60_000; // tokens per ms
    entry.tokens = Math.min(this.config.burstSize, entry.tokens + elapsed * refillRate);
    entry.lastRefill = nowMs;

    if (entry.tokens < 1) {
      const refillTime = (1 - entry.tokens) / refillRate;
      return { allowed: false, remaining: 0, resetAt: nowMs + refillTime };
    }

    entry.tokens -= 1;
    return { allowed: true, remaining: Math.floor(entry.tokens), resetAt: nowMs + 60_000 };
  }

  private fixedWindowCheck(entry: { timestamps: number[] }, nowMs: number) {
    const windowMs = 60_000;
    const windowStart = Math.floor(nowMs / windowMs) * windowMs;
    entry.timestamps = entry.timestamps.filter(ts => ts >= windowStart);

    if (entry.timestamps.length >= this.config.requestsPerMinute) {
      return { allowed: false, remaining: 0, resetAt: windowStart + windowMs };
    }

    entry.timestamps.push(nowMs);
    return { allowed: true, remaining: this.config.requestsPerMinute - entry.timestamps.length, resetAt: windowStart + windowMs };
  }

  reset(clientId: string): void {
    this.windows.delete(clientId);
  }

  clear(): void {
    this.windows.clear();
  }
}

// ── Load Balancer ───────────────────────────────────────────────────

class LoadBalancer {
  private roundRobinIdx = 0;
  private connections = new Map<string, number>();
  private healthy = new Set<string>();
  private config: LoadBalancerConfig;
  private healthCheckTimer: ReturnType<typeof setInterval> | null = null;
  private onHealthChange: (endpoint: string, healthy: boolean) => void;

  constructor(config: LoadBalancerConfig, onHealthChange: (endpoint: string, healthy: boolean) => void) {
    this.config = config;
    this.onHealthChange = onHealthChange;
  }

  select(endpoints: string[]): string | null {
    if (endpoints.length === 0) return null;

    const healthyEndpoints = endpoints.filter(ep => this.healthy.has(ep));
    const pool = healthyEndpoints.length > 0 ? healthyEndpoints : endpoints;

    switch (this.config.strategy) {
      case 'round-robin':
        return pool[this.roundRobinIdx++ % pool.length];
      case 'random':
        return pool[Math.floor(Math.random() * pool.length)];
      case 'least-connections':
        return pool.reduce((min, ep) =>
          (this.connections.get(ep) ?? 0) < (this.connections.get(min) ?? 0) ? ep : min,
          pool[0],
        );
      case 'weighted':
        // Simple weighted round-robin (equal weights unless configured)
        return pool[this.roundRobinIdx++ % pool.length];
      default:
        return pool[0];
    }
  }

  trackConnection(endpoint: string, delta: 1 | -1): void {
    const current = this.connections.get(endpoint) ?? 0;
    this.connections.set(endpoint, Math.max(0, current + delta));
  }

  startHealthChecks(endpoints: string[]): void {
    if (this.healthCheckTimer) clearInterval(this.healthCheckTimer);

    // Mark all as healthy initially
    for (const ep of endpoints) {
      this.healthy.add(ep);
    }

    this.healthCheckTimer = setInterval(() => {
      for (const ep of endpoints) {
        this.checkHealth(ep);
      }
    }, this.config.healthCheckInterval);

    // Don't prevent process exit
    if (this.healthCheckTimer.unref) {
      this.healthCheckTimer.unref();
    }
  }

  private async checkHealth(endpoint: string): Promise<void> {
    try {
      const parsed = parseOrigin(endpoint);
      const transport = parsed.protocol === 'https:' ? https : http;
      const start = now();

      await new Promise<void>((resolve, reject) => {
        const req = transport.request(
          { hostname: parsed.hostname, port: parseInt(parsed.port), path: '/health', method: 'GET', timeout: 5000 },
          (res) => {
            res.resume();
            if (res.statusCode && res.statusCode < 500) {
              resolve();
            } else {
              reject(new Error(`Health check returned ${res.statusCode}`));
            }
          },
        );
        req.on('error', reject);
        req.on('timeout', () => { req.destroy(); reject(new Error('Health check timeout')); });
        req.end();
      });

      const wasUnhealthy = !this.healthy.has(endpoint);
      this.healthy.add(endpoint);
      if (wasUnhealthy) this.onHealthChange(endpoint, true);
    } catch {
      const wasHealthy = this.healthy.has(endpoint);
      this.healthy.delete(endpoint);
      if (wasHealthy) this.onHealthChange(endpoint, false);
    }
  }

  isHealthy(endpoint: string): boolean {
    return this.healthy.has(endpoint);
  }

  getHealthyCount(): number {
    return this.healthy.size;
  }

  stop(): void {
    if (this.healthCheckTimer) {
      clearInterval(this.healthCheckTimer);
      this.healthCheckTimer = null;
    }
  }
}

// ── Gateway Class ───────────────────────────────────────────────────

const GATEWAY_VERSION = '0.1.0';

export class Gateway {
  private config: GatewayConfig;
  private server: http.Server | null = null;
  private routes: Map<string, GatewayRoute> = new Map();
  private rateLimiter: RateLimiter;
  private loadBalancer: LoadBalancer;
  private caches = new Map<string, LRUCache>();
  private metrics: {
    totalRequests: number;
    activeRequests: number;
    totalErrors: number;
    latencies: number[];
    rateLimitHits: number;
    cacheHits: number;
    cacheMisses: number;
    upstreamLatencies: Map<string, number[]>;
    requestTimestamps: number[];
    startTime: number;
  };
  private logs: GatewayLogEntry[] = [];
  private logHistoryMax = 10_000;
  private stopping = false;

  constructor(config: GatewayConfig) {
    this.config = clone(config);

    // Validate config
    if (!this.config.host) this.config.host = '0.0.0.0';
    if (!this.config.port) this.config.port = 3000;

    // Initialize routes
    for (const route of this.config.routes) {
      this.routes.set(route.path, clone(route));
    }

    // Initialize rate limiter
    this.rateLimiter = new RateLimiter(this.config.rateLimit);

    // Initialize load balancer
    this.loadBalancer = new LoadBalancer(this.config.loadBalancer, (endpoint, healthy) => {
      this.emitLog(healthy ? 'info' : 'warn', `Endpoint ${endpoint} is now ${healthy ? 'healthy' : 'unhealthy'}`);
    });

    // Initialize metrics
    this.metrics = {
      totalRequests: 0,
      activeRequests: 0,
      totalErrors: 0,
      latencies: [],
      rateLimitHits: 0,
      cacheHits: 0,
      cacheMisses: 0,
      upstreamLatencies: new Map(),
      requestTimestamps: [],
      startTime: now(),
    };

    // Start health checks for upstreams
    const upstreams = [...new Set([...this.config.routes.map(r => r.upstream)])];
    if (upstreams.length > 0) {
      this.loadBalancer.startHealthChecks(upstreams);
    }
  }

  // ── Server lifecycle ───────────────────────────────────────────

  async start(): Promise<void> {
    return new Promise((resolve, reject) => {
      this.server = http.createServer(this.handleRequest.bind(this));

      this.server.on('error', (err) => {
        this.emitLog('error', `Server error: ${err.message}`);
        if (!this.stopping) reject(err);
      });

      this.server.listen(this.config.port, this.config.host, () => {
        this.emitLog('info', `Gateway listening on ${this.config.host}:${this.config.port}`);
        this.emitLog('info', `${this.routes.size} route(s) configured`);
        resolve();
      });
    });
  }

  async stop(): Promise<void> {
    this.stopping = true;
    this.loadBalancer.stop();

    return new Promise((resolve, reject) => {
      if (!this.server) {
        resolve();
        return;
      }

      this.server.close((err) => {
        if (err && !this.stopping) {
          reject(err);
          return;
        }
        this.server = null;
        this.emitLog('info', 'Gateway stopped');
        resolve();
      });

      // Force close after 5s
      setTimeout(() => {
        if (this.server) {
          this.server.closeAllConnections?.();
          this.server = null;
          resolve();
        }
      }, 5000).unref();
    });
  }

  // ── Route management ───────────────────────────────────────────

  addRoute(route: GatewayRoute): void {
    this.routes.set(route.path, clone(route));
    this.caches.delete(route.path);
    this.emitLog('info', `Route added: ${route.path} -> ${route.upstream} (${route.modelId})`);
  }

  removeRoute(path: string): boolean {
    const removed = this.routes.delete(path);
    this.caches.delete(path);
    if (removed) {
      this.emitLog('info', `Route removed: ${path}`);
    }
    return removed;
  }

  updateRoute(path: string, updates: Partial<GatewayRoute>): boolean {
    const existing = this.routes.get(path);
    if (!existing) return false;

    const updated = { ...clone(existing), ...updates };
    this.routes.set(path, updated);
    this.caches.delete(path);
    this.emitLog('info', `Route updated: ${path}`);
    return true;
  }

  getRoutes(): GatewayRoute[] {
    return [...this.routes.values()].map(r => clone(r));
  }

  // ── Metrics ────────────────────────────────────────────────────

  getMetrics(): GatewayMetrics {
    const sortedLatencies = [...this.metrics.latencies].sort((a, b) => a - b);
    // Keep only last 10k latencies for percentile calc
    if (this.metrics.latencies.length > 10_000) {
      this.metrics.latencies = sortedLatencies.slice(-10_000);
    }

    const avgLatency = sortedLatencies.length > 0
      ? sortedLatencies.reduce((s, v) => s + v, 0) / sortedLatencies.length
      : 0;

    // Rolling requests per minute
    const cutoff = now() - 60_000;
    this.metrics.requestTimestamps = this.metrics.requestTimestamps.filter(t => t > cutoff);

    const upstreamLatencies: Record<string, number> = {};
    for (const [route, latencies] of this.metrics.upstreamLatencies) {
      if (latencies.length > 0) {
        upstreamLatencies[route] = latencies.reduce((s, v) => s + v, 0) / latencies.length;
      }
    }

    return {
      totalRequests: this.metrics.totalRequests,
      activeRequests: this.metrics.activeRequests,
      totalErrors: this.metrics.totalErrors,
      avgLatencyMs: Math.round(avgLatency),
      p50Latency: Math.round(percentile(sortedLatencies, 50)),
      p99Latency: Math.round(percentile(sortedLatencies, 99)),
      rateLimitHits: this.metrics.rateLimitHits,
      cacheHits: this.metrics.cacheHits,
      cacheMisses: this.metrics.cacheMisses,
      upstreamLatencies,
      requestsPerMinute: this.metrics.requestTimestamps.length,
      startTime: this.metrics.startTime,
      uptimeSeconds: Math.round((now() - this.metrics.startTime) / 1000),
    };
  }

  // ── Health ─────────────────────────────────────────────────────

  getHealth(): GatewayHealth {
    const m = this.getMetrics();
    const errorRate = m.totalRequests > 0 ? m.totalErrors / m.totalRequests : 0;
    const healthyRoutes = [...this.routes.values()].filter(r =>
      this.loadBalancer.isHealthy(r.upstream),
    ).length;

    let status: GatewayHealth['status'] = 'healthy';
    if (errorRate > 0.5 || healthyRoutes === 0) status = 'unhealthy';
    else if (errorRate > 0.1 || healthyRoutes < this.routes.size) status = 'degraded';

    return {
      status,
      uptime: m.uptimeSeconds,
      totalRoutes: this.routes.size,
      healthyRoutes,
      activeConnections: m.activeRequests,
      errorRate: Math.round(errorRate * 10000) / 100,
      avgLatencyMs: m.avgLatencyMs,
      version: GATEWAY_VERSION,
    };
  }

  // ── Config reload ──────────────────────────────────────────────

  reloadConfig(config: GatewayConfig): void {
    this.config = clone(config);
    if (!this.config.host) this.config.host = '0.0.0.0';
    if (!this.config.port) this.config.port = 3000;

    // Rebuild routes
    this.routes.clear();
    this.caches.clear();
    for (const route of this.config.routes) {
      this.routes.set(route.path, clone(route));
    }

    // Rebuild rate limiter
    this.rateLimiter = new RateLimiter(this.config.rateLimit);

    this.emitLog('info', 'Configuration reloaded');
  }

  // ── Middleware factory ─────────────────────────────────────────

  createMiddleware(type: GatewayMiddlewareType): (req: http.IncomingMessage, res: http.ServerResponse, next: () => void) => void {
    switch (type) {
      case 'auth':
        return this.authMiddleware();
      case 'rate-limit':
        return this.rateLimitMiddleware();
      case 'cors':
        return this.corsMiddleware();
      case 'logging':
        return this.loggingMiddleware();
      case 'cache':
        return this.cacheMiddleware();
      case 'timeout':
        return this.timeoutMiddleware();
      default:
        return (_req, _res, next) => next();
    }
  }

  // ── Internal: Middleware implementations ───────────────────────

  private authMiddleware() {
    return (req: http.IncomingMessage, res: http.ServerResponse, next: () => void) => {
      if (!this.config.auth.enabled || this.config.auth.type === 'none') {
        next();
        return;
      }

      const header = this.config.auth.apiKeyHeader;
      const apiKey = req.headers[header.toLowerCase()] as string | undefined;

      if (!apiKey) {
        res.writeHead(401, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ error: 'Missing authentication credentials' }));
        return;
      }

      switch (this.config.auth.type) {
        case 'api-key': {
          // Validate API key format (Bearer <key>)
          if (!apiKey.startsWith('Bearer ')) {
            res.writeHead(401, { 'Content-Type': 'application/json' });
            res.end(JSON.stringify({ error: 'Invalid API key format. Use: Bearer <key>' }));
            return;
          }
          next();
          break;
        }
        case 'jwt': {
          // Basic JWT validation placeholder
          const token = apiKey.replace('Bearer ', '');
          if (!token || token.split('.').length !== 3) {
            res.writeHead(401, { 'Content-Type': 'application/json' });
            res.end(JSON.stringify({ error: 'Invalid JWT token' }));
            return;
          }
          next();
          break;
        }
        default:
          next();
      }
    };
  }

  private rateLimitMiddleware() {
    return (req: http.IncomingMessage, res: http.ServerResponse, next: () => void) => {
      if (!this.config.rateLimit.enabled) {
        next();
        return;
      }

      const clientId = req.socket.remoteAddress || 'unknown';
      const result = this.rateLimiter.check(clientId);

      // Set rate limit headers
      res.setHeader('X-RateLimit-Remaining', String(result.remaining));
      if (result.resetAt > 0) {
        res.setHeader('X-RateLimit-Reset', String(Math.ceil(result.resetAt / 1000)));
      }

      if (!result.allowed) {
        this.metrics.rateLimitHits++;
        res.writeHead(429, { 'Content-Type': 'application/json', 'Retry-After': '60' });
        res.end(JSON.stringify({ error: 'Rate limit exceeded', retryAfter: 60 }));
        return;
      }

      next();
    };
  }

  private corsMiddleware() {
    return (req: http.IncomingMessage, res: http.ServerResponse, next: () => void) => {
      if (!this.config.cors.enabled) {
        next();
        return;
      }

      const origin = req.headers.origin;
      const allowedOrigins = this.config.cors.origins;

      if (origin && (allowedOrigins.includes('*') || allowedOrigins.includes(origin))) {
        res.setHeader('Access-Control-Allow-Origin', origin === '*' ? '*' : origin);
      }

      res.setHeader('Access-Control-Allow-Methods', this.config.cors.methods.join(', '));
      res.setHeader('Access-Control-Allow-Headers', this.config.cors.headers.join(', '));
      res.setHeader('Access-Control-Max-Age', String(this.config.cors.maxAge));

      if (req.method === 'OPTIONS') {
        res.writeHead(204);
        res.end();
        return;
      }

      next();
    };
  }

  private loggingMiddleware() {
    return (req: http.IncomingMessage, res: http.ServerResponse, next: () => void) => {
      if (!this.config.logging.enabled) {
        next();
        return;
      }

      const start = now();
      const originalEnd = res.end.bind(res);

      res.end = (chunk?: any, encoding?: any, cb?: any) => {
        const latency = now() - start;
        this.emitLog('info', `${req.method} ${req.url} -> ${res.statusCode}`, {
          method: req.method,
          route: req.url,
          statusCode: res.statusCode,
          latencyMs: latency,
          clientIp: req.socket.remoteAddress,
        });
        return originalEnd(chunk, encoding, cb);
      };

      next();
    };
  }

  private cacheMiddleware() {
    return (req: http.IncomingMessage, res: http.ServerResponse, next: () => void) => {
      // Cache is applied per-route in the proxy handler
      next();
    };
  }

  private timeoutMiddleware() {
    return (_req: http.IncomingMessage, res: http.ServerResponse, next: () => void) => {
      const defaultTimeout = 30_000;
      res.setTimeout(defaultTimeout, () => {
        if (!res.headersSent) {
          res.writeHead(504, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({ error: 'Gateway timeout' }));
        }
      });
      next();
    };
  }

  // ── Internal: Request handler ─────────────────────────────────

  private async handleRequest(req: http.IncomingMessage, res: http.ServerResponse): Promise<void> {
    const startMs = now();
    const method = req.method?.toUpperCase() || 'GET';
    const reqPath = req.url || '/';

    this.metrics.totalRequests++;
    this.metrics.activeRequests++;
    this.metrics.requestTimestamps.push(now());

    try {
      // Run middleware chain
      await this.runMiddleware(req, res);

      if (res.headersSent) return;

      // Match route
      const route = this.findRoute(reqPath, method);

      if (!route) {
        res.writeHead(404, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ error: 'Not found', path: reqPath }));
        return;
      }

      // Check cache for GET requests
      if (method === 'GET' && route.cache.enabled) {
        const cacheKey = this.buildCacheKey(req, route);
        const cached = this.getCache(route.path, cacheKey);
        if (cached) {
          this.metrics.cacheHits++;
          res.writeHead(200, {
            'Content-Type': 'application/json',
            'X-Cache': 'HIT',
          });
          res.end(cached);
          return;
        }
        this.metrics.cacheMisses++;
      }

      // Proxy request to upstream
      const proxyResult = await this.proxyRequest(req, res, route, method, reqPath, startMs);

      if (!proxyResult && !res.headersSent) {
        res.writeHead(502, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ error: 'Bad Gateway: upstream unavailable' }));
      }
    } catch (err) {
      this.metrics.totalErrors++;
      const message = err instanceof Error ? err.message : String(err);
      this.emitLog('error', `Request error: ${message}`, { route: reqPath, method });

      if (!res.headersSent) {
        res.writeHead(500, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ error: 'Internal gateway error', message }));
      }
    } finally {
      this.metrics.activeRequests--;
      const latency = now() - startMs;
      this.metrics.latencies.push(latency);
    }
  }

  private async runMiddleware(req: http.IncomingMessage, res: http.ServerResponse): Promise<void> {
    const middlewareTypes: GatewayMiddlewareType[] = ['cors', 'logging', 'auth', 'rate-limit', 'timeout'];
    for (const type of middlewareTypes) {
      await new Promise<void>((resolve) => {
        const mw = this.createMiddleware(type);
        mw(req, res, resolve);
      });
      if (res.headersSent) return;
    }
  }

  private findRoute(reqPath: string, method: string): GatewayRoute | undefined {
    for (const [routePath, route] of this.routes) {
      if (matchRoute(reqPath, routePath) && route.methods.includes(method)) {
        return route;
      }
    }
    return undefined;
  }

  private async proxyRequest(
    req: http.IncomingMessage,
    res: http.ServerResponse,
    route: GatewayRoute,
    method: string,
    reqPath: string,
    startMs: number,
  ): Promise<boolean> {
    const upstreamUrl = route.upstream.replace(/\/$/, '') + reqPath;
    const parsed = parseOrigin(upstreamUrl);

    // Collect request body
    const bodyChunks: Buffer[] = [];
    for await (const chunk of req) {
      bodyChunks.push(chunk);
    }
    const body = Buffer.concat(bodyChunks);

    // Select upstream via load balancer
    const selectedUpstream = this.loadBalancer.select([route.upstream]);
    if (!selectedUpstream) return false;

    this.loadBalancer.trackConnection(selectedUpstream, 1);

    try {
      return await this.attemptProxy(parsed, method, req, res, body, route, startMs);
    } finally {
      this.loadBalancer.trackConnection(selectedUpstream, -1);
    }
  }

  private attemptProxy(
    parsed: { protocol: string; hostname: string; port: string },
    method: string,
    req: http.IncomingMessage,
    res: http.ServerResponse,
    body: Buffer,
    route: GatewayRoute,
    startMs: number,
    attempt: number = 0,
  ): Promise<boolean> {
    return new Promise<boolean>((resolve) => {
      const transport = parsed.protocol === 'https:' ? https : http;
      const upstreamStart = now();

      const proxyReq = transport.request(
        {
          hostname: parsed.hostname,
          port: parseInt(parsed.port),
          path: req.url || '/',
          method,
          headers: {
            ...req.headers,
            host: parsed.hostname,
            'x-forwarded-for': req.socket.remoteAddress || '',
            'x-gateway-attempt': String(attempt),
          },
          timeout: route.timeout,
        },
        (proxyRes) => {
          const chunks: Buffer[] = [];
          proxyRes.on('data', (chunk: Buffer) => chunks.push(chunk));
          proxyRes.on('end', () => {
            const upstreamLatency = now() - upstreamStart;

            // Track upstream latency
            if (!this.metrics.upstreamLatencies.has(route.path)) {
              this.metrics.upstreamLatencies.set(route.path, []);
            }
            const latencies = this.metrics.upstreamLatencies.get(route.path)!;
            latencies.push(upstreamLatency);
            if (latencies.length > 1000) latencies.splice(0, latencies.length - 1000);

            // Check for retryable status
            if (route.retryPolicy.retryOn.includes(proxyRes.statusCode || 0) &&
                attempt < route.retryPolicy.maxRetries) {
              const delay = route.retryPolicy.retryDelay * Math.pow(route.retryPolicy.backoffMultiplier, attempt);
              this.emitLog('warn', `Retrying ${req.url} (attempt ${attempt + 1}, status ${proxyRes.statusCode})`);
              setTimeout(() => {
                this.attemptProxy(parsed, method, req, res, body, route, startMs, attempt + 1)
                  .then(resolve);
              }, delay);
              return;
            }

            // Set response headers
            const headers: Record<string, string> = {};
            for (const [key, value] of Object.entries(proxyRes.headers)) {
              if (value && typeof value === 'string') {
                headers[key] = value;
              } else if (Array.isArray(value)) {
                headers[key] = value.join(', ');
              }
            }
            headers['x-gateway-latency'] = String(now() - startMs);
            headers['x-upstream-latency'] = String(upstreamLatency);

            const fullBody = Buffer.concat(chunks);

            // Cache successful GET responses
            if (method === 'GET' && route.cache.enabled &&
                proxyRes.statusCode && proxyRes.statusCode >= 200 && proxyRes.statusCode < 300) {
              const cacheKey = this.buildCacheKey(req, route);
              this.setCache(route.path, cacheKey, fullBody, route.cache.ttlSeconds * 1000);
              headers['X-Cache'] = 'MISS';
            }

            res.writeHead(proxyRes.statusCode || 502, headers);
            res.end(fullBody);
            resolve(true);
          });
        },
      );

      proxyReq.on('error', (err) => {
        this.metrics.totalErrors++;
        this.emitLog('error', `Proxy error: ${err.message}`);

        if (attempt < route.retryPolicy.maxRetries) {
          const delay = route.retryPolicy.retryDelay * Math.pow(route.retryPolicy.backoffMultiplier, attempt);
          setTimeout(() => {
            this.attemptProxy(parsed, method, req, res, body, route, startMs, attempt + 1)
              .then(resolve);
          }, delay);
        } else {
          resolve(false);
        }
      });

      proxyReq.on('timeout', () => {
        proxyReq.destroy();
        this.metrics.totalErrors++;
        this.emitLog('error', `Proxy timeout to ${parsed.hostname}:${parsed.port}`);

        if (attempt < route.retryPolicy.maxRetries) {
          const delay = route.retryPolicy.retryDelay * Math.pow(route.retryPolicy.backoffMultiplier, attempt);
          setTimeout(() => {
            this.attemptProxy(parsed, method, req, res, body, route, startMs, attempt + 1)
              .then(resolve);
          }, delay);
        } else {
          resolve(false);
        }
      });

      if (body.length > 0) {
        proxyReq.write(body);
      }
      proxyReq.end();
    });
  }

  // ── Internal: Cache helpers ───────────────────────────────────

  private getRouteCache(routePath: string): LRUCache {
    let cache = this.caches.get(routePath);
    if (!cache) {
      const route = this.routes.get(routePath);
      cache = new LRUCache(route?.cache.maxSize || 1000);
      this.caches.set(routePath, cache);
    }
    return cache;
  }

  private getCache(routePath: string, key: string): Buffer | undefined {
    return this.getRouteCache(routePath).get(key);
  }

  private setCache(routePath: string, key: string, value: Buffer, ttlMs: number): void {
    this.getRouteCache(routePath).set(key, value, ttlMs);
  }

  private buildCacheKey(req: http.IncomingMessage, route: GatewayRoute): string {
    if (route.cache.keyFn) {
      return route.cache.keyFn
        .replace('{method}', req.method || 'GET')
        .replace('{path}', req.url || '/')
        .replace('{modelId}', route.modelId);
    }
    return `${req.method || 'GET'}:${req.url || '/'}:${route.modelId}`;
  }

  // ── Internal: Logging ─────────────────────────────────────────

  private emitLog(level: GatewayLogLevel, message: string, meta?: Record<string, unknown>): void {
    const entry: GatewayLogEntry = {
      timestamp: new Date().toISOString(),
      level,
      message,
      ...meta,
    };

    this.logs.push(entry);
    if (this.logs.length > this.logHistoryMax) {
      this.logs.splice(0, this.logs.length - this.logHistoryMax);
    }

    // Only print if logging enabled and level passes
    if (this.config.logging.enabled) {
      const levels: GatewayLogLevel[] = ['debug', 'info', 'warn', 'error'];
      if (levels.indexOf(level) >= levels.indexOf(this.config.logging.level)) {
        const ts = entry.timestamp.slice(11, 23);
        const lvl = level.toUpperCase().padEnd(5);
        if (this.config.logging.format === 'json') {
          process.stderr.write(JSON.stringify(entry) + '\n');
        } else {
          process.stderr.write(`${ts} ${lvl} ${message}\n`);
        }
      }
    }
  }

  getLogs(level?: GatewayLogLevel): GatewayLogEntry[] {
    if (level) return this.logs.filter(l => l.level === level);
    return [...this.logs];
  }

  clearLogs(): void {
    this.logs = [];
  }

  resetMetrics(): void {
    this.metrics.totalRequests = 0;
    this.metrics.activeRequests = 0;
    this.metrics.totalErrors = 0;
    this.metrics.latencies = [];
    this.metrics.rateLimitHits = 0;
    this.metrics.cacheHits = 0;
    this.metrics.cacheMisses = 0;
    this.metrics.upstreamLatencies.clear();
    this.metrics.requestTimestamps = [];
    this.metrics.startTime = now();
  }
}
