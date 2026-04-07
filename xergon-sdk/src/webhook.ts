/**
 * Xergon SDK -- Webhook Management
 *
 * Provides webhook creation, management, delivery tracking, and replay
 * capabilities. Webhooks allow integrating Xergon events with external services.
 */

import * as crypto from 'node:crypto';

// ── Types ───────────────────────────────────────────────────────────

export type WebhookEvent =
  | 'inference.complete'
  | 'inference.error'
  | 'model.deployed'
  | 'model.updated'
  | 'fine-tune.complete'
  | 'fine-tune.failed'
  | 'billing.invoice'
  | 'billing.threshold'
  | 'provider.registered'
  | 'provider.status_change'
  | 'team.member_added'
  | 'team.member_removed';

export interface Webhook {
  id: string;
  url: string;
  events: WebhookEvent[];
  secret: string;
  enabled: boolean;
  createdAt: string;
  lastDelivery?: WebhookDelivery;
  description?: string;
}

export interface WebhookDelivery {
  id: string;
  webhookId: string;
  event: string;
  payload: any;
  statusCode: number;
  response: string;
  duration: number;
  success: boolean;
  timestamp: string;
  retries: number;
}

export interface CreateWebhookParams {
  url: string;
  events: WebhookEvent[];
  description?: string;
}

export interface UpdateWebhookParams {
  url?: string;
  events?: WebhookEvent[];
  enabled?: boolean;
  description?: string;
}

// ── Supported Events ───────────────────────────────────────────────

export const SUPPORTED_WEBHOOK_EVENTS: WebhookEvent[] = [
  'inference.complete',
  'inference.error',
  'model.deployed',
  'model.updated',
  'fine-tune.complete',
  'fine-tune.failed',
  'billing.invoice',
  'billing.threshold',
  'provider.registered',
  'provider.status_change',
  'team.member_added',
  'team.member_removed',
];

// ── Webhook Client ─────────────────────────────────────────────────

export class WebhookClient {
  private baseUrl: string;

  constructor(options?: { baseUrl?: string }) {
    this.baseUrl = options?.baseUrl || 'https://relay.xergon.gg';
  }

  /**
   * Create a new webhook.
   */
  async createWebhook(params: CreateWebhookParams): Promise<Webhook> {
    const url = `${this.baseUrl}/v1/webhooks`;
    const response = await fetch(url, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(params),
    });

    if (!response.ok) {
      const data = await response.json().catch(() => ({}));
      throw new Error((data as any).message || `Failed to create webhook: ${response.status}`);
    }

