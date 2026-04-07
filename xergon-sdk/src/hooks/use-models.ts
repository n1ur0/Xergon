/**
 * useModels -- React hook for fetching available models.
 */

import { useState, useCallback, useEffect } from 'react';
import type { Model } from '../types';

export interface UseModelsOptions {
  /** Base URL of the Xergon relay. */
  baseUrl?: string;
  /** API key for authentication. */
  apiKey?: string;
  /** Whether to fetch models on mount (default: true). */
  autoFetch?: boolean;
}

export function useModels(options: UseModelsOptions = {}) {
  const baseUrl = options.baseUrl || 'https://relay.xergon.gg';
  const apiKey = options.apiKey;
  const autoFetch = options.autoFetch ?? true;

  const [models, setModels] = useState<Model[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<Error | null>(null);

  const fetchModels = useCallback(async () => {
    setIsLoading(true);
    setError(null);

    try {
      const headers: Record<string, string> = {};
      if (apiKey) {
        headers['Authorization'] = `Bearer ${apiKey}`;
      }

      const res = await fetch(`${baseUrl}/v1/models`, {
        method: 'GET',
        headers,
      });

      if (!res.ok) {
        throw new Error(`Failed to fetch models (${res.status})`);
      }

      const data = await res.json();
      setModels(data.data ?? data ?? []);
    } catch (err) {
      const error = err instanceof Error ? err : new Error(String(err));
      setError(error);
    } finally {
      setIsLoading(false);
    }
  }, [baseUrl, apiKey]);

  useEffect(() => {
    if (autoFetch) {
      fetchModels();
    }
  }, [autoFetch, fetchModels]);

  return { models, isLoading, error, refetch: fetchModels } as const;
}
