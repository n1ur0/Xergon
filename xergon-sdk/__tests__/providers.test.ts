/**
 * Tests for provider listing and leaderboard.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { XergonClient } from '../src/index';

describe('Providers', () => {
  const mockProviders = [
    {
      publicKey: '0xprovider1',
      endpoint: 'https://node1.example.com',
      models: ['llama-3.3-70b', 'mistral-7b'],
      region: 'us-east',
      pownScore: 95.5,
      lastHeartbeat: 1700000000,
      pricing: { 'llama-3.3-70b': '0.001 ERG/1K tokens' },
    },
    {
      publicKey: '0xprovider2',
      endpoint: 'https://node2.example.com',
      models: ['deepseek-coder-33b'],
      region: 'eu-west',
      pownScore: 88.2,
      pricing: {},
    },
    {
      publicKey: '0xprovider3',
      endpoint: 'https://node3.example.com',
      models: ['llama-3.3-70b', 'mistral-7b', 'deepseek-coder-33b'],
      region: 'ap-southeast',
      pownScore: 76.0,
      lastHeartbeat: 1699999000,
    },
  ];

  const mockLeaderboard = [
    {
      publicKey: '0xprovider1',
      endpoint: 'https://node1.example.com',
      models: ['llama-3.3-70b', 'mistral-7b'],
      region: 'us-east',
      pownScore: 95.5,
      online: true,
      totalRequests: 15000,
      totalPromptTokens: 3000000,
      totalCompletionTokens: 6000000,
      totalTokens: 9000000,
    },
    {
      publicKey: '0xprovider2',
      endpoint: 'https://node2.example.com',
      models: ['deepseek-coder-33b'],
      region: 'eu-west',
      pownScore: 88.2,
      online: true,
      totalRequests: 8000,
    },
  ];

  beforeEach(() => {
    vi.restoreAllMocks();
  });

  describe('list', () => {
    it('returns a list of providers on success', async () => {
      vi.stubGlobal('fetch', vi.fn().mockResolvedValue({
        ok: true,
        status: 200,
        headers: new Headers({ 'content-type': 'application/json' }),
        json: () => Promise.resolve(mockProviders),
      }));

      const client = new XergonClient();
      const providers = await client.providers.list();

      expect(providers).toHaveLength(3);
      expect(providers[0].publicKey).toBe('0xprovider1');
      expect(providers[0].endpoint).toBe('https://node1.example.com');
      expect(providers[0].models).toEqual(['llama-3.3-70b', 'mistral-7b']);
      expect(providers[0].region).toBe('us-east');
      expect(providers[0].pownScore).toBe(95.5);
    });

    it('handles empty provider list', async () => {
      vi.stubGlobal('fetch', vi.fn().mockResolvedValue({
        ok: true,
        status: 200,
        headers: new Headers({ 'content-type': 'application/json' }),
        json: () => Promise.resolve([]),
      }));

      const client = new XergonClient();
      const providers = await client.providers.list();

      expect(providers).toHaveLength(0);
      expect(Array.isArray(providers)).toBe(true);
    });

    it('makes GET request to /v1/providers', async () => {
      vi.stubGlobal('fetch', vi.fn().mockResolvedValue({
        ok: true,
        status: 200,
        headers: new Headers({ 'content-type': 'application/json' }),
        json: () => Promise.resolve(mockProviders),
      }));

      const client = new XergonClient();
      await client.providers.list();

      expect(fetch).toHaveBeenCalledTimes(1);
      const call = (fetch as ReturnType<typeof vi.fn>).mock.calls[0];
      const url = call[0] as string;
      const options = call[1] as RequestInit;

      expect(url).toContain('/v1/providers');
      expect(options.method).toBe('GET');
    });

    it('returns providers with optional fields', async () => {
      vi.stubGlobal('fetch', vi.fn().mockResolvedValue({
        ok: true,
        status: 200,
        headers: new Headers({ 'content-type': 'application/json' }),
        json: () => Promise.resolve(mockProviders),
      }));

      const client = new XergonClient();
      const providers = await client.providers.list();

      // Provider 1 has lastHeartbeat and pricing
      expect(providers[0].lastHeartbeat).toBe(1700000000);
      expect(providers[0].pricing).toEqual({ 'llama-3.3-70b': '0.001 ERG/1K tokens' });

      // Provider 2 has no lastHeartbeat but has pricing (empty)
      expect(providers[1].lastHeartbeat).toBeUndefined();
      expect(providers[1].pricing).toEqual({});

      // Provider 3 has no pricing
      expect(providers[2].pricing).toBeUndefined();
    });
  });

  describe('leaderboard', () => {
    it('returns leaderboard entries', async () => {
      vi.stubGlobal('fetch', vi.fn().mockResolvedValue({
        ok: true,
        status: 200,
        headers: new Headers({ 'content-type': 'application/json' }),
        json: () => Promise.resolve(mockLeaderboard),
      }));

      const client = new XergonClient();
      const leaderboard = await client.leaderboard();

      expect(leaderboard).toHaveLength(2);
      expect(leaderboard[0].publicKey).toBe('0xprovider1');
      expect(leaderboard[0].online).toBe(true);
      expect(leaderboard[0].totalRequests).toBe(15000);
      expect(leaderboard[1].publicKey).toBe('0xprovider2');
    });

    it('passes limit and offset as query parameters', async () => {
      vi.stubGlobal('fetch', vi.fn().mockResolvedValue({
        ok: true,
        status: 200,
        headers: new Headers({ 'content-type': 'application/json' }),
        json: () => Promise.resolve(mockLeaderboard),
      }));

      const client = new XergonClient();
      await client.leaderboard({ limit: 10, offset: 20 });

      expect(fetch).toHaveBeenCalledTimes(1);
      const call = (fetch as ReturnType<typeof vi.fn>).mock.calls[0];
      const url = call[0] as string;

      expect(url).toContain('/v1/leaderboard');
      expect(url).toContain('limit=10');
      expect(url).toContain('offset=20');
    });

    it('works without pagination parameters', async () => {
      vi.stubGlobal('fetch', vi.fn().mockResolvedValue({
        ok: true,
        status: 200,
        headers: new Headers({ 'content-type': 'application/json' }),
        json: () => Promise.resolve([]),
      }));

      const client = new XergonClient();
      await client.leaderboard();

      const call = (fetch as ReturnType<typeof vi.fn>).mock.calls[0];
      const url = call[0] as string;

      expect(url).toContain('/v1/leaderboard');
      expect(url).not.toContain('limit=');
      expect(url).not.toContain('offset=');
    });

    it('handles empty leaderboard', async () => {
      vi.stubGlobal('fetch', vi.fn().mockResolvedValue({
        ok: true,
        status: 200,
        headers: new Headers({ 'content-type': 'application/json' }),
        json: () => Promise.resolve([]),
      }));

      const client = new XergonClient();
      const leaderboard = await client.leaderboard();

      expect(leaderboard).toHaveLength(0);
      expect(Array.isArray(leaderboard)).toBe(true);
    });
  });
});