    return await response.json() as Webhook;
  }

  /**
   * List all webhooks for the authenticated user.
   */
  async listWebhooks(): Promise<Webhook[]> {
    const url = `${this.baseUrl}/v1/webhooks`;
    const response = await fetch(url);

    if (!response.ok) {
      throw new Error(`Failed to list webhooks: ${response.status}`);
    }

    const data = await response.json() as { webhooks?: Webhook[] } | Webhook[];
    return Array.isArray(data) ? data : (data.webhooks || []);
  }

  /**
   * Get webhook details by ID.
   */
  async getWebhook(id: string): Promise<Webhook> {
    const url = `${this.baseUrl}/v1/webhooks/${encodeURIComponent(id)}`;
    const response = await fetch(url);

    if (!response.ok) {
      if (response.status === 404) throw new Error(`Webhook "${id}" not found`);
      throw new Error(`Failed to get webhook: ${response.status}`);
    }

    return await response.json() as Webhook;
  }

  /**
   * Update a webhook.
   */
  async updateWebhook(id: string, updates: UpdateWebhookParams): Promise<Webhook> {
    const url = `${this.baseUrl}/v1/webhooks/${encodeURIComponent(id)}`;
    const response = await fetch(url, {
      method: 'PATCH',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(updates),
    });

    if (!response.ok) {
      const data = await response.json().catch(() => ({}));
      throw new Error((data as any).message || `Failed to update webhook: ${response.status}`);
    }

    return await response.json() as Webhook;
  }

  /**
   * Delete a webhook.
   */
  async deleteWebhook(id: string): Promise<void> {
    const url = `${this.baseUrl}/v1/webhooks/${encodeURIComponent(id)}`;
    const response = await fetch(url, { method: 'DELETE' });

    if (!response.ok) {
      const data = await response.json().catch(() => ({}));
      throw new Error((data as any).message || `Failed to delete webhook: ${response.status}`);
    }
  }

  /**
   * Send a test event to a webhook.
   */
  async testWebhook(id: string, event?: WebhookEvent): Promise<WebhookDelivery> {
    const webhook = await this.getWebhook(id);
    const testEvent = event || (webhook.events[0] || 'inference.complete');

    const url = `${this.baseUrl}/v1/webhooks/${encodeURIComponent(id)}/test`;
    const response = await fetch(url, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ event: testEvent }),
    });

    if (!response.ok) {
      const data = await response.json().catch(() => ({}));
      throw new Error((data as any).message || `Test delivery failed: ${response.status}`);
    }

    return await response.json() as WebhookDelivery;
  }

  /**
   * Get delivery history for a webhook.
   */
  async getDeliveries(id: string, limit: number = 20): Promise<WebhookDelivery[]> {
    const params = new URLSearchParams({ limit: String(limit) });
    const url = `${this.baseUrl}/v1/webhooks/${encodeURIComponent(id)}/deliveries?${params}`;
    const response = await fetch(url);

    if (!response.ok) {
      throw new Error(`Failed to get deliveries: ${response.status}`);
    }

    const data = await response.json() as { deliveries?: WebhookDelivery[] } | WebhookDelivery[];
    return Array.isArray(data) ? data : (data.deliveries || []);
  }

  /**
   * Replay a failed (or any) webhook delivery.
   */
  async replayDelivery(deliveryId: string): Promise<WebhookDelivery> {
    const url = `${this.baseUrl}/v1/webhooks/deliveries/${encodeURIComponent(deliveryId)}/replay`;
    const response = await fetch(url, { method: 'POST' });

    if (!response.ok) {
      const data = await response.json().catch(() => ({}));
      throw new Error((data as any).message || `Failed to replay delivery: ${response.status}`);
    }

    return await response.json() as WebhookDelivery;
  }

  /**
   * Get all supported webhook events.
   */
  getSupportedEvents(): WebhookEvent[] {
    return [...SUPPORTED_WEBHOOK_EVENTS];
  }

  // ── Signature Verification ───────────────────────────────────────

  /**
   * Verify a webhook signature from an incoming webhook payload.
   * Uses HMAC-SHA256 matching the Xergon-Signature-256 header.
   */
  static verifySignature(payload: string | Buffer, signature: string, secret: string): boolean {
    const expected = 'sha256=' + crypto
      .createHmac('sha256', secret)
      .update(payload)
      .digest('hex');
    return crypto.timingSafeEqual(
      Buffer.from(signature),
      Buffer.from(expected),
    );
  }

  /**
   * Generate a webhook secret for signing payloads.
   */
  static generateSecret(): string {
    return crypto.randomBytes(32).toString('hex');
  }
}

// ── Convenience Functions ──────────────────────────────────────────

let defaultClient: WebhookClient | null = null;

function getClient(): WebhookClient {
  if (!defaultClient) {
    defaultClient = new WebhookClient();
  }
  return defaultClient;
}

export async function createWebhook(params: CreateWebhookParams): Promise<Webhook> {
  return getClient().createWebhook(params);
}

export async function listWebhooks(): Promise<Webhook[]> {
  return getClient().listWebhooks();
}

export async function getWebhook(id: string): Promise<Webhook> {
  return getClient().getWebhook(id);
}

export async function updateWebhook(id: string, updates: UpdateWebhookParams): Promise<Webhook> {
  return getClient().updateWebhook(id, updates);
}

export async function deleteWebhook(id: string): Promise<void> {
  return getClient().deleteWebhook(id);
}

export async function testWebhook(id: string, event?: WebhookEvent): Promise<WebhookDelivery> {
  return getClient().testWebhook(id, event);
}

export async function getDeliveries(id: string, limit?: number): Promise<WebhookDelivery[]> {
  return getClient().getDeliveries(id, limit);
}

export async function replayDelivery(deliveryId: string): Promise<WebhookDelivery> {
  return getClient().replayDelivery(deliveryId);
}

export function getSupportedEvents(): WebhookEvent[] {
  return getClient().getSupportedEvents();
}
