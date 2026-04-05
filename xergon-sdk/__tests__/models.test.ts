/**
 * Tests for model listing.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { XergonClient } from '../src/index';

describe('Models', () => {
  const mockModelsResponse = {
    object: 'list',
    data: [
      {
        id: 'llama-3.3-70b',
        object: 'model',
        ownedBy: 'meta',
        pricing: '0.001 ERG/1K tokens',
      },
      {
        id: 'mistral-7b',
        object: 'model',
        ownedBy: 'mistral-ai',
      },
      {
        id: 'deepseek-coder-33b',
        object: 'model',
        ownedBy: 'deepseek',
        pricing: '0.002 ERG/1K tokens',
      },
    ],
  };

  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it('returns a list of models on success', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue({
      ok: true,
      status: 200,
      headers: new Headers({ 'content-type': 'application/json' }),
      json: () => Promise.resolve(mockModelsResponse),
    }));

    const client = new XergonClient();
    const models = await client.models.list();

    expect(models).toHaveLength(3);
    expect(models[0].id).toBe('llama-3.3-70b');
    expect(models[0].object).toBe('model');
    expect(models[0].ownedBy).toBe('meta');
    expect(models[1].id).toBe('mistral-7b');
    expect(models[2].id).toBe('deepseek-coder-33b');
  });

  it('returns models in correct format with optional fields', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue({
      ok: true,
      status: 200,
      headers: new Headers({ 'content-type': 'application/json' }),
      json: () => Promise.resolve(mockModelsResponse),
    }));

    const client = new XergonClient();
    const models = await client.models.list();

    // First model has pricing
    expect(models[0].pricing).toBe('0.001 ERG/1K tokens');
    // Second model does not have pricing
    expect(models[1].pricing).toBeUndefined();
    // Third model has pricing
    expect(models[2].pricing).toBe('0.002 ERG/1K tokens');
  });

  it('handles empty model list', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue({
      ok: true,
      status: 200,
      headers: new Headers({ 'content-type': 'application/json' }),
      json: () => Promise.resolve({ object: 'list', data: [] }),
    }));

    const client = new XergonClient();
    const models = await client.models.list();

    expect(models).toHaveLength(0);
    expect(Array.isArray(models)).toBe(true);
  });

  it('makes GET request to /v1/models', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue({
      ok: true,
      status: 200,
      headers: new Headers({ 'content-type': 'application/json' }),
      json: () => Promise.resolve(mockModelsResponse),
    }));

    const client = new XergonClient();
    await client.models.list();

    expect(fetch).toHaveBeenCalledTimes(1);
    const call = (fetch as ReturnType<typeof vi.fn>).mock.calls[0];
    const url = call[0] as string;
    const options = call[1] as RequestInit;

    expect(url).toContain('/v1/models');
    expect(options.method).toBe('GET');
  });
});
