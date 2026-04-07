/**
 * useProvider -- React hook for checking relay provider status and latency.
 */

import { useState, useCallback, useEffect, useRef } from 'react';

export type ProviderStatus = 'connected' | 'disconnected' | 'checking';

export interface UseProviderOptions {
  /** Base URL of the Xergon relay. */
  baseUrl?: string;
  /** API key for authentication. */
  apiKey?: string;
  /** Health check interval in ms (default: 30000, 0 = disabled). */
  checkIntervalMs?: number;
  /** Whether to check on mount (default: true). */
  autoCheck?: boolean;
}

export interface ProviderInfo {
  status: string;
  version?: string;
  uptimeSecs?: number;
  ergoNodeConnected?: boolean;
  activeProviders?: number;
  totalProviders?: number;
}

export function useProvider(options: UseProviderOptions = {}) {
  const baseUrl = options.baseUrl || 'https://relay.xergon.gg';
  const apiKey = options.apiKey;
  const checkIntervalMs = options.checkIntervalMs ?? 30000;
  const autoCheck = options.autoCheck ?? true;

  const [status, setStatus] = useState<ProviderStatus>('checking');
  const [latency, setLatency] = useState<number | null>(null);
  const [providerInfo, setProviderInfo] = useState<ProviderInfo | null>(null);
  const [error, setError] = useState<Error | null>(null);
  const intervalRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const check = useCallback(async () => {
    setStatus('checking');

    try {
      const headers: Record<string, string> = {};
      if (apiKey) {
        headers['Authorization'] = `Bearer ${apiKey}`;
      }

      const start = performance.now();
      const res = await fetch(`${baseUrl}/health`, {
        method: 'GET',
        headers,
      });
      const end = performance.now();

      const latencyMs = Math.round(end - start);
      setLatency(latencyMs);
      setError(null);

      if (res.ok) {
        setStatus('connected');
        try {
          const info = await res.json();
          setProviderInfo(info);
        } catch {
          // Health endpoint may return plain text "OK"
          setProviderInfo({ status: 'ok' });
        }
      } else {
        setStatus('disconnected');
      }
    } catch (err) {
      setStatus('disconnected');
      setLatency(null);
      const error = err instanceof Error ? err : new Error(String(err));
      setError(error);
    }
  }, [baseUrl, apiKey]);

  useEffect(() => {
    if (autoCheck) {
      check();
    }

    if (checkIntervalMs > 0) {
      intervalRef.current = setInterval(check, checkIntervalMs);
    }

    return () => {
      if (intervalRef.current) {
        clearInterval(intervalRef.current);
        intervalRef.current = null;
      }
    };
  }, [autoCheck, check, checkIntervalMs]);

  return {
    status,
    latency,
    providerInfo,
    error,
    check,
  } as const;
}
