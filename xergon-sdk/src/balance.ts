/**
 * Balance checking methods.
 */

import type { BalanceResponse } from './types';
import { XergonClientCore } from './client';

/**
 * Get user's ERG balance from their on-chain Staking Box.
 */
export async function getBalance(
  client: XergonClientCore,
  userPk: string,
): Promise<BalanceResponse> {
  return client.get<BalanceResponse>(
    `/v1/balance/${encodeURIComponent(userPk)}`,
  );
}
