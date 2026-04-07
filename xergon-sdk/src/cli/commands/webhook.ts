/**
 * CLI command: webhook
 *
 * Manage Xergon SDK webhooks for event integration.
 *
 * Usage:
 *   xergon webhook list                   -- List webhook subscriptions
 *   xergon webhook create                 -- Create webhook subscription
 *   xergon webhook delete <id>            -- Delete webhook
 *   xergon webhook test <id>              -- Test webhook delivery
 *   xergon webhook history <id>           -- Show delivery history
 *   xergon webhook events                 -- List available event types
 *   xergon webhook retry <delivery-id>    -- Retry failed delivery
 */

import type { Command, CommandOption, ParsedArgs, CLIContext } from '../mod';
import * as crypto from 'node:crypto';

// ── Types ──────────────────────────────────────────────────────────

type WebhookStatus = 'active' | 'disabled' | 'pending';
type DeliveryStatus = 'success' | 'failure' | 'pending' | 'retrying';
type EventCategory = 'inference' | 'model' | 'billing' | 'provider' | 'org' | 'system';

interface WebhookSubscription {
  id: string;
  url: string;
  events: string[];
  description: string | null;
  secret: string;
  status: WebhookStatus;
  createdAt: string;
  updatedAt: string;
  lastDelivery: WebhookDelivery | null;
  totalDeliveries: number;
  failureCount: number;
  successRate: number;
}

interface WebhookDelivery {
  id: string;
  webhookId: string;
  event: string;
  payload: Record<string, any>;
  statusCode: number;
  success: boolean;
  status: DeliveryStatus;
  duration: number;
  response: string | null;
  errorMessage: string | null;
  retries: number;
  maxRetries: number;
  timestamp: string;
  nextRetry: string | null;
  signature: string;
}

interface WebhookEvent {
  name: string;
  category: EventCategory;
  description: string;
  payloadExample: Record<string, any>;
}

interface CreateWebhookResult {
  success: boolean;
  id: string;
  url: string;
  events: string[];
  secret: string;
  message: string;
}

interface DeleteWebhookResult {
  success: boolean;
  id: string;
  message: string;
}

interface TestDeliveryResult {
  success: boolean;
  deliveryId: string;
  event: string;
  statusCode: number;
  duration: number;
  response: string | null;
  message: string;
}

interface RetryResult {
  success: boolean;
  deliveryId: string;
  event: string;
  statusCode: number;
  duration: number;
  message: string;
}

interface SignatureVerifyResult {
  valid: boolean;
  message: string;
}

// ── Constants ──────────────────────────────────────────────────────

