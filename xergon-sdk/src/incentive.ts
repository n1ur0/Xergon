/**
 * Incentive system -- rare model bonuses.
 */

import type { IncentiveStatus, RareModel, RareModelDetail } from './types';
import { XergonClientCore } from './client';

/**
 * Get the incentive system status.
 */
export async function getIncentiveStatus(
  client: XergonClientCore,
): Promise<IncentiveStatus> {
  return client.get<IncentiveStatus>('/v1/incentive/status');
}

/**
 * Get all rare models with bonus information.
 */
export async function getIncentiveModels(
  client: XergonClientCore,
): Promise<RareModel[]> {
  return client.get<RareModel[]>('/v1/incentive/models');
}

/**
 * Get detailed rarity information for a specific model.
 */
export async function getIncentiveModelDetail(
  client: XergonClientCore,
  model: string,
): Promise<RareModelDetail> {
  return client.get<RareModelDetail>(
    `/v1/incentive/models/${encodeURIComponent(model)}`,
  );
}
