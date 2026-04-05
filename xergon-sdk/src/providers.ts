/**
 * Provider discovery and leaderboard methods.
 */

import type { Provider, LeaderboardEntry } from './types';
import { XergonClientCore } from './client';

/**
 * List all active providers.
 */
export async function listProviders(
  client: XergonClientCore,
): Promise<Provider[]> {
  return client.get<Provider[]>('/v1/providers');
}

/**
 * Get the provider leaderboard ranked by PoNW score.
 */
export async function getLeaderboard(
  client: XergonClientCore,
  params?: { limit?: number; offset?: number },
): Promise<LeaderboardEntry[]> {
  const searchParams = new URLSearchParams();
  if (params?.limit != null) searchParams.set('limit', String(params.limit));
  if (params?.offset != null) searchParams.set('offset', String(params.offset));
  const qs = searchParams.toString();
  return client.get<LeaderboardEntry[]>(
    `/v1/leaderboard${qs ? `?${qs}` : ''}`,
  );
}