const WEBHOOK_EVENT_TYPES: WebhookEvent[] = [
  {
    name: 'inference.complete',
    category: 'inference',
    description: 'An inference request completed successfully',
    payloadExample: {
      requestId: 'req_abc123',
      model: 'llama-3.3-70b',
      provider: 'node-0x123',
      duration: 1250,
      tokens: { prompt: 50, completion: 200 },
      cost: 0.0025,
    },
  },
  {
    name: 'inference.error',
    category: 'inference',
    description: 'An inference request failed',
    payloadExample: {
      requestId: 'req_def456',
      model: 'llama-3.3-70b',
      error: 'Provider timeout',
      statusCode: 504,
    },
  },
  {
    name: 'inference.stream.chunk',
    category: 'inference',
    description: 'A streaming inference chunk was produced',
    payloadExample: {
      requestId: 'req_ghi789',
      chunk: 42,
      token: 'Hello',
      finished: false,
    },
  },
  {
    name: 'model.deployed',
    category: 'model',
    description: 'A model was deployed to the network',
    payloadExample: {
      modelId: 'model_abc',
      name: 'llama-3.3-70b',
      version: 'v2',
      nodeCount: 5,
      deployedBy: 'admin@acme.com',
    },
  },
  {
    name: 'model.undeployed',
    category: 'model',
    description: 'A model was removed from the network',
    payloadExample: {
      modelId: 'model_def',
      name: 'mistral-7b',
      reason: 'maintenance',
      removedBy: 'admin@acme.com',
    },
  },
  {
    name: 'model.updated',
    category: 'model',
    description: 'A model configuration was updated',
    payloadExample: {
      modelId: 'model_ghi',
      name: 'llama-3.3-70b',
      changes: ['maxTokens', 'temperature'],
      updatedBy: 'admin@acme.com',
    },
  },
  {
    name: 'billing.paid',
    category: 'billing',
    description: 'A billing payment was processed',
    payloadExample: {
      invoiceId: 'inv_001',
      amount: 25.50,
      currency: 'USD',
      method: 'card_****4242',
      period: '2025-06',
    },
  },
  {
    name: 'billing.failed',
    category: 'billing',
    description: 'A billing payment failed',
    payloadExample: {
      invoiceId: 'inv_002',
      amount: 15.00,
      currency: 'USD',
      error: 'Card declined',
      retryDate: '2025-06-02T10:00:00Z',
    },
  },
  {
    name: 'billing.limit_reached',
    category: 'billing',
    description: 'Spending limit was reached',
    payloadExample: {
      limit: 100.00,
      spent: 100.00,
      currency: 'USD',
      period: 'monthly',
    },
  },
  {
    name: 'provider.registered',
    category: 'provider',
    description: 'A new compute provider registered',
    payloadExample: {
      providerId: 'prov_xyz',
      name: 'GPU Farm Alpha',
      gpuType: 'A100',
      gpuCount: 8,
      region: 'us-west-2',
    },
  },
  {
    name: 'provider.deregistered',
    category: 'provider',
    description: 'A compute provider deregistered',
    payloadExample: {
      providerId: 'prov_abc',
      name: 'GPU Farm Beta',
      reason: 'maintenance',
    },
  },
  {
    name: 'provider.status_change',
    category: 'provider',
    description: 'A provider status changed',
    payloadExample: {
      providerId: 'prov_def',
      previousStatus: 'offline',
      newStatus: 'online',
      gpuCount: 4,
    },
  },
  {
    name: 'org.member_added',
    category: 'org',
    description: 'A new member joined the organization',
    payloadExample: {
      orgId: 'org_abc',
      email: 'newuser@acme.com',
      role: 'Member',
      invitedBy: 'admin@acme.com',
    },
  },
  {
    name: 'org.member_removed',
    category: 'org',
    description: 'A member was removed from the organization',
    payloadExample: {
      orgId: 'org_abc',
      email: 'olduser@acme.com',
      removedBy: 'admin@acme.com',
    },
  },
  {
    name: 'org.settings_changed',
    category: 'org',
    description: 'Organization settings were changed',
    payloadExample: {
      orgId: 'org_abc',
      settings: ['require2FA', 'defaultRole'],
      changedBy: 'admin@acme.com',
    },
  },
  {
    name: 'system.maintenance',
    category: 'system',
    description: 'System maintenance scheduled or started',
    payloadExample: {
      type: 'scheduled',
      startAt: '2025-06-15T02:00:00Z',
      endAt: '2025-06-15T04:00:00Z',
      affected: ['inference', 'deploy'],
    },
  },
  {
    name: 'system.alert',
    category: 'system',
    description: 'System alert triggered',
    payloadExample: {
      severity: 'warning',
      message: 'High latency detected in us-east-1',
      metric: 'p99_latency',
      value: 5200,
      threshold: 3000,
    },
  },
];

const EVENT_CATEGORIES: EventCategory[] = ['inference', 'model', 'billing', 'provider', 'org', 'system'];

const CATEGORY_LABELS: Record<EventCategory, string> = {
  inference: 'Inference',
  model: 'Models',
  billing: 'Billing',
  provider: 'Providers',
  org: 'Organization',
  system: 'System',
};

const MAX_RETRIES = 3;
const RETRY_DELAYS_MS = [1000, 5000, 30000]; // exponential-ish backoff

// ── Helpers ────────────────────────────────────────────────────────

function generateWebhookId(): string {
  return `wh_${crypto.randomBytes(12).toString('hex')}`;
}

function generateDeliveryId(): string {
  return `dlv_${crypto.randomBytes(16).toString('hex')}`;
}

function generateWebhookSecret(): string {
  return `whsec_${crypto.randomBytes(32).toString('hex')}`;
}

function computeSignature(payload: string, secret: string, timestamp: number): string {
  const data = `${timestamp}.${payload}`;
  return 'sha256=' + crypto.createHmac('sha256', secret).update(data).digest('hex');
}

