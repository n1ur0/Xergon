/**
 * Model listing methods.
 */

import type { Model, ModelsResponse } from './types';
import { XergonClientCore } from './client';

/**
 * List all available models from active providers.
 */
export async function listModels(
  client: XergonClientCore,
): Promise<Model[]> {
  const res = await client.get<ModelsResponse>('/v1/models');
  return res.data;
}
