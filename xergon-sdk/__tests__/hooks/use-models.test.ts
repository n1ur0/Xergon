/**
 * Tests for useModels hook.
 * @vitest-environment jsdom
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { useModels } from '../../src/hooks/use-models';
import React from 'react';

describe('useModels', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  const mockModels = {
    object: 'list',
    data: [
      { id: 'llama-3.3-70b', object: 'model', ownedBy: 'meta' },
      { id: 'mistral-7b', object: 'model', ownedBy: 'mistral' },
    ],
  };

  describe('fetches models on mount', () => {
    it('calls fetch on mount by default', async () => {
      const fetchMock = vi.fn().mockResolvedValue({
        ok: true,
        json: () => Promise.resolve(mockModels),
      });
      vi.stubGlobal('fetch', fetchMock);

      const { result } = renderHook(() => useModels());

      // Wait for the effect to complete
      await act(async () => {
        await vi.waitFor(() => expect(fetchMock).toHaveBeenCalledTimes(1));
      });

      expect(fetchMock).toHaveBeenCalledTimes(1);
      expect(fetchMock).toHaveBeenCalledWith(
        'https://relay.xergon.gg/v1/models',
        expect.objectContaining({ method: 'GET' }),
      );
      expect(result.current.models).toHaveLength(2);
      expect(result.current.models[0].id).toBe('llama-3.3-70b');
    });

    it('does not fetch on mount when autoFetch is false', async () => {
      const fetchMock = vi.fn().mockResolvedValue({
        ok: true,
        json: () => Promise.resolve(mockModels),
      });
      vi.stubGlobal('fetch', fetchMock);

      renderHook(() => useModels({ autoFetch: false }));

      // Give some time for any effect to potentially fire
      await act(async () => {
        await new Promise(r => setTimeout(r, 50));
      });

      expect(fetchMock).not.toHaveBeenCalled();
    });
  });

  describe('loading state', () => {
    it('sets isLoading true while fetching', async () => {
      let resolveFetch: (v: unknown) => void;
      const pending = new Promise(resolve => { resolveFetch = resolve; });
      const fetchMock = vi.fn().mockReturnValue(pending);
      vi.stubGlobal('fetch', fetchMock);

      const { result } = renderHook(() => useModels());

      // Should be loading immediately
      expect(result.current.isLoading).toBe(true);

      // Resolve the fetch
      await act(async () => {
        resolveFetch!({
          ok: true,
          json: () => Promise.resolve(mockModels),
        });
      });

      expect(result.current.isLoading).toBe(false);
    });
  });

  describe('error handling', () => {
    it('sets error when fetch fails', async () => {
      vi.stubGlobal('fetch', vi.fn().mockRejectedValue(new Error('Network error')));

      const { result } = renderHook(() => useModels());

      // Wait for state to settle
      await act(async () => {
        await new Promise(r => setTimeout(r, 100));
      });

      expect(result.current.error).toBeInstanceOf(Error);
      expect(result.current.error!.message).toBe('Network error');
      expect(result.current.isLoading).toBe(false);
    });

    it('sets error when response is not ok', async () => {
      vi.stubGlobal('fetch', vi.fn().mockResolvedValue({
        ok: false,
        status: 500,
        statusText: 'Internal Server Error',
      }));

      const { result } = renderHook(() => useModels());

      // Wait for state to settle
      await act(async () => {
        await new Promise(r => setTimeout(r, 100));
      });

      expect(result.current.error).toBeInstanceOf(Error);
      expect(result.current.error!.message).toContain('500');
    });
  });

  describe('refetch', () => {
    it('refetches models when called', async () => {
      const fetchMock = vi.fn().mockResolvedValue({
        ok: true,
        json: () => Promise.resolve(mockModels),
      });
      vi.stubGlobal('fetch', fetchMock);

      const { result } = renderHook(() => useModels());

      // Wait for initial fetch
      await act(async () => {
        await vi.waitFor(() => expect(fetchMock).toHaveBeenCalledTimes(1));
      });

      // Refetch
      await act(async () => {
        await result.current.refetch();
      });

      expect(fetchMock).toHaveBeenCalledTimes(2);
    });
  });

  describe('baseUrl and apiKey', () => {
    it('uses custom baseUrl', async () => {
      const fetchMock = vi.fn().mockResolvedValue({
        ok: true,
        json: () => Promise.resolve(mockModels),
      });
      vi.stubGlobal('fetch', fetchMock);

      const { result } = renderHook(() => useModels({
        baseUrl: 'https://custom.relay.gg',
      }));

      await act(async () => {
        await vi.waitFor(() => expect(fetchMock).toHaveBeenCalledTimes(1));
      });

      expect(fetchMock).toHaveBeenCalledWith(
        'https://custom.relay.gg/v1/models',
        expect.objectContaining({ method: 'GET' }),
      );
    });

    it('includes Authorization header when apiKey is provided', async () => {
      const fetchMock = vi.fn().mockResolvedValue({
        ok: true,
        json: () => Promise.resolve(mockModels),
      });
      vi.stubGlobal('fetch', fetchMock);

      const { result } = renderHook(() => useModels({
        apiKey: 'test-key-123',
      }));

      await act(async () => {
        await vi.waitFor(() => expect(fetchMock).toHaveBeenCalledTimes(1));
      });

      expect(fetchMock).toHaveBeenCalledWith(
        expect.any(String),
        expect.objectContaining({
          headers: expect.objectContaining({
            Authorization: 'Bearer test-key-123',
          }),
        }),
      );
    });
  });
});