function verifySignature(payload: string, secret: string, signature: string, timestamp: number, toleranceMs: number = 300_000): SignatureVerifyResult {
  // Check timestamp freshness
  const now = Date.now();
  if (Math.abs(now - timestamp) > toleranceMs) {
    return { valid: false, message: 'Timestamp outside tolerance window' };
  }

  const expected = computeSignature(payload, secret, timestamp);
  // timingSafeEqual requires same-length buffers; length mismatch means invalid
  if (signature.length !== expected.length) {
    return { valid: false, message: 'Signature mismatch' };
  }
  if (!crypto.timingSafeEqual(Buffer.from(signature), Buffer.from(expected))) {
    return { valid: false, message: 'Signature mismatch' };
  }

  return { valid: true, message: 'Signature valid' };
}

function formatDeliveryStatus(status: DeliveryStatus): string {
  const colors: Record<DeliveryStatus, string> = {
    success: 'green',
    failure: 'red',
    pending: 'yellow',
    retrying: 'yellow',
  };
  return status.toUpperCase();
}

function formatRelativeTime(iso: string): string {
  try {
    const d = new Date(iso);
    if (isNaN(d.getTime())) return 'unknown';
    const now = Date.now();
    const diff = now - d.getTime();
    if (diff < 0) return 'just now';
    const seconds = Math.floor(diff / 1000);
    const minutes = Math.floor(seconds / 60);
    const hours = Math.floor(minutes / 60);
    const days = Math.floor(hours / 24);
    if (days > 0) return `${days}d ago`;
    if (hours > 0) return `${hours}h ago`;
    if (minutes > 0) return `${minutes}m ago`;
    return `${seconds}s ago`;
  } catch {
    return 'unknown';
  }
}

function maskSecret(secret: string): string {
  if (secret.length <= 12) return '••••••••••••';
  return secret.substring(0, 8) + '••••' + secret.substring(secret.length - 4);
}

function parseEvents(eventStr: string): string[] {
  return eventStr
    .split(',')
    .map(e => e.trim())
    .filter(e => e.length > 0);
}

function isValidEvent(eventName: string): boolean {
  return WEBHOOK_EVENT_TYPES.some(e => e.name === eventName);
}

function getEventsByCategory(): Record<EventCategory, WebhookEvent[]> {
  const result: Record<EventCategory, WebhookEvent[]> = {
    inference: [],
    model: [],
    billing: [],
    provider: [],
    org: [],
    system: [],
  };
  for (const event of WEBHOOK_EVENT_TYPES) {
    result[event.category].push(event);
  }
  return result;
}

function getRetryDelay(retryCount: number): number {
  if (retryCount <= 0) return 0;
  if (retryCount > RETRY_DELAYS_MS.length) return RETRY_DELAYS_MS[RETRY_DELAYS_MS.length - 1];
  return RETRY_DELAYS_MS[retryCount - 1];
}

function shouldRetry(delivery: WebhookDelivery): boolean {
  return !delivery.success && delivery.retries < delivery.maxRetries;
}

function isJsonOutput(args: ParsedArgs): boolean {
  return args.options.json === true || args.options.j === true;
}

// ── Mock data ──────────────────────────────────────────────────────

