/**
 * Model Registry -- rich model discovery, comparison, recommendation,
 * versioning, and lineage for the Xergon Network.
 *
 * Extends the basic model listing with filtering, search, benchmarks,
 * deprecation tracking, and popularity rankings.
 */

import { XergonClientCore } from './client';
import type { Model, ModelsResponse } from './types';

// ── Types ──────────────────────────────────────────────────────────

export interface ModelInfo {
  id: string;
  name: string;
  provider: string;
  providerAddress: string;
  task: string;
  description: string;
  pricing: { perToken: number; perRequest?: number; currency: string };
  benchmarks: Record<string, number>;
  quantization?: string;
  contextLength: number;
  maxOutputTokens: number;
  status: 'active' | 'inactive' | 'deprecated';
  tags: string[];
  createdAt: string;
  updatedAt: string;
}

export interface ModelVersion {
  version: string;
  modelId: string;
  changelog: string;
  publishedAt: string;
  deprecated: boolean;
  migrationNotes?: string;
}

export interface ModelFilter {
  task?: string;
  provider?: string;
  status?: 'active' | 'inactive' | 'deprecated';
  quantization?: string;
  tags?: string[];
  minContextLength?: number;
  maxPricePerToken?: number;
}

export interface SortOption {
  field: string;
  direction: 'asc' | 'desc';
}

export interface PaginationOptions {
  offset?: number;
  limit?: number;
}

export interface ModelComparison {
  model1: Partial<ModelInfo>;
  model2: Partial<ModelInfo>;
  differences: Array<{ field: string; left: unknown; right: unknown }>;
  recommendation?: string;
}

export interface ModelRecommendation {
  modelId: string;
  score: number;
  reason: string;
  estimatedCost?: number;
}

export interface LineageNode {
  modelId: string;
  relationship: 'parent' | 'child' | 'fork' | 'successor';
  version?: string;
}

// ── In-memory registry cache ───────────────────────────────────────

const registryCache = new Map<string, ModelInfo>();
const versionHistory = new Map<string, ModelVersion[]>();
const popularityScores = new Map<string, number>();
const lineageMap = new Map<string, LineageNode[]>();
const subscribers = new Map<string, Array<(model: ModelInfo) => void>>();

/**
 * Enrich a basic Model into a full ModelInfo with defaults.
 */
function enrichModel(model: Model): ModelInfo {
  const cached = registryCache.get(model.id);
  if (cached) return cached;

  const info: ModelInfo = {
    id: model.id,
    name: model.id,
    provider: model.ownedBy ?? 'unknown',
    providerAddress: '',
    task: guessTask(model.id),
    description: '',
    pricing: { perToken: 0, currency: 'ERG' },
    benchmarks: {},
    quantization: guessQuantization(model.id),
    contextLength: 4096,
    maxOutputTokens: 2048,
    status: 'active',
    tags: extractTags(model.id, model.ownedBy),
    createdAt: new Date().toISOString(),
    updatedAt: new Date().toISOString(),
  };

  // Parse pricing from the string if available
  if (model.pricing) {
    const match = model.pricing.match(/([\d.]+)/);
    if (match) {
      info.pricing.perToken = parseFloat(match[1]);
    }
  }

  return info;
}

function guessTask(modelId: string): string {
  const lower = modelId.toLowerCase();
  if (lower.includes('embed')) return 'embedding';
  if (lower.includes('vision') || lower.includes('vl')) return 'vision';
  if (lower.includes('code') || lower.includes('coder')) return 'code';
  if (lower.includes('instruct') || lower.includes('chat')) return 'chat';
  if (lower.includes('tts') || lower.includes('speech')) return 'text-to-speech';
  if (lower.includes('stt') || lower.includes('whisper') || lower.includes('transcri')) return 'speech-to-text';
  return 'general';
}

function guessQuantization(modelId: string): string | undefined {
  const lower = modelId.toLowerCase();
  if (lower.includes('q4') || lower.includes('4bit')) return '4-bit';
  if (lower.includes('q8') || lower.includes('8bit')) return '8-bit';
  if (lower.includes('fp16') || lower.includes('half')) return 'fp16';
  if (lower.includes('fp32') || lower.includes('full')) return 'fp32';
  if (lower.includes('int8')) return 'int8';
  if (lower.includes('int4')) return 'int4';
  if (lower.includes('gguf') || lower.includes('ggml')) return 'gguf';
  return undefined;
}

