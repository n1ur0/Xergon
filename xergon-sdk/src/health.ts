/**
 * Health and readiness probes.
 */

import { XergonClientCore } from './client';

/**
 * Check if the relay process is running (liveness probe).
 */
export async function healthCheck(
  client: XergonClientCore,
): Promise<boolean> {
  try {
    const res = await client.get<string>('/health', { skipAuth: true });
    return res.trim().toUpperCase() === 'OK';
  } catch {
    return false;
  }
}

/**
 * Check if the relay can serve requests (readiness probe).
 * Returns true only when the chain scanner is healthy and providers are available.
 */
export async function readyCheck(
  client: XergonClientCore,
): Promise<boolean> {
  try {
    const res = await client.get<string>('/ready', { skipAuth: true });
    return res.trim().toUpperCase() === 'OK';
  } catch {
    return false;
  }
}