function getMockWebhooks(): WebhookSubscription[] {
  return [
    {
      id: 'wh_abc123def456',
      url: 'https://hooks.acme.com/xergon',
      events: ['inference.complete', 'inference.error'],
      description: 'Production inference events',
      secret: 'whsec_abcdef1234567890abcdef1234567890abcdef1234567890abcdef123456',
      status: 'active',
      createdAt: '2025-01-20T10:00:00Z',
      updatedAt: '2025-05-30T14:00:00Z',
      lastDelivery: {
        id: 'dlv_111111222222',
        webhookId: 'wh_abc123def456',
        event: 'inference.complete',
        payload: { requestId: 'req_001', model: 'llama-3.3-70b' },
        statusCode: 200,
        success: true,
        status: 'success',
        duration: 125,
        response: 'OK',
        errorMessage: null,
        retries: 0,
        maxRetries: MAX_RETRIES,
        timestamp: '2025-06-01T14:15:00Z',
        nextRetry: null,
        signature: 'sha256=abc123',
      },
      totalDeliveries: 15420,
      failureCount: 23,
      successRate: 99.85,
    },
    {
      id: 'wh_fedcba654321',
      url: 'https://billing.acme.com/webhooks',
      events: ['billing.paid', 'billing.failed', 'billing.limit_reached'],
      description: 'Billing notifications',
      secret: 'whsec_111111111111111111111111111111111111111111111111111111111111',
      status: 'active',
      createdAt: '2025-02-01T09:00:00Z',
      updatedAt: '2025-05-28T11:00:00Z',
      lastDelivery: {
        id: 'dlv_333333444444',
        webhookId: 'wh_fedcba654321',
        event: 'billing.paid',
        payload: { invoiceId: 'inv_001', amount: 25.50 },
        statusCode: 200,
        success: true,
        status: 'success',
        duration: 89,
        response: '{"received": true}',
        errorMessage: null,
        retries: 0,
        maxRetries: MAX_RETRIES,
        timestamp: '2025-06-01T00:00:00Z',
        nextRetry: null,
        signature: 'sha256=def456',
      },
      totalDeliveries: 328,
      failureCount: 2,
      successRate: 99.39,
    },
    {
      id: 'wh_999999888888',
      url: 'https://old.acme.com/hook',
      events: ['model.deployed'],
      description: 'Old model deployment hook',
      secret: 'whsec_zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz',
      status: 'disabled',
      createdAt: '2025-01-05T08:00:00Z',
      updatedAt: '2025-03-15T10:00:00Z',
      lastDelivery: null,
      totalDeliveries: 12,
      failureCount: 5,
      successRate: 58.33,
    },
  ];
}

function getMockDeliveryHistory(webhookId: string): WebhookDelivery[] {
  return [
    {
      id: 'dlv_111111222222',
      webhookId,
      event: 'inference.complete',
      payload: { requestId: 'req_001', model: 'llama-3.3-70b', duration: 1250 },
      statusCode: 200,
      success: true,
      status: 'success',
      duration: 125,
      response: 'OK',
      errorMessage: null,
      retries: 0,
      maxRetries: MAX_RETRIES,
      timestamp: '2025-06-01T14:15:00Z',
      nextRetry: null,
      signature: 'sha256=abc123',
    },
    {
      id: 'dlv_222222333333',
      webhookId,
      event: 'inference.error',
      payload: { requestId: 'req_002', error: 'Provider timeout' },
      statusCode: 200,
      success: true,
      status: 'success',
      duration: 95,
      response: 'Acknowledged',
      errorMessage: null,
      retries: 0,
      maxRetries: MAX_RETRIES,
      timestamp: '2025-06-01T14:10:00Z',
      nextRetry: null,
      signature: 'sha256=def456',
    },
    {
      id: 'dlv_333333444444',
      webhookId,
      event: 'inference.complete',
      payload: { requestId: 'req_003', model: 'llama-3.3-70b' },
      statusCode: 500,
      success: false,
      status: 'failure',
      duration: 5000,
      response: 'Internal Server Error',
      errorMessage: 'Connection timeout after 5000ms',
      retries: 3,
      maxRetries: MAX_RETRIES,
      timestamp: '2025-06-01T13:55:00Z',
      nextRetry: null,
      signature: 'sha256=ghi789',
    },
    {
      id: 'dlv_444444555555',
      webhookId,
      event: 'inference.complete',
      payload: { requestId: 'req_004', model: 'mistral-7b' },
      statusCode: 502,
      success: false,
      status: 'retrying',
      duration: 3000,
      response: 'Bad Gateway',
      errorMessage: 'Upstream unavailable',
      retries: 1,
      maxRetries: MAX_RETRIES,
      timestamp: '2025-06-01T13:50:00Z',
      nextRetry: '2025-06-01T13:50:05Z',
      signature: 'sha256=jkl012',
    },
    {
      id: 'dlv_555555666666',
      webhookId,
      event: 'inference.complete',
      payload: { requestId: 'req_005', model: 'llama-3.3-70b' },
      statusCode: 200,
      success: true,
      status: 'success',
      duration: 110,
      response: 'OK',
      errorMessage: null,
      retries: 0,
      maxRetries: MAX_RETRIES,
      timestamp: '2025-06-01T13:45:00Z',
      nextRetry: null,
      signature: 'sha256=mno345',
    },
  ];
}

// ── Options ────────────────────────────────────────────────────────