function extractTags(modelId: string, owner: string): string[] {
  const tags: string[] = [];
  const lower = modelId.toLowerCase();

  if (lower.includes('llama')) tags.push('llama');
  if (lower.includes('mistral')) tags.push('mistral');
  if (lower.includes('mixtral')) tags.push('mixtral');
  if (lower.includes('phi')) tags.push('phi');
  if (lower.includes('qwen')) tags.push('qwen');
  if (lower.includes('deepseek')) tags.push('deepseek');
  if (lower.includes('gemma')) tags.push('gemma');
  if (lower.includes('codellama') || lower.includes('code-llama')) tags.push('code', 'llama');
  if (lower.includes('70b')) tags.push('70b');
  if (lower.includes('8b')) tags.push('8b');
  if (lower.includes('13b')) tags.push('13b');
  if (lower.includes('32b')) tags.push('32b');

  if (owner && owner !== 'unknown') tags.push(owner.toLowerCase());
  return tags;
}

// ── Public API ─────────────────────────────────────────────────────

/**
 * List models with optional filters, sorting, and pagination.
 */
export async function listModels(
  client: XergonClientCore,
  filters?: ModelFilter,
  sort?: SortOption,
  pagination?: PaginationOptions,
): Promise<{ models: ModelInfo[]; total: number }> {
  const res = await client.get<ModelsResponse>('/v1/models');
  const rawModels = res.data;

  let enriched = rawModels.map(enrichModel);

  // Apply filters
  if (filters) {
    if (filters.task) {
      enriched = enriched.filter(m => m.task === filters.task);
    }
    if (filters.provider) {
      enriched = enriched.filter(m =>
        m.provider.toLowerCase().includes(filters.provider!.toLowerCase()),
      );
    }
    if (filters.status) {
      enriched = enriched.filter(m => m.status === filters.status);
    }
    if (filters.quantization) {
      enriched = enriched.filter(m => m.quantization === filters.quantization);
    }
    if (filters.tags && filters.tags.length > 0) {
      enriched = enriched.filter(m =>
        filters.tags!.some(t => m.tags.includes(t.toLowerCase())),
      );
    }
    if (filters.minContextLength) {
      enriched = enriched.filter(m => m.contextLength >= filters.minContextLength!);
    }
    if (filters.maxPricePerToken) {
      enriched = enriched.filter(m => m.pricing.perToken <= filters.maxPricePerToken!);
    }
  }

  const total = enriched.length;

  // Apply sorting
  if (sort) {
    const dir = sort.direction === 'desc' ? -1 : 1;
    enriched.sort((a, b) => {
      const aVal = (a as any)[sort.field];
      const bVal = (b as any)[sort.field];
      if (aVal == null && bVal == null) return 0;
      if (aVal == null) return 1;
      if (bVal == null) return -1;
      if (typeof aVal === 'string' && typeof bVal === 'string') {
        return aVal.localeCompare(bVal) * dir;
      }
      return ((aVal as number) - (bVal as number)) * dir;
    });
  }

  // Apply pagination
  if (pagination) {
    const offset = pagination.offset ?? 0;
    const limit = pagination.limit ?? enriched.length;
    enriched = enriched.slice(offset, offset + limit);
  }

  return { models: enriched, total };
}

/**
 * Get detailed information for a specific model.
 */
export async function getModel(
  client: XergonClientCore,
  id: string,
): Promise<ModelInfo | null> {
  const res = await client.get<ModelsResponse>('/v1/models');
  const raw = res.data.find(m => m.id === id || m.id.toLowerCase() === id.toLowerCase());
  if (!raw) return null;

  const info = enrichModel(raw);
  registryCache.set(info.id, info);
  return info;
}

/**
 * Search models by query string matching name, description, or tags.
 */
export async function searchModels(
  client: XergonClientCore,
  query: string,
  pagination?: PaginationOptions,
): Promise<ModelInfo[]> {
  const res = await client.get<ModelsResponse>('/v1/models');
  const lower = query.toLowerCase();
  const terms = lower.split(/\s+/);

  const enriched = res.data
    .map(enrichModel)
    .filter(m => {
      const haystack = `${m.id} ${m.name} ${m.description} ${m.tags.join(' ')} ${m.provider}`.toLowerCase();
      return terms.every(term => haystack.includes(term));
    });

  if (pagination) {
    const offset = pagination.offset ?? 0;
    const limit = pagination.limit ?? enriched.length;
    return enriched.slice(offset, offset + limit);
  }

  return enriched;
}

