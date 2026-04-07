/**
 * Tests for CLI command: webhook
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import * as crypto from 'node:crypto';
import {
  webhookCommand,
  webhookAction,
  handleList,
  handleCreate,
  handleDelete,
  handleTest,
  handleHistory,
  handleEvents,
  handleRetry,
  handleVerify,
  WEBHOOK_EVENT_TYPES,
  EVENT_CATEGORIES,
  CATEGORY_LABELS,
  MAX_RETRIES,
  RETRY_DELAYS_MS,
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
  type DeliveryStatus,
  type EventCategory,
  type WebhookDelivery,
} from './webhook';

// ── Mock output formatter ──────────────────────────────────────────

function createMockOutput() {
  return {
    colorize: (text: string, _style: string) => text,
    write: vi.fn(),
    writeError: vi.fn(),
    info: vi.fn(),
    success: vi.fn(),
    warn: vi.fn(),
    formatTable: (data: any[]) => JSON.stringify(data),
    formatOutput: (data: any) => JSON.stringify(data, null, 2),
  };
}

function createMockContext(overrides?: Record<string, any>): any {
  return {
    client: null,
    config: {
      baseUrl: 'https://relay.xergon.gg',
      apiKey: '',
      defaultModel: 'llama-3.3-70b',
      outputFormat: 'text' as const,
      color: false,
      timeout: 30000,
    },
    output: createMockOutput(),
    ...overrides,
  };
}

// ── WEBHOOK_EVENT_TYPES tests ──────────────────────────────────────

describe('WEBHOOK_EVENT_TYPES', () => {
  it('is non-empty', () => {
    expect(WEBHOOK_EVENT_TYPES.length).toBeGreaterThan(0);
  });
  it('includes inference.complete', () => {
    expect(isValidEvent('inference.complete')).toBe(true);
  });
  it('includes model.deployed', () => {
    expect(isValidEvent('model.deployed')).toBe(true);
  });
  it('includes billing.paid', () => {
    expect(isValidEvent('billing.paid')).toBe(true);
  });
  it('includes provider.registered', () => {
    expect(isValidEvent('provider.registered')).toBe(true);
  });
  it('each event has a category', () => {
    for (const evt of WEBHOOK_EVENT_TYPES) {
      expect(EVENT_CATEGORIES).toContain(evt.category);
    }
  });
  it('each event has a description', () => {
    for (const evt of WEBHOOK_EVENT_TYPES) {
      expect(evt.description.length).toBeGreaterThan(0);
    }
  });
  it('each event has a payload example', () => {
    for (const evt of WEBHOOK_EVENT_TYPES) {
      expect(evt.payloadExample).toBeDefined();
      expect(typeof evt.payloadExample).toBe('object');
    }
  });
  it('event names are unique', () => {
    const names = WEBHOOK_EVENT_TYPES.map(e => e.name);
    expect(new Set(names).size).toBe(names.length);
  });
});

// ── EVENT_CATEGORIES tests ─────────────────────────────────────────

describe('EVENT_CATEGORIES', () => {
  it('includes inference', () => {
    expect(EVENT_CATEGORIES).toContain('inference');
  });
  it('includes model', () => {
    expect(EVENT_CATEGORIES).toContain('model');
  });
  it('includes billing', () => {
    expect(EVENT_CATEGORIES).toContain('billing');
  });
  it('includes provider', () => {
    expect(EVENT_CATEGORIES).toContain('provider');
  });
  it('includes org', () => {
    expect(EVENT_CATEGORIES).toContain('org');
  });
  it('includes system', () => {
    expect(EVENT_CATEGORIES).toContain('system');
  });
});

// ── CATEGORY_LABELS tests ──────────────────────────────────────────

describe('CATEGORY_LABELS', () => {
  it('has labels for all categories', () => {
    for (const cat of EVENT_CATEGORIES) {
      expect(CATEGORY_LABELS[cat]).toBeTruthy();
      expect(typeof CATEGORY_LABELS[cat]).toBe('string');
    }
  });
});

// ── generateWebhookId tests ────────────────────────────────────────

describe('generateWebhookId', () => {
  it('starts with wh_', () => {
    expect(generateWebhookId()).toMatch(/^wh_/);
  });
  it('is unique', () => {
    const ids = new Set(Array.from({ length: 100 }, () => generateWebhookId()));
    expect(ids.size).toBe(100);
  });
});

// ── generateDeliveryId tests ───────────────────────────────────────

describe('generateDeliveryId', () => {
  it('starts with dlv_', () => {
    expect(generateDeliveryId()).toMatch(/^dlv_/);
  });
  it('is unique', () => {
    const ids = new Set(Array.from({ length: 100 }, () => generateDeliveryId()));
    expect(ids.size).toBe(100);
  });
});

// ── generateWebhookSecret tests ────────────────────────────────────

describe('generateWebhookSecret', () => {
  it('starts with whsec_', () => {
    expect(generateWebhookSecret()).toMatch(/^whsec_/);
  });
  it('is unique', () => {
    const secrets = new Set(Array.from({ length: 100 }, () => generateWebhookSecret()));
    expect(secrets.size).toBe(100);
  });
});

// ── computeSignature tests ─────────────────────────────────────────

describe('computeSignature', () => {
  it('starts with sha256=', () => {
    expect(computeSignature('test', 'secret', 1234567890)).toMatch(/^sha256=/);
  });
  it('is deterministic', () => {
    const sig1 = computeSignature('payload', 'secret', 1000);
    const sig2 = computeSignature('payload', 'secret', 1000);
    expect(sig1).toBe(sig2);
  });
  it('changes with different payload', () => {
    const sig1 = computeSignature('payload1', 'secret', 1000);
    const sig2 = computeSignature('payload2', 'secret', 1000);
    expect(sig1).not.toBe(sig2);
  });
  it('changes with different secret', () => {
    const sig1 = computeSignature('payload', 'secret1', 1000);
    const sig2 = computeSignature('payload', 'secret2', 1000);
    expect(sig1).not.toBe(sig2);
  });
  it('changes with different timestamp', () => {
    const sig1 = computeSignature('payload', 'secret', 1000);
    const sig2 = computeSignature('payload', 'secret', 2000);
    expect(sig1).not.toBe(sig2);
  });
});

// ── verifySignature tests ──────────────────────────────────────────

describe('verifySignature', () => {
  it('validates correct signature', () => {
    const payload = '{"event":"test"}';
    const secret = 'whsec_testsecret';
    const timestamp = Date.now();
    const signature = computeSignature(payload, secret, timestamp);
    const result = verifySignature(payload, secret, signature, timestamp);
    expect(result.valid).toBe(true);
  });

  it('rejects wrong signature', () => {
    const payload = '{"event":"test"}';
    const secret = 'whsec_testsecret';
    const timestamp = Date.now();
    const result = verifySignature(payload, secret, 'sha256=wrong', timestamp);
    expect(result.valid).toBe(false);
    expect(result.message).toBe('Signature mismatch');
  });

  it('rejects expired timestamp', () => {
    const payload = '{"event":"test"}';
    const secret = 'whsec_testsecret';
    const timestamp = Date.now() - 600_000; // 10 minutes ago
    const signature = computeSignature(payload, secret, timestamp);
    const result = verifySignature(payload, secret, signature, timestamp);
    expect(result.valid).toBe(false);
    expect(result.message).toBe('Timestamp outside tolerance window');
  });

  it('rejects future timestamp', () => {
    const payload = '{"event":"test"}';
    const secret = 'whsec_testsecret';
    const timestamp = Date.now() + 600_000; // 10 minutes ahead
    const signature = computeSignature(payload, secret, timestamp);
    const result = verifySignature(payload, secret, signature, timestamp);
    expect(result.valid).toBe(false);
    expect(result.message).toBe('Timestamp outside tolerance window');
  });

  it('accepts timestamp within tolerance', () => {
    const payload = '{"event":"test"}';
    const secret = 'whsec_testsecret';
    const timestamp = Date.now() - 60_000; // 1 minute ago (within 5 min tolerance)
    const signature = computeSignature(payload, secret, timestamp);
    const result = verifySignature(payload, secret, signature, timestamp);
    expect(result.valid).toBe(true);
  });
});

// ── parseEvents tests ──────────────────────────────────────────────

describe('parseEvents', () => {
  it('parses single event', () => {
    expect(parseEvents('inference.complete')).toEqual(['inference.complete']);
  });
  it('parses comma-separated events', () => {
    expect(parseEvents('inference.complete,model.deployed')).toEqual(['inference.complete', 'model.deployed']);
  });
  it('trims whitespace', () => {
    expect(parseEvents(' inference.complete , model.deployed ')).toEqual(['inference.complete', 'model.deployed']);
  });
  it('filters empty strings', () => {
    expect(parseEvents('inference.complete,,model.deployed')).toEqual(['inference.complete', 'model.deployed']);
  });
  it('returns empty for empty string', () => {
    expect(parseEvents('')).toEqual([]);
  });
});

// ── isValidEvent tests ─────────────────────────────────────────────

describe('isValidEvent', () => {
  it('accepts valid event', () => {
    expect(isValidEvent('inference.complete')).toBe(true);
  });
  it('rejects invalid event', () => {
    expect(isValidEvent('nonexistent.event')).toBe(false);
  });
  it('rejects empty string', () => {
    expect(isValidEvent('')).toBe(false);
  });
});

// ── getEventsByCategory tests ──────────────────────────────────────

describe('getEventsByCategory', () => {
  it('returns a key for each category', () => {
    const byCategory = getEventsByCategory();
    for (const cat of EVENT_CATEGORIES) {
      expect(byCategory[cat]).toBeDefined();
      expect(Array.isArray(byCategory[cat])).toBe(true);
    }
  });
  it('all events are accounted for', () => {
    const byCategory = getEventsByCategory();
    const total = Object.values(byCategory).reduce((sum, arr) => sum + arr.length, 0);
    expect(total).toBe(WEBHOOK_EVENT_TYPES.length);
  });
});

// ── getRetryDelay tests ────────────────────────────────────────────

describe('getRetryDelay', () => {
  it('returns 0 for retryCount 0', () => {
    expect(getRetryDelay(0)).toBe(0);
  });
  it('returns first delay for retryCount 1', () => {
    expect(getRetryDelay(1)).toBe(RETRY_DELAYS_MS[0]);
  });
  it('returns second delay for retryCount 2', () => {
    expect(getRetryDelay(2)).toBe(RETRY_DELAYS_MS[1]);
  });
  it('returns third delay for retryCount 3', () => {
    expect(getRetryDelay(3)).toBe(RETRY_DELAYS_MS[2]);
  });
  it('returns last delay for exceeding retries', () => {
    expect(getRetryDelay(10)).toBe(RETRY_DELAYS_MS[RETRY_DELAYS_MS.length - 1]);
  });
});

// ── shouldRetry tests ──────────────────────────────────────────────

describe('shouldRetry', () => {
  const makeDelivery = (overrides: Partial<WebhookDelivery> = {}): WebhookDelivery => ({
    id: 'dlv_test',
    webhookId: 'wh_test',
    event: 'inference.complete',
    payload: {},
    statusCode: 500,
    success: false,
    status: 'failure',
    duration: 1000,
    response: null,
    errorMessage: 'error',
    retries: 0,
    maxRetries: MAX_RETRIES,
    timestamp: new Date().toISOString(),
    nextRetry: null,
    signature: 'sha256=test',
    ...overrides,
  });

  it('returns true for failed delivery with retries left', () => {
    expect(shouldRetry(makeDelivery({ success: false, retries: 1 }))).toBe(true);
  });
  it('returns false for successful delivery', () => {
    expect(shouldRetry(makeDelivery({ success: true }))).toBe(false);
  });
  it('returns false when max retries reached', () => {
    expect(shouldRetry(makeDelivery({ success: false, retries: MAX_RETRIES }))).toBe(false);
  });
});

// ── maskSecret tests ───────────────────────────────────────────────

describe('maskSecret', () => {
  it('masks long secrets', () => {
    const secret = 'whsec_abcdefghijklmnopqrstuvwxyz1234567890';
    const result = maskSecret(secret);
    expect(result).toContain('••••');
    expect(result.startsWith('whsec_a')).toBe(true);
  });
  it('returns dots for short secrets', () => {
    expect(maskSecret('short')).toBe('••••••••••••');
  });
  it('returns dots for empty string', () => {
    expect(maskSecret('')).toBe('••••••••••••');
  });
});

// ── formatDeliveryStatus tests ─────────────────────────────────────

describe('formatDeliveryStatus', () => {
  it('formats success', () => {
    expect(formatDeliveryStatus('success')).toBe('SUCCESS');
  });
  it('formats failure', () => {
    expect(formatDeliveryStatus('failure')).toBe('FAILURE');
  });
  it('formats pending', () => {
    expect(formatDeliveryStatus('pending')).toBe('PENDING');
  });
  it('formats retrying', () => {
    expect(formatDeliveryStatus('retrying')).toBe('RETRYING');
  });
});

// ── formatRelativeTime tests ───────────────────────────────────────

describe('formatRelativeTime', () => {
  it('shows seconds ago', () => {
    const past = new Date(Date.now() - 30_000).toISOString();
    expect(formatRelativeTime(past)).toMatch(/\d+s ago$/);
  });
  it('shows minutes ago', () => {
    const past = new Date(Date.now() - 5 * 60_000).toISOString();
    expect(formatRelativeTime(past)).toMatch(/\d+m ago$/);
  });
  it('shows hours ago', () => {
    const past = new Date(Date.now() - 3 * 3600_000).toISOString();
    expect(formatRelativeTime(past)).toMatch(/\d+h ago$/);
  });
  it('shows days ago', () => {
    const past = new Date(Date.now() - 2 * 86400_000).toISOString();
    expect(formatRelativeTime(past)).toMatch(/\d+d ago$/);
  });
  it('returns unknown for invalid input', () => {
    expect(formatRelativeTime('bad')).toBe('unknown');
  });
});

// ── isJsonOutput tests ─────────────────────────────────────────────

describe('isJsonOutput', () => {
  it('returns true for --json', () => {
    expect(isJsonOutput({ command: 'webhook', positional: [], options: { json: true } })).toBe(true);
  });
  it('returns true for -j', () => {
    expect(isJsonOutput({ command: 'webhook', positional: [], options: { j: true } })).toBe(true);
  });
  it('returns false when no json flag', () => {
    expect(isJsonOutput({ command: 'webhook', positional: [], options: {} })).toBe(false);
  });
});

// ── MAX_RETRIES and RETRY_DELAYS_MS ────────────────────────────────

describe('MAX_RETRIES', () => {
  it('is 3', () => {
    expect(MAX_RETRIES).toBe(3);
  });
});

describe('RETRY_DELAYS_MS', () => {
  it('has 3 entries', () => {
    expect(RETRY_DELAYS_MS).toHaveLength(3);
  });
  it('is increasing (backoff)', () => {
    for (let i = 1; i < RETRY_DELAYS_MS.length; i++) {
      expect(RETRY_DELAYS_MS[i]).toBeGreaterThan(RETRY_DELAYS_MS[i - 1]);
    }
  });
});

// ── webhookCommand definition ──────────────────────────────────────

describe('webhookCommand', () => {
  it('has correct name', () => {
    expect(webhookCommand.name).toBe('webhook');
  });
  it('has description', () => {
    expect(webhookCommand.description).toBeTruthy();
  });
  it('has aliases', () => {
    expect(webhookCommand.aliases).toContain('webhooks');
  });
  it('has options', () => {
    expect(webhookCommand.options.length).toBeGreaterThan(0);
  });
  it('has action function', () => {
    expect(typeof webhookCommand.action).toBe('function');
  });
});

// ── Handler tests ──────────────────────────────────────────────────

describe('handleList', () => {
  it('works without client (offline mode)', async () => {
    const ctx = createMockContext();
    await handleList({ command: 'webhook', positional: ['list'], options: {} }, ctx);
    expect(ctx.output.write).toHaveBeenCalled();
  });
});

describe('handleCreate', () => {
  it('requires URL', async () => {
    const ctx = createMockContext();
    vi.spyOn(process, 'exit').mockImplementation(() => { throw new Error('exit'); });
    await expect(handleCreate({ command: 'webhook', positional: ['create'], options: {} }, ctx)).rejects.toThrow('exit');
    expect(ctx.output.writeError).toHaveBeenCalledWith(expect.stringContaining('URL required'));
    vi.restoreAllMocks();
  });

  it('validates URL format', async () => {
    const ctx = createMockContext();
    vi.spyOn(process, 'exit').mockImplementation(() => { throw new Error('exit'); });
    await expect(handleCreate({ command: 'webhook', positional: ['create'], options: { url: 'not-a-url' } }, ctx)).rejects.toThrow('exit');
    expect(ctx.output.writeError).toHaveBeenCalledWith(expect.stringContaining('Invalid URL'));
    vi.restoreAllMocks();
  });

  it('requires events', async () => {
    const ctx = createMockContext();
    vi.spyOn(process, 'exit').mockImplementation(() => { throw new Error('exit'); });
    await expect(handleCreate({ command: 'webhook', positional: ['create'], options: { url: 'https://example.com/hook' } }, ctx)).rejects.toThrow('exit');
    expect(ctx.output.writeError).toHaveBeenCalledWith(expect.stringContaining('Events required'));
    vi.restoreAllMocks();
  });

  it('rejects invalid event types', async () => {
    const ctx = createMockContext();
    vi.spyOn(process, 'exit').mockImplementation(() => { throw new Error('exit'); });
    await expect(handleCreate({
      command: 'webhook', positional: ['create'],
      options: { url: 'https://example.com/hook', events: 'invalid.event' },
    }, ctx)).rejects.toThrow('exit');
    expect(ctx.output.writeError).toHaveBeenCalledWith(expect.stringContaining('Unknown event'));
    vi.restoreAllMocks();
  });

  it('works without client (offline mode)', async () => {
    const ctx = createMockContext();
    await handleCreate({
      command: 'webhook', positional: ['create'],
      options: { url: 'https://example.com/hook', events: 'inference.complete,inference.error' },
    }, ctx);
    expect(ctx.output.success).toHaveBeenCalledWith('Webhook created');
    expect(ctx.output.write).toHaveBeenCalledWith(expect.stringContaining('wh_'));
  });
});

describe('handleDelete', () => {
  it('requires webhook ID', async () => {
    const ctx = createMockContext();
    vi.spyOn(process, 'exit').mockImplementation(() => { throw new Error('exit'); });
    await expect(handleDelete({ command: 'webhook', positional: ['delete'], options: {} }, ctx)).rejects.toThrow('exit');
    expect(ctx.output.writeError).toHaveBeenCalledWith(expect.stringContaining('ID required'));
    vi.restoreAllMocks();
  });

  it('requires --force flag', async () => {
    const ctx = createMockContext();
    vi.spyOn(process, 'exit').mockImplementation(() => { throw new Error('exit'); });
    await expect(handleDelete({ command: 'webhook', positional: ['delete', 'wh_123'], options: {} }, ctx)).rejects.toThrow('exit');
    expect(ctx.output.writeError).toHaveBeenCalledWith(expect.stringContaining('permanently'));
    vi.restoreAllMocks();
  });

  it('works with --force', async () => {
    const ctx = createMockContext();
    await handleDelete({ command: 'webhook', positional: ['delete', 'wh_123'], options: { force: true } }, ctx);
    expect(ctx.output.success).toHaveBeenCalledWith(expect.stringContaining('deleted'));
  });
});

describe('handleTest', () => {
  it('requires webhook ID', async () => {
    const ctx = createMockContext();
    vi.spyOn(process, 'exit').mockImplementation(() => { throw new Error('exit'); });
    await expect(handleTest({ command: 'webhook', positional: ['test'], options: {} }, ctx)).rejects.toThrow('exit');
    expect(ctx.output.writeError).toHaveBeenCalledWith(expect.stringContaining('ID required'));
    vi.restoreAllMocks();
  });

  it('rejects invalid event type', async () => {
    const ctx = createMockContext();
    vi.spyOn(process, 'exit').mockImplementation(() => { throw new Error('exit'); });
    await expect(handleTest({ command: 'webhook', positional: ['test', 'wh_123'], options: { event: 'bad.event' } }, ctx)).rejects.toThrow('exit');
    expect(ctx.output.writeError).toHaveBeenCalledWith(expect.stringContaining('Unknown event'));
    vi.restoreAllMocks();
  });

  it('works without client (offline mode)', async () => {
    const ctx = createMockContext();
    await handleTest({ command: 'webhook', positional: ['test', 'wh_123'], options: {} }, ctx);
    expect(ctx.output.success).toHaveBeenCalledWith('Test delivery succeeded');
  });
});

describe('handleHistory', () => {
  it('requires webhook ID', async () => {
    const ctx = createMockContext();
    vi.spyOn(process, 'exit').mockImplementation(() => { throw new Error('exit'); });
    await expect(handleHistory({ command: 'webhook', positional: ['history'], options: {} }, ctx)).rejects.toThrow('exit');
    expect(ctx.output.writeError).toHaveBeenCalledWith(expect.stringContaining('ID required'));
    vi.restoreAllMocks();
  });

  it('works without client (offline mode)', async () => {
    const ctx = createMockContext();
    await handleHistory({ command: 'webhook', positional: ['history', 'wh_123'], options: {} }, ctx);
    expect(ctx.output.write).toHaveBeenCalled();
  });
});

describe('handleEvents', () => {
  it('works without client (offline mode)', async () => {
    const ctx = createMockContext();
    await handleEvents({ command: 'webhook', positional: ['events'], options: {} }, ctx);
    expect(ctx.output.write).toHaveBeenCalledWith(expect.stringContaining('Available'));
  });

  it('outputs JSON when --json flag', async () => {
    const ctx = createMockContext();
    await handleEvents({ command: 'webhook', positional: ['events'], options: { json: true } }, ctx);
    expect(ctx.output.write).toHaveBeenCalledWith(expect.stringContaining('"name"'));
  });
});

describe('handleRetry', () => {
  it('requires delivery ID', async () => {
    const ctx = createMockContext();
    vi.spyOn(process, 'exit').mockImplementation(() => { throw new Error('exit'); });
    await expect(handleRetry({ command: 'webhook', positional: ['retry'], options: {} }, ctx)).rejects.toThrow('exit');
    expect(ctx.output.writeError).toHaveBeenCalledWith(expect.stringContaining('Delivery ID required'));
    vi.restoreAllMocks();
  });

  it('works without client (offline mode)', async () => {
    const ctx = createMockContext();
    await handleRetry({ command: 'webhook', positional: ['retry', 'dlv_123'], options: {} }, ctx);
    expect(ctx.output.success).toHaveBeenCalledWith(expect.stringContaining('Retried'));
  });
});

describe('handleVerify', () => {
  it('requires payload, secret, and signature', async () => {
    const ctx = createMockContext();
    vi.spyOn(process, 'exit').mockImplementation(() => { throw new Error('exit'); });
    await expect(handleVerify({ command: 'webhook', positional: ['verify'], options: {} }, ctx)).rejects.toThrow('exit');
    expect(ctx.output.writeError).toHaveBeenCalledWith(expect.stringContaining('Missing required'));
    vi.restoreAllMocks();
  });

  it('verifies valid signature', async () => {
    const ctx = createMockContext();
    const payload = '{"event":"test"}';
    const secret = 'whsec_test';
    const timestamp = Date.now();
    const signature = computeSignature(payload, secret, timestamp);
    await handleVerify({
      command: 'webhook', positional: ['verify'],
      options: { payload, secret, signature, timestamp: String(timestamp) },
    }, ctx);
    expect(ctx.output.success).toHaveBeenCalledWith(expect.stringContaining('verified'));
  });

  it('rejects invalid signature', async () => {
    const ctx = createMockContext();
    await handleVerify({
      command: 'webhook', positional: ['verify'],
      options: { payload: '{"event":"test"}', secret: 'whsec_test', signature: 'sha256=invalid' },
    }, ctx);
    expect(ctx.output.writeError).toHaveBeenCalledWith(expect.stringContaining('failed'));
  });
});

describe('webhookAction dispatch', () => {
  it('shows usage when no subcommand', async () => {
    const ctx = createMockContext();
    vi.spyOn(process, 'exit').mockImplementation(() => { throw new Error('exit'); });
    await expect(webhookAction({ command: 'webhook', positional: [], options: {} }, ctx)).rejects.toThrow('exit');
    expect(ctx.output.writeError).toHaveBeenCalledWith(expect.stringContaining('Usage'));
    vi.restoreAllMocks();
  });

  it('rejects unknown subcommand', async () => {
    const ctx = createMockContext();
    vi.spyOn(process, 'exit').mockImplementation(() => { throw new Error('exit'); });
    await expect(webhookAction({ command: 'webhook', positional: ['bogus'], options: {} }, ctx)).rejects.toThrow('exit');
    expect(ctx.output.writeError).toHaveBeenCalledWith(expect.stringContaining('Unknown'));
    vi.restoreAllMocks();
  });

  it('accepts alias ls for list', async () => {
    const ctx = createMockContext();
    await webhookAction({ command: 'webhook', positional: ['ls'], options: {} }, ctx);
    expect(ctx.output.write).toHaveBeenCalled();
  });

  it('accepts alias replay for retry', async () => {
    const ctx = createMockContext();
    vi.spyOn(process, 'exit').mockImplementation(() => { throw new Error('exit'); });
    await expect(webhookAction({ command: 'webhook', positional: ['replay'], options: {} }, ctx)).rejects.toThrow('exit');
    expect(ctx.output.writeError).toHaveBeenCalledWith(expect.stringContaining('Delivery ID required'));
    vi.restoreAllMocks();
  });
});