const webhookOptions: CommandOption[] = [
  {
    name: 'url',
    short: '',
    long: '--url',
    description: 'Webhook URL endpoint',
    required: false,
    type: 'string',
  },
  {
    name: 'events',
    short: '',
    long: '--events',
    description: 'Comma-separated event types to subscribe to',
    required: false,
    type: 'string',
  },
  {
    name: 'description',
    short: '',
    long: '--description',
    description: 'Webhook description',
    required: false,
    type: 'string',
  },
  {
    name: 'secret',
    short: '',
    long: '--secret',
    description: 'Webhook signing secret',
    required: false,
    type: 'string',
  },
  {
    name: 'event',
    short: '',
    long: '--event',
    description: 'Specific event type for test delivery',
    required: false,
    type: 'string',
  },
  {
    name: 'limit',
    short: '',
    long: '--limit',
    description: 'Max delivery history entries (default: 20)',
    required: false,
    default: '20',
    type: 'string',
  },
  {
    name: 'force',
    short: '',
    long: '--force',
    description: 'Skip confirmation prompt',
    required: false,
    type: 'boolean',
  },
  {
    name: 'verify',
    short: '',
    long: '--verify',
    description: 'Verify a webhook signature (with --payload, --secret, --signature)',
    required: false,
    type: 'boolean',
  },
  {
    name: 'payload',
    short: '',
    long: '--payload',
    description: 'Payload string for signature verification',
    required: false,
    type: 'string',
  },
  {
    name: 'signature',
    short: '',
    long: '--signature',
    description: 'Signature to verify (sha256=...)',
    required: false,
    type: 'string',
  },
  {
    name: 'timestamp',
    short: '',
    long: '--timestamp',
    description: 'Timestamp for signature verification',
    required: false,
    type: 'string',
  },
];

// ── Subcommand handlers ────────────────────────────────────────────

async function handleList(_args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const outputJson = isJsonOutput(_args);

  try {
    let webhooks: WebhookSubscription[];
    if (ctx.client) {
      const resp = await ctx.client.get('/api/v1/webhooks');
      webhooks = (resp as any).webhooks ?? resp;
    } else {
      webhooks = getMockWebhooks();
    }

    if (outputJson) {
      ctx.output.write(JSON.stringify(webhooks, null, 2));
      return;
    }

    if (webhooks.length === 0) {
      ctx.output.info('No webhooks configured. Create one with: xergon webhook create');
      return;
    }

    const tableData = webhooks.map(w => ({
      ID: w.id.length > 16 ? w.id.slice(0, 16) + '...' : w.id,
      URL: w.url.length > 40 ? w.url.slice(0, 40) + '...' : w.url,
      Events: String(w.events.length),
      Status: w.status === 'active'
        ? ctx.output.colorize('ACTIVE', 'green')
        : w.status === 'disabled'
          ? ctx.output.colorize('DISABLED', 'red')
          : ctx.output.colorize('PENDING', 'yellow'),
      'Success Rate': `${w.successRate.toFixed(1)}%`,
      Failures: String(w.failureCount),
      'Last Delivery': w.lastDelivery
        ? formatRelativeTime(w.lastDelivery.timestamp)
        : 'never',
    }));

    ctx.output.write(ctx.output.formatTable(tableData, `Webhooks (${webhooks.length})`));
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to list webhooks: ${message}`);
    process.exit(1);
  }
}

async function handleCreate(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const url = (args.options.url as string) || args.positional[1];
  const eventsStr = args.options.events as string | undefined;
  const description = args.options.description as string | undefined;
  const outputJson = isJsonOutput(args);

  if (!url) {
    ctx.output.writeError('URL required. Use: xergon webhook create --url <url> --events <events>');
    process.exit(1);
    return;
  }

  try {
    new URL(url);
  } catch {
    ctx.output.writeError(`Invalid URL: ${url}`);
    process.exit(1);
    return;
  }

  if (!eventsStr) {
    ctx.output.writeError('Events required. Use: --events inference.complete,inference.error');
    ctx.output.write('Run "xergon webhook events" to see available events.');
    process.exit(1);
    return;
  }

  const events = parseEvents(eventsStr);
  const invalidEvents = events.filter(e => !isValidEvent(e));
  if (invalidEvents.length > 0) {
    ctx.output.writeError(`Unknown event types: ${invalidEvents.join(', ')}`);
    ctx.output.write('Run "xergon webhook events" to see available events.');
    process.exit(1);
    return;
  }

  const webhookId = generateWebhookId();
  const secret = generateWebhookSecret();

  try {
    if (ctx.client) {
      const resp = await ctx.client.post('/api/v1/webhooks', {
        url,
        events,
        description,
      });
      const webhook = resp as any;

      if (outputJson) {
        ctx.output.write(JSON.stringify(webhook, null, 2));
        return;
      }

      ctx.output.success('Webhook created');
      ctx.output.write(`  ID:          ${webhook.id}`);
      ctx.output.write(`  URL:         ${webhook.url}`);
      ctx.output.write(`  Events:      ${webhook.events.join(', ')}`);
      ctx.output.write(`  Secret:      ${webhook.secret}`);
      ctx.output.write(`  Status:      ${webhook.status}`);
      ctx.output.write('');
      ctx.output.info('Keep the secret safe -- it is used to verify webhook signatures.');
    } else {
      const result: CreateWebhookResult = {
        success: true,
        id: webhookId,
        url,
        events,
        secret,
        message: `Webhook created successfully.`,
      };

      if (outputJson) {
        ctx.output.write(JSON.stringify(result, null, 2));
        return;
      }

      ctx.output.success('Webhook created');
      ctx.output.write(`  ID:          ${result.id}`);
      ctx.output.write(`  URL:         ${result.url}`);
      ctx.output.write(`  Events:      ${result.events.join(', ')}`);
      ctx.output.write(`  Secret:      ${result.secret}`);
      ctx.output.write(`  Status:      active`);
      ctx.output.write('');
      ctx.output.info('Keep the secret safe -- it is used to verify webhook signatures.');
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to create webhook: ${message}`);
    process.exit(1);
  }
}