/**
 * Get version history for a model.
 */
export async function getModelVersions(
  _client: XergonClientCore,
  modelId: string,
): Promise<ModelVersion[]> {
  // The relay does not currently expose a version history endpoint.
  // Return cached data if available, otherwise generate a synthetic entry.
  const cached = versionHistory.get(modelId);
  if (cached) return cached;

  const now = new Date().toISOString();
  const entry: ModelVersion = {
    version: '1.0.0',
    modelId,
    changelog: 'Initial model registration on Xergon Network',
    publishedAt: now,
    deprecated: false,
  };

  return [entry];
}

/**
 * Compare two models side-by-side.
 */
export async function compareModels(
  client: XergonClientCore,
  id1: string,
  id2: string,
): Promise<ModelComparison | null> {
  const [m1, m2] = await Promise.all([
    getModel(client, id1),
    getModel(client, id2),
  ]);

  if (!m1 || !m2) return null;

  const differences: Array<{ field: string; left: unknown; right: unknown }> = [];
  const fields: (keyof ModelInfo)[] = [
    'contextLength', 'maxOutputTokens', 'pricing', 'quantization',
    'task', 'benchmarks', 'status', 'tags',
  ];

  for (const field of fields) {
    const left = m1[field];
    const right = m2[field];
    if (JSON.stringify(left) !== JSON.stringify(right)) {
      differences.push({ field, left, right });
    }
  }

  // Simple recommendation: prefer longer context, lower price, more benchmarks
  let recommendation: string | undefined;
  if (m1.contextLength > m2.contextLength && m1.pricing.perToken <= m2.pricing.perToken) {
    recommendation = `${m1.id} offers longer context at equal or lower cost`;
  } else if (m2.contextLength > m1.contextLength && m2.pricing.perToken <= m1.pricing.perToken) {
    recommendation = `${m2.id} offers longer context at equal or lower cost`;
  } else if (m1.pricing.perToken < m2.pricing.perToken) {
    recommendation = `${m1.id} is more cost-effective`;
  } else if (m2.pricing.perToken < m1.pricing.perToken) {
    recommendation = `${m2.id} is more cost-effective`;
  }

  return {
    model1: m1,
    model2: m2,
    differences,
    recommendation,
  };
}

/**
 * Recommend a model for a given task and optional budget.
 */
export async function getRecommended(
  client: XergonClientCore,
  task: string,
  budget?: number,
): Promise<ModelRecommendation[]> {
  const res = await client.get<ModelsResponse>('/v1/models');
  const enriched = res.data.map(enrichModel);

  const taskLower = task.toLowerCase();
  const scored = enriched
    .filter(m => m.status === 'active')
    .map(m => {
      let score = 0;
      const reasons: string[] = [];

      // Task match scoring
      if (m.task === taskLower) {
        score += 50;
        reasons.push(`Primary task match: ${m.task}`);
      } else if (m.tags.includes(taskLower)) {
        score += 30;
        reasons.push(`Tag match: ${taskLower}`);
      } else if (taskLower === 'code' && (m.tags.includes('code') || m.id.toLowerCase().includes('code'))) {
        score += 40;
        reasons.push('Code-related model');
      } else if (taskLower === 'chat' && (m.tags.includes('chat') || m.id.toLowerCase().includes('instruct'))) {
        score += 35;
        reasons.push('Chat/instruct model');
      }

      // Context length bonus
      if (m.contextLength >= 8192) {
        score += 10;
        reasons.push(`Good context window: ${m.contextLength}`);
      }

      // Popularity bonus
      const pop = popularityScores.get(m.id) ?? 0;
      score += Math.min(pop * 5, 20);

      // Budget filtering
      if (budget !== undefined && m.pricing.perToken > budget) {
        score -= 100;
        reasons.push('Exceeds budget');
      } else if (budget !== undefined && m.pricing.perToken <= budget * 0.5) {
        score += 15;
        reasons.push('Well within budget');
      }

      return {
        modelId: m.id,
        score,
        reason: reasons.join('; '),
        estimatedCost: m.pricing.perToken,
      };
    })
    .filter(r => r.score > 0)
    .sort((a, b) => b.score - a.score);

  return scored.slice(0, 5);
}

