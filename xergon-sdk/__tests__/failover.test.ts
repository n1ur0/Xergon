/**
 * Tests for FailoverProviderManager -- multi-provider failover, circuit breaker, health checks.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import {
  FailoverProviderManager,
  AllEndpointsFailedError,
} from '../src/providers/failover';
import type { ProviderEndpoint } from '../src/providers/failover';

// ── Helpers ──────────────────────────────────────────────────────────

function makeEndpoint(overrides: Partial<ProviderEndpoint> & { url: string }): ProviderEndpoint {
  return {
    priority: 1,
    region: 'us-east',
    maxRetries: 3,
    timeoutMs: 5000,
    ...overrides,
  };
}

function mockFetchOk(data: unknown, url?: string): ReturnType<typeof fetch> {
  return Promise.resolve({
    ok: true,
    status: 200,
    headers: new Headers({ 'content-type': 'application/json' }),
    json: () => Promise.resolve(data),
    text: () => Promise.resolve(JSON.stringify(data)),
    body: null,
  } as Response);
}

function mockFetchError(status: number, message: string): ReturnType<typeof fetch> {
  return Promise.resolve({
    ok: false,
    status,
    statusText: message,
    headers: new Headers(),
    json: () => Promise.resolve({ error: { type: 'internal_error', message, code: status } }),
    text: () => Promise.resolve(message),
    body: null,
  } as Response);
}

function mockFetchThrow(err: Error): typeof fetch {
  return (() => Promise.reject(err)) as unknown as typeof fetch;
}

// ── Tests ────────────────────────────────────────────────────────────

describe('FailoverProviderManager', () => {
  let fetchMock: ReturnType<typeof vi.fn>;
  let manager: FailoverProviderManager;

  beforeEach(() => {
    vi.useFakeTimers();
    fetchMock = vi.fn();
  });

  afterEach(() => {
    vi.useRealTimers();
    if (manager) manager.destroy();
  });

  describe('basic failover', () => {
    it('uses primary endpoint when healthy', async () => {
      fetchMock.mockResolvedValue(mockFetchOk({ status: 'ok' }));

      manager = new FailoverProviderManager(
        [
          makeEndpoint({ url: 'https://primary.example.com', priority: 1 }),
          makeEndpoint({ url: 'https://secondary.example.com', priority: 2 }),
        ],
        { fetchFn: fetchMock as unknown as typeof fetch, healthCheckIntervalMs: 0 },
      );

      const result = await manager.request('GET', '/v1/models');
      expect(result).toEqual({ status: 'ok' });
      expect(fetchMock).toHaveBeenCalledTimes(1);
      expect(fetchMock).toHaveBeenCalledWith(
        'https://primary.example.com/v1/models',
        expect.objectContaining({ method: 'GET' }),
      );
    });

    it('falls back to secondary when primary fails', async () => {
      fetchMock
        .mockRejectedValueOnce(new Error('Connection refused'))
        .mockResolvedValueOnce(mockFetchOk({ status: 'ok' }));

      manager = new FailoverProviderManager(
        [
          makeEndpoint({ url: 'https://primary.example.com', priority: 1 }),
          makeEndpoint({ url: 'https://secondary.example.com', priority: 2 }),
        ],
        { fetchFn: fetchMock as unknown as typeof fetch, healthCheckIntervalMs: 0 },
      );

      const result = await manager.request('GET', '/v1/models');
      expect(result).toEqual({ status: 'ok' });
      expect(fetchMock).toHaveBeenCalledTimes(2);
      expect(fetchMock).toHaveBeenNthCalledWith(
        2,
        'https://secondary.example.com/v1/models',
        expect.objectContaining({ method: 'GET' }),
      );
    });

    it('sends body as JSON when provided', async () => {
      fetchMock.mockResolvedValue(mockFetchOk({ id: '1' }));

      manager = new FailoverProviderManager(
        [makeEndpoint({ url: 'https://primary.example.com', priority: 1 })],
        { fetchFn: fetchMock as unknown as typeof fetch, healthCheckIntervalMs: 0 },
      );

      await manager.request('POST', '/v1/chat/completions', { model: 'llama-3.3-70b', messages: [] });
      expect(fetchMock).toHaveBeenCalledWith(
        'https://primary.example.com/v1/chat/completions',
        expect.objectContaining({
          method: 'POST',
          body: JSON.stringify({ model: 'llama-3.3-70b', messages: [] }),
        }),
      );
    });
  });

  describe('circuit breaker', () => {
    it('marks endpoint unhealthy after threshold failures', async () => {
      fetchMock.mockRejectedValue(new Error('Connection refused'));

      manager = new FailoverProviderManager(
        [
          makeEndpoint({ url: 'https://primary.example.com', priority: 1 }),
          makeEndpoint({ url: 'https://secondary.example.com', priority: 2 }),
        ],
        {
          fetchFn: fetchMock as unknown as typeof fetch,
          healthCheckIntervalMs: 0,
          circuitBreakerThreshold: 2,
        },
      );

      // First request: primary fails, secondary fails => both get 1 failure
      await expect(manager.request('GET', '/test')).rejects.toThrow(AllEndpointsFailedError);
      expect(fetchMock).toHaveBeenCalledTimes(2);

      // Second request: both still healthy (1 failure < 2 threshold)
      fetchMock.mockClear();
      await expect(manager.request('GET', '/test')).rejects.toThrow(AllEndpointsFailedError);
      expect(fetchMock).toHaveBeenCalledTimes(2); // Still tries both

      // Third request: primary now has 2 failures (circuit opens), secondary has 2 failures too
      fetchMock.mockClear();
      await expect(manager.request('GET', '/test')).rejects.toThrow(AllEndpointsFailedError);
      expect(fetchMock).toHaveBeenCalledTimes(0); // Both circuits are open, neither tried

      const endpoints = manager.getEndpoints();
      expect(endpoints).toHaveLength(2);
      expect(endpoints[0].healthy).toBe(false);
      expect(endpoints[0].consecutiveFailures).toBeGreaterThanOrEqual(2);
      expect(endpoints[1].healthy).toBe(false);
    });

    it('marks endpoint unhealthy manually', () => {
      manager = new FailoverProviderManager(
        [makeEndpoint({ url: 'https://primary.example.com', priority: 1 })],
        { fetchFn: fetchMock as unknown as typeof fetch, healthCheckIntervalMs: 0 },
      );

      manager.markUnhealthy('https://primary.example.com', 'manual override');

      const endpoints = manager.getEndpoints();
      expect(endpoints[0].healthy).toBe(false);
      expect(endpoints[0].lastError).toBe('manual override');
    });

    it('resets all endpoints to healthy', () => {
      manager = new FailoverProviderManager(
        [
          makeEndpoint({ url: 'https://primary.example.com', priority: 1 }),
          makeEndpoint({ url: 'https://secondary.example.com', priority: 2 }),
        ],
        { fetchFn: fetchMock as unknown as typeof fetch, healthCheckIntervalMs: 0 },
      );

      manager.markUnhealthy('https://primary.example.com', 'test');
      manager.markUnhealthy('https://secondary.example.com', 'test');

      expect(manager.getEndpoints().every((e) => !e.healthy)).toBe(true);

      manager.resetAll();
      expect(manager.getEndpoints().every((e) => e.healthy)).toBe(true);
      expect(manager.getEndpoints().every((e) => e.consecutiveFailures === 0)).toBe(true);
    });
  });

  describe('health check recovery', () => {
    it('recovers unhealthy endpoint on successful health check', async () => {
      // Health checks fail initially
      fetchMock.mockRejectedValue(new Error('Connection refused'));

      manager = new FailoverProviderManager(
        [makeEndpoint({ url: 'https://primary.example.com', priority: 1 })],
        { fetchFn: fetchMock as unknown as typeof fetch, healthCheckIntervalMs: 0, circuitBreakerThreshold: 1 },
      );

      // Make endpoint unhealthy
      manager.markUnhealthy('https://primary.example.com', 'manual');
      expect(manager.getEndpoints()[0].healthy).toBe(false);

      // Health check succeeds now
      fetchMock.mockResolvedValue({
        ok: true,
        status: 200,
        headers: new Headers(),
        json: () => Promise.resolve('OK'),
        text: () => Promise.resolve('OK'),
        body: null,
      } as Response);

      await manager.healthCheck();

      expect(manager.getEndpoints()[0].healthy).toBe(true);
      expect(manager.getEndpoints()[0].consecutiveFailures).toBe(0);
    });

    it('returns health map from healthCheck()', async () => {
      fetchMock.mockResolvedValue({
        ok: true,
        status: 200,
        headers: new Headers(),
        json: () => Promise.resolve('OK'),
        text: () => Promise.resolve('OK'),
        body: null,
      } as Response);

      manager = new FailoverProviderManager(
        [
          makeEndpoint({ url: 'https://primary.example.com', priority: 1 }),
          makeEndpoint({ url: 'https://secondary.example.com', priority: 2 }),
        ],
        { fetchFn: fetchMock as unknown as typeof fetch, healthCheckIntervalMs: 0 },
      );

      const health = await manager.healthCheck();
      expect(health.size).toBe(2);
      expect(health.get('https://primary.example.com')?.healthy).toBe(true);
      expect(health.get('https://secondary.example.com')?.healthy).toBe(true);
    });
  });

  describe('all endpoints fail', () => {
    it('throws AllEndpointsFailedError with all error details', async () => {
      fetchMock.mockRejectedValue(new Error('Network error'));

      manager = new FailoverProviderManager(
        [
          makeEndpoint({ url: 'https://ep1.example.com', priority: 1 }),
          makeEndpoint({ url: 'https://ep2.example.com', priority: 2 }),
          makeEndpoint({ url: 'https://ep3.example.com', priority: 3 }),
        ],
        { fetchFn: fetchMock as unknown as typeof fetch, healthCheckIntervalMs: 0 },
      );

      try {
        await manager.request('GET', '/test');
        expect.fail('Should have thrown');
      } catch (err) {
        expect(err).toBeInstanceOf(AllEndpointsFailedError);
        const allErr = err as AllEndpointsFailedError;
        expect(allErr.errors).toHaveLength(3);
        expect(allErr.errors[0].url).toBe('https://ep1.example.com');
        expect(allErr.errors[1].url).toBe('https://ep2.example.com');
        expect(allErr.errors[2].url).toBe('https://ep3.example.com');
      }
    });
  });

  describe('priority ordering', () => {
    it('tries endpoints in priority order (lowest number first)', async () => {
      fetchMock
        .mockRejectedValueOnce(new Error('ep3 down'))
        .mockRejectedValueOnce(new Error('ep1 down'))
        .mockResolvedValueOnce(mockFetchOk({ result: 'from ep2' }));

      manager = new FailoverProviderManager(
        [
          makeEndpoint({ url: 'https://ep3.example.com', priority: 3 }),
          makeEndpoint({ url: 'https://ep1.example.com', priority: 1 }),
          makeEndpoint({ url: 'https://ep2.example.com', priority: 2 }),
        ],
        { fetchFn: fetchMock as unknown as typeof fetch, healthCheckIntervalMs: 0 },
      );

      const result = await manager.request('GET', '/test');
      expect(result).toEqual({ result: 'from ep2' });

      // Verify call order: ep1 (priority 1), ep2 (priority 2), ep3 (priority 3)
      expect(fetchMock).toHaveBeenCalledTimes(3);
      expect(fetchMock.mock.calls[0][0]).toBe('https://ep1.example.com/test');
      expect(fetchMock.mock.calls[1][0]).toBe('https://ep2.example.com/test');
      expect(fetchMock.mock.calls[2][0]).toBe('https://ep3.example.com/test');
    });

    it('getBestEndpoint returns lowest priority healthy endpoint', async () => {
      fetchMock.mockResolvedValue({
        ok: true,
        status: 200,
        headers: new Headers(),
        json: () => Promise.resolve('OK'),
        text: () => Promise.resolve('OK'),
        body: null,
      } as Response);

      manager = new FailoverProviderManager(
        [
          makeEndpoint({ url: 'https://ep3.example.com', priority: 3 }),
          makeEndpoint({ url: 'https://ep1.example.com', priority: 1 }),
          makeEndpoint({ url: 'https://ep2.example.com', priority: 2 }),
        ],
        { fetchFn: fetchMock as unknown as typeof fetch, healthCheckIntervalMs: 0 },
      );

      const best = await manager.getBestEndpoint();
      expect(best.url).toBe('https://ep1.example.com');
      expect(best.priority).toBe(1);
    });

    it('getBestEndpoint skips unhealthy and returns next best', async () => {
      fetchMock.mockResolvedValue({
        ok: true,
        status: 200,
        headers: new Headers(),
        json: () => Promise.resolve('OK'),
        text: () => Promise.resolve('OK'),
        body: null,
      } as Response);

      manager = new FailoverProviderManager(
        [
          makeEndpoint({ url: 'https://ep1.example.com', priority: 1 }),
          makeEndpoint({ url: 'https://ep2.example.com', priority: 2 }),
        ],
        { fetchFn: fetchMock as unknown as typeof fetch, healthCheckIntervalMs: 0 },
      );

      manager.markUnhealthy('https://ep1.example.com', 'down');

      const best = await manager.getBestEndpoint();
      expect(best.url).toBe('https://ep2.example.com');
      expect(best.priority).toBe(2);
    });
  });

  describe('getEndpoints', () => {
    it('returns all endpoints with status info', () => {
      manager = new FailoverProviderManager(
        [
          makeEndpoint({ url: 'https://ep1.example.com', priority: 1, region: 'us-east' }),
          makeEndpoint({ url: 'https://ep2.example.com', priority: 2, region: 'eu-west' }),
        ],
        { fetchFn: fetchMock as unknown as typeof fetch, healthCheckIntervalMs: 0 },
      );

      const endpoints = manager.getEndpoints();
      expect(endpoints).toHaveLength(2);
      expect(endpoints[0]).toMatchObject({
        url: 'https://ep1.example.com',
        priority: 1,
        region: 'us-east',
        healthy: true,
      });
      expect(endpoints[1]).toMatchObject({
        url: 'https://ep2.example.com',
        priority: 2,
        region: 'eu-west',
        healthy: true,
      });
    });
  });

  describe('custom headers', () => {
    it('includes custom headers on requests', async () => {
      fetchMock.mockResolvedValue(mockFetchOk({ ok: true }));

      manager = new FailoverProviderManager(
        [makeEndpoint({ url: 'https://primary.example.com', priority: 1 })],
        {
          fetchFn: fetchMock as unknown as typeof fetch,
          healthCheckIntervalMs: 0,
          headers: { 'X-Custom': 'value' },
        },
      );

      await manager.request('GET', '/test');
      expect(fetchMock).toHaveBeenCalledWith(
        expect.any(String),
        expect.objectContaining({
          headers: expect.objectContaining({ 'X-Custom': 'value' }),
        }),
      );
    });
  });

  describe('destroy', () => {
    it('stops background health checks', () => {
      manager = new FailoverProviderManager(
        [makeEndpoint({ url: 'https://primary.example.com', priority: 1 })],
        { fetchFn: fetchMock as unknown as typeof fetch, healthCheckIntervalMs: 100 },
      );

      // Should not throw
      manager.destroy();

      // Calling destroy again should be safe
      manager.destroy();
    });
  });
});