async function handleDelete(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const id = args.positional[1];
  const force = args.options.force as boolean | undefined;
  const outputJson = isJsonOutput(args);

  if (!id) {
    ctx.output.writeError('Webhook ID required. Use: xergon webhook delete <id>');
    process.exit(1);
    return;
  }

  if (!force) {
    ctx.output.writeError('This will permanently delete the webhook.');
    ctx.output.write('Use --force to confirm deletion.');
    process.exit(1);
    return;
  }

  try {
    if (ctx.client) {
      await ctx.client.delete(`/api/v1/webhooks/${id}`);
    }

    const result: DeleteWebhookResult = {
      success: true,
      id,
      message: `Webhook ${id} deleted.`,
    };

    if (outputJson) {
      ctx.output.write(JSON.stringify(result, null, 2));
      return;
    }

    ctx.output.success(`Webhook ${id} deleted.`);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to delete webhook: ${message}`);
    process.exit(1);
  }
}

async function handleTest(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const id = args.positional[1];
  const event = args.options.event as string | undefined;
  const outputJson = isJsonOutput(args);

  if (!id) {
    ctx.output.writeError('Webhook ID required. Use: xergon webhook test <id>');
    process.exit(1);
    return;
  }

  if (event && !isValidEvent(event)) {
    ctx.output.writeError(`Unknown event type: ${event}`);
    ctx.output.write('Run "xergon webhook events" to see available events.');
    process.exit(1);
    return;
  }

  const testEvent = event || 'inference.complete';
  const deliveryId = generateDeliveryId();

  try {
    if (ctx.client) {
      const resp = await ctx.client.post(`/api/v1/webhooks/${id}/test`, { event: testEvent });
      const delivery = resp as any;

      if (outputJson) {
        ctx.output.write(JSON.stringify(delivery, null, 2));
        return;
      }

      ctx.output.success(delivery.success ? 'Test delivery succeeded' : 'Test delivery failed');
      ctx.output.write(`  Event:     ${delivery.event}`);
      ctx.output.write(`  Status:    ${delivery.statusCode}`);
      ctx.output.write(`  Duration:  ${delivery.duration}ms`);
      ctx.output.write(`  Response:  ${(delivery.response || '').slice(0, 200)}`);
    } else {
      const result: TestDeliveryResult = {
        success: true,
        deliveryId,
        event: testEvent,
        statusCode: 200,
        duration: 142,
        response: '{"status": "ok"}',
        message: `Test delivery of "${testEvent}" to webhook ${id} succeeded.`,
      };

      if (outputJson) {
        ctx.output.write(JSON.stringify(result, null, 2));
        return;
      }

      ctx.output.success('Test delivery succeeded');
      ctx.output.write(`  Event:     ${result.event}`);
      ctx.output.write(`  Status:    ${result.statusCode}`);
      ctx.output.write(`  Duration:  ${result.duration}ms`);
      ctx.output.write(`  Response:  ${(result.response || '').slice(0, 200)}`);
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Test delivery failed: ${message}`);
    process.exit(1);
  }
}