/**
 * Get popular models ranked by usage / community preference.
 */
export async function getPopularModels(
  client: XergonClientCore,
  limit: number = 10,
): Promise<ModelInfo[]> {
  const res = await client.get<ModelsResponse>('/v1/models');
  const enriched = res.data.map(enrichModel);

  // Sort by popularity score (cached or default to index-based heuristic)
  return enriched
    .sort((a, b) => {
      const aScore = popularityScores.get(a.id) ?? 0;
      const bScore = popularityScores.get(b.id) ?? 0;
      if (bScore !== aScore) return bScore - aScore;
      // Fallback: shorter IDs tend to be more popular (common models)
      return a.id.localeCompare(b.id);
    })
    .slice(0, limit);
}

/**
 * Subscribe to model changes (added, updated, deprecated).
 * Returns an unsubscribe function.
 */
export function subscribeModel(
  modelId: string,
  callback: (model: ModelInfo) => void,
): () => void {
  if (!subscribers.has(modelId)) {
    subscribers.set(modelId, []);
  }
  subscribers.get(modelId)!.push(callback);

  // Return unsubscribe function
  return () => {
    const subs = subscribers.get(modelId);
    if (subs) {
      const idx = subs.indexOf(callback);
      if (idx >= 0) subs.splice(idx, 1);
      if (subs.length === 0) subscribers.delete(modelId);
    }
  };
}

/**
 * Notify subscribers of a model change.
 */
export function notifyModelChange(model: ModelInfo): void {
  const subs = subscribers.get(model.id);
  if (subs) {
    for (const cb of subs) {
      try { cb(model); } catch { /* ignore subscriber errors */ }
    }
  }
}

/**
 * Check if a model has a deprecation notice.
 */
export async function getDeprecationNotice(
  client: XergonClientCore,
  modelId: string,
): Promise<{ deprecated: boolean; notice?: string; migration?: string } | null> {
  const model = await getModel(client, modelId);
  if (!model) return null;

  if (model.status === 'deprecated') {
    const versions = await getModelVersions(client, modelId);
    const latestVersion = versions.find(v => v.deprecated && v.migrationNotes);
    return {
      deprecated: true,
      notice: `Model "${modelId}" has been deprecated.`,
      migration: latestVersion?.migrationNotes,
    };
  }

  return { deprecated: false };
}

/**
 * Get model lineage (parent/child/fork relationships).
 */
export async function getModelLineage(
  _client: XergonClientCore,
  modelId: string,
): Promise<LineageNode[]> {
  // The relay does not currently expose lineage data.
  // Return cached data if available, otherwise try heuristic matching.
  const cached = lineageMap.get(modelId);
  if (cached) return cached;

  // Heuristic: look for related models by name prefix
  const nodes: LineageNode[] = [];
  const lower = modelId.toLowerCase();

  // Common model family patterns
  const familyPatterns: Array<{ pattern: RegExp; relationship: LineageNode['relationship'] }> = [
    { pattern: /(\d+\.?\d*)b/i, relationship: 'fork' },
    { pattern: /-instruct/i, relationship: 'fork' },
    { pattern: /-chat/i, relationship: 'fork' },
    { pattern: /-v(\d+)/i, relationship: 'successor' },
  ];

  for (const { pattern, relationship } of familyPatterns) {
    const match = modelId.match(pattern);
    if (match) {
      // Generate a plausible related model name
      const base = lower.replace(pattern, '').replace(/-+$/, '');
      if (base) {
        nodes.push({
          modelId: `${base}-base`,
          relationship: 'parent',
        });
      }
    }
  }

  return nodes;
}

/**
 * Update popularity score for a model (used internally).
 */
export function setPopularityScore(modelId: string, score: number): void {
  popularityScores.set(modelId, score);
}

/**
 * Register a version in the local history.
 */
export function registerVersion(version: ModelVersion): void {
  const history = versionHistory.get(version.modelId) ?? [];
  history.push(version);
  versionHistory.set(version.modelId, history);
}

/**
 * Register lineage data for a model.
 */
export function registerLineage(modelId: string, nodes: LineageNode[]): void {
  lineageMap.set(modelId, nodes);
}

/**
 * Clear all cached registry data.
 */
export function clearRegistryCache(): void {
  registryCache.clear();
  versionHistory.clear();
  popularityScores.clear();
  lineageMap.clear();
  subscribers.clear();
}