async function handleHistory(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const id = args.positional[1];
  const limit = parseInt((args.options.limit as string) || '20', 10);
  const outputJson = isJsonOutput(args);

  if (!id) {
    ctx.output.writeError('Webhook ID required. Use: xergon webhook history <id>');
    process.exit(1);
    return;
  }

  try {
    let deliveries: WebhookDelivery[];
    if (ctx.client) {
      const resp = await ctx.client.get(`/api/v1/webhooks/${id}/deliveries?limit=${limit}`);
      deliveries = (resp as any).deliveries ?? resp;
    } else {
      deliveries = getMockDeliveryHistory(id).slice(0, limit);
    }

    if (outputJson) {
      ctx.output.write(JSON.stringify(deliveries, null, 2));
      return;
    }

    if (deliveries.length === 0) {
      ctx.output.info('No delivery history found for this webhook.');
      return;
    }

    // Summary
    const successes = deliveries.filter(d => d.success).length;
    const failures = deliveries.length - successes;
    ctx.output.write(ctx.output.colorize(`Delivery History for ${id}`, 'bold'));
    ctx.output.write(`  Total: ${deliveries.length} | Success: ${ctx.output.colorize(String(successes), 'green')} | Failed: ${ctx.output.colorize(String(failures), 'red')}`);
    ctx.output.write('');

    const tableData = deliveries.map(d => ({
      ID: d.id.length > 16 ? d.id.slice(0, 16) + '...' : d.id,
      Event: d.event,
      Status: d.success
        ? ctx.output.colorize(formatDeliveryStatus('success'), 'green')
        : d.status === 'retrying'
          ? ctx.output.colorize(formatDeliveryStatus('retrying'), 'yellow')
          : ctx.output.colorize(formatDeliveryStatus('failure'), 'red'),
      Code: String(d.statusCode),
      Duration: `${d.duration}ms`,
      Retries: `${d.retries}/${d.maxRetries}`,
      Time: formatRelativeTime(d.timestamp),
    }));

    ctx.output.write(ctx.output.formatTable(tableData, `Recent Deliveries (${deliveries.length})`));
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to get delivery history: ${message}`);
    process.exit(1);
  }
}

async function handleEvents(_args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const outputJson = isJsonOutput(_args);
  const byCategory = getEventsByCategory();

  if (outputJson) {
    ctx.output.write(JSON.stringify(WEBHOOK_EVENT_TYPES, null, 2));
    return;
  }

  ctx.output.write(ctx.output.colorize('Available Webhook Events:', 'bold'));
  ctx.output.write('');

  for (const cat of EVENT_CATEGORIES) {
    const events = byCategory[cat];
    ctx.output.write(`  ${ctx.output.colorize(CATEGORY_LABELS[cat] + ':', 'cyan')}`);
    for (const evt of events) {
      ctx.output.write(`    ${ctx.output.colorize('•', 'cyan')} ${evt.name}`);
      ctx.output.write(`      ${evt.description}`);
    }
    ctx.output.write('');
  }

  ctx.output.info('Subscribe to events with: xergon webhook create --url <url> --events event1,event2');
}

async function handleRetry(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const deliveryId = args.positional[1];
  const outputJson = isJsonOutput(args);

  if (!deliveryId) {
    ctx.output.writeError('Delivery ID required. Use: xergon webhook retry <delivery-id>');
    process.exit(1);
    return;
  }

  try {
    if (ctx.client) {
      const resp = await ctx.client.post(`/api/v1/webhooks/deliveries/${deliveryId}/retry`);
      const delivery = resp as any;

      if (outputJson) {
        ctx.output.write(JSON.stringify(delivery, null, 2));
        return;
      }

      ctx.output.success(`Retried delivery ${delivery.id}`);
      ctx.output.write(`  Event:     ${delivery.event}`);
      ctx.output.write(`  Status:    ${delivery.statusCode}`);
      ctx.output.write(`  Duration:  ${delivery.duration}ms`);
      ctx.output.write(`  Success:   ${delivery.success ? 'yes' : 'no'}`);
    } else {
      const result: RetryResult = {
        success: true,
        deliveryId,
        event: 'inference.complete',
        statusCode: 200,
        duration: 88,
        message: `Delivery ${deliveryId} retried successfully.`,
      };

      if (outputJson) {
        ctx.output.write(JSON.stringify(result, null, 2));
        return;
      }

      ctx.output.success(`Retried delivery ${deliveryId}`);
      ctx.output.write(`  Event:     ${result.event}`);
      ctx.output.write(`  Status:    ${result.statusCode}`);
      ctx.output.write(`  Duration:  ${result.duration}ms`);
      ctx.output.write(`  Success:   yes`);
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Retry failed: ${message}`);
    process.exit(1);
  }
}

async function handleVerify(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const payload = args.options.payload as string | undefined;
  const secret = args.options.secret as string | undefined;
  const signature = args.options.signature as string | undefined;
  const timestampStr = args.options.timestamp as string | undefined;
  const outputJson = isJsonOutput(args);

  if (!payload || !secret || !signature) {
    ctx.output.writeError('Missing required options for verification.');
    ctx.output.write('Use: xergon webhook verify --payload <str> --secret <str> --signature <sha256=...> [--timestamp <ms>]');
    process.exit(1);
    return;
  }

  const timestamp = timestampStr ? parseInt(timestampStr, 10) : Date.now();

  const result = verifySignature(payload, secret, signature, timestamp);

  if (outputJson) {
    ctx.output.write(JSON.stringify(result, null, 2));
    return;
  }

  if (result.valid) {
    ctx.output.success('Signature verified successfully');
  } else {
    ctx.output.writeError(`Signature verification failed: ${result.message}`);
  }
}

// ── Command action ─────────────────────────────────────────────────

async function webhookAction(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const sub = args.positional[0];

  if (!sub) {
    ctx.output.writeError('Usage: xergon webhook <list|create|delete|test|history|events|retry|verify> [args]');
    process.exit(1);
    return;
  }

  switch (sub) {
    case 'list':
    case 'ls':
      await handleList(args, ctx);
      break;
    case 'create':
    case 'add':
      await handleCreate(args, ctx);
      break;
    case 'delete':
    case 'del':
    case 'rm':
      await handleDelete(args, ctx);
      break;
    case 'test':
    case 'ping':
      await handleTest(args, ctx);
      break;
    case 'history':
    case 'deliveries':
      await handleHistory(args, ctx);
      break;
    case 'events':
    case 'types':
      await handleEvents(args, ctx);
      break;
    case 'retry':
    case 'replay':
      await handleRetry(args, ctx);
      break;
    case 'verify':
      await handleVerify(args, ctx);
      break;
    default:
      ctx.output.writeError(`Unknown subcommand: ${sub}`);
      ctx.output.write('Usage: xergon webhook <list|create|delete|test|history|events|retry|verify> [args]');
      process.exit(1);
  }
}

// ── Command definition ─────────────────────────────────────────────

export const webhookCommand: Command = {
  name: 'webhook',
  description: 'Manage webhooks for event integration',
  aliases: ['webhooks'],
  options: webhookOptions,
  action: webhookAction,
};

// ── Exports for testing ───────────────────────────────────────────

export {
  // Types
  type WebhookStatus,
  type DeliveryStatus,
  type EventCategory,
  type WebhookSubscription,
  type WebhookDelivery,
  type WebhookEvent,
  type CreateWebhookResult,
  type DeleteWebhookResult,
  type TestDeliveryResult,
  type RetryResult,
  type SignatureVerifyResult,
  // Constants
  WEBHOOK_EVENT_TYPES,
  EVENT_CATEGORIES,
  CATEGORY_LABELS,
  MAX_RETRIES,
  RETRY_DELAYS_MS,
  // Helpers
  generateWebhookId,
  generateDeliveryId,
  generateWebhookSecret,
  computeSignature,
  verifySignature,
  formatDeliveryStatus,
  formatRelativeTime,
  maskSecret,
  parseEvents,
  isValidEvent,
  getEventsByCategory,
  getRetryDelay,
  shouldRetry,
  isJsonOutput,
  // Handlers
  handleList,
  handleCreate,
  handleDelete,
  handleTest,
  handleHistory,
  handleEvents,
  handleRetry,
  handleVerify,
  webhookAction,
};
