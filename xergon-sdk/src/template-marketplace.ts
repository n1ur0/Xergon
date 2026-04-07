/**
 * Xergon SDK -- Template Marketplace
 *
 * Provides a marketplace interface for discovering, publishing, downloading,
 * forking, and reviewing prompt templates. Connects to the Xergon relay API
 * for remote marketplace operations, with local caching.
 *
 * @example
 * ```ts
 * import {
 *   searchTemplates,
 *   downloadTemplate,
 *   publishTemplate,
 *   rateTemplate,
 * } from '@xergon/sdk';
 *
 * // Search for templates
 * const results = await searchTemplates({ query: 'code review', category: 'coding' });
 *
 * // Download a template
 * await downloadTemplate('tpl_abc123');
 *
 * // Publish your own template
 * const published = await publishTemplate(
 *   { name: 'my-review', template: '...', variables: ['code'], category: 'coding' },
 *   { author: '0x...', description: 'My custom review template', tags: ['review', 'code'] },
 * );
 * ```
 */

import * as fs from 'node:fs';
import * as path from 'node:path';
import * as os from 'node:os';
import * as crypto from 'node:crypto';
import { addTemplate, listTemplates, getTemplate as getLocalTemplate } from './prompt-templates';

// ── Types ───────────────────────────────────────────────────────────

export type TemplateCategory =
  | 'coding'
  | 'writing'
  | 'analysis'
  | 'creative'
  | 'business'
  | 'education'
  | 'debugging'
  | 'testing'
  | 'documentation'
  | 'translation'
  | 'summarization'
  | 'conversation'
  | 'roleplay'
  | 'custom';

export interface SharedTemplate {
  id: string;
  name: string;
  description: string;
  template: string; // the prompt template content with {{variables}}
  variables: string[]; // list of variable names
  author: string; // author address or username
  authorName: string;
  category: TemplateCategory;
  tags: string[];
  rating: number;
  downloads: number;
  createdAt: string;
  updatedAt: string;
  version: string;
  isPublic: boolean;
  verified: boolean;
  forkCount: number;
  sourceTemplateId?: string; // if forked
}

export interface TemplateReview {
  id: string;
  templateId: string;
  author: string;
  rating: number;
  comment: string;
  createdAt: string;
}

export interface TemplateSearchOptions {
  query?: string;
  category?: TemplateCategory;
  tags?: string[];
  sort?: 'popular' | 'newest' | 'rating' | 'downloads';
  author?: string;
  limit?: number;
  offset?: number;
}

export interface PublishOptions {
  author: string;
  authorName?: string;
  description: string;
  tags?: string[];
  category: TemplateCategory;
  isPublic?: boolean;
  sourceTemplateId?: string;
}

interface MarketplaceCache {
  templates: Record<string, SharedTemplate>;
  reviews: Record<string, TemplateReview[]>;
  lastSync: string;
}

// ── Storage helpers ────────────────────────────────────────────────

const CACHE_DIR = () => path.join(os.homedir(), '.xergon', 'cache', 'template-marketplace');
const CACHE_FILE = () => path.join(CACHE_DIR(), 'templates.json');

function ensureCacheDir(): void {
  const dir = CACHE_DIR();
  if (!fs.existsSync(dir)) {
    fs.mkdirSync(dir, { recursive: true });
  }
}

function loadCache(): MarketplaceCache {
  try {
    const data = fs.readFileSync(CACHE_FILE(), 'utf-8');
    return JSON.parse(data);
  } catch {
    return { templates: {}, reviews: {}, lastSync: '' };
  }
}

function saveCache(cache: MarketplaceCache): void {
  ensureCacheDir();
  fs.writeFileSync(CACHE_FILE(), JSON.stringify(cache, null, 2) + '\n');
}

// ── Constants ──────────────────────────────────────────────────────

const DEFAULT_BASE_URL = 'https://relay.xergon.gg';
const CACHE_TTL_MS = 5 * 60 * 1000; // 5 minutes

const ALL_CATEGORIES: TemplateCategory[] = [
  'coding', 'writing', 'analysis', 'creative', 'business', 'education',
  'debugging', 'testing', 'documentation', 'translation', 'summarization',
  'conversation', 'roleplay', 'custom',
];

// ── HTTP helpers ───────────────────────────────────────────────────

async function marketplaceFetch<T>(
  baseUrl: string,
  endpoint: string,
  options?: { method?: string; body?: unknown; headers?: Record<string, string> },
): Promise<T> {
  const url = `${baseUrl}/v1/marketplace/templates${endpoint}`;
  const fetchOptions: RequestInit = {
    method: options?.method || 'GET',
    headers: {
      'Content-Type': 'application/json',
      ...options?.headers,
    },
  };

  if (options?.body) {
    fetchOptions.body = JSON.stringify(options.body);
  }

  const response = await fetch(url, fetchOptions);

  if (!response.ok) {
    const errorBody = await response.text().catch(() => '');
    throw new Error(
      `Template marketplace request failed: ${response.status} ${response.statusText}` +
      (errorBody ? ` -- ${errorBody}` : ''),
    );
  }

  return response.json() as Promise<T>;
}

// ── Public API ─────────────────────────────────────────────────────

/**
 * Search the template marketplace.
 *
 * Returns templates matching the given criteria. Results are cached locally
 * for 5 minutes to reduce redundant network requests.
 */
export async function searchTemplates(
  options: TemplateSearchOptions = {},
  config?: { baseUrl?: string; useCache?: boolean },
): Promise<SharedTemplate[]> {
  const baseUrl = config?.baseUrl || DEFAULT_BASE_URL;
  const useCache = config?.useCache !== false;

  // Build query params
  const params = new URLSearchParams();
  if (options.query) params.set('q', options.query);
  if (options.category) params.set('category', options.category);
  if (options.tags?.length) params.set('tags', options.tags.join(','));
  if (options.sort) params.set('sort', options.sort);
  if (options.author) params.set('author', options.author);
  if (options.limit) params.set('limit', String(options.limit));
  if (options.offset) params.set('offset', String(options.offset));

  const queryString = params.toString();
  const cacheKey = `search:${queryString}`;

  // Check cache
  if (useCache) {
    const cache = loadCache();
    const lastSync = new Date(cache.lastSync).getTime();
    const isFresh = Date.now() - lastSync < CACHE_TTL_MS;

    if (isFresh && cache.templates[cacheKey]) {
      return [cache.templates[cacheKey]].flat();
    }
  }

  try {
    const results = await marketplaceFetch<SharedTemplate[]>(
      baseUrl,
      `?${queryString}`,
    );

    // Update cache
    if (useCache) {
      const cache = loadCache();
      cache.lastSync = new Date().toISOString();
      for (const tpl of results) {
        cache.templates[tpl.id] = tpl;
      }
      saveCache(cache);
    }

    return results;
  } catch (err) {
    // On network error, return cached results if available
    if (useCache) {
      const cache = loadCache();
      const cached = Object.values(cache.templates).filter((tpl) => {
        if (!options.query) return true;
        const q = options.query.toLowerCase();
        return (
          tpl.name.toLowerCase().includes(q) ||
          tpl.description.toLowerCase().includes(q) ||
          tpl.tags.some((t) => t.toLowerCase().includes(q))
        );
      });
      if (cached.length > 0) {
        return cached;
      }
    }
    throw err;
  }
}

/**
 * Get a single template by its marketplace ID.
 */
export async function getTemplate(
  id: string,
  config?: { baseUrl?: string; useCache?: boolean },
): Promise<SharedTemplate> {
  const baseUrl = config?.baseUrl || DEFAULT_BASE_URL;
  const useCache = config?.useCache !== false;

  // Check cache first
  if (useCache) {
    const cache = loadCache();
    if (cache.templates[id]) {
      return cache.templates[id];
    }
  }

  const result = await marketplaceFetch<SharedTemplate>(baseUrl, `/${encodeURIComponent(id)}`);

  // Update cache
  if (useCache) {
    const cache = loadCache();
    cache.templates[id] = result;
    cache.lastSync = new Date().toISOString();
    saveCache(cache);
  }

  return result;
}

/**
 * Download (install) a marketplace template locally.
 *
 * The template is saved to ~/.xergon/templates.json so it can be used
 * via `renderTemplate()`, the CLI `template use` command, etc.
 *
 * Returns the local template name that was installed.
 */
export async function downloadTemplate(
  id: string,
  options?: { name?: string; baseUrl?: string; overwrite?: boolean },
): Promise<string> {
  const sharedTemplate = await getTemplate(id, { baseUrl: options?.baseUrl });
  const localName = options?.name || sharedTemplate.name;

  // Check for name collisions
  const existing = getLocalTemplate(localName);
  if (existing && !options?.overwrite) {
    throw new Error(
      `Template "${localName}" already exists locally. Use overwrite: true to replace it.`,
    );
  }

  // Map marketplace category to local category
  const categoryMap: Record<TemplateCategory, 'system' | 'creative' | 'code' | 'analysis' | 'custom'> = {
    coding: 'code',
    writing: 'creative',
    analysis: 'analysis',
    creative: 'creative',
    business: 'custom',
    education: 'system',
    debugging: 'code',
    testing: 'code',
    documentation: 'code',
    translation: 'system',
    summarization: 'analysis',
    conversation: 'custom',
    roleplay: 'creative',
    custom: 'custom',
  };

  addTemplate({
    name: localName,
    description: sharedTemplate.description,
    template: sharedTemplate.template,
    variables: sharedTemplate.variables,
    category: categoryMap[sharedTemplate.category] || 'custom',
  });

  // Increment download count on marketplace (fire-and-forget)
  try {
    await marketplaceFetch(baseUrl(options?.baseUrl), `/${encodeURIComponent(id)}/download`, {
      method: 'POST',
      body: {},
    });
  } catch {
    // Non-critical: best-effort download count update
  }

  return localName;
}

/**
 * Publish a template to the marketplace.
 *
 * @param template - The template definition (name, template string, variables)
 * @param options  - Publication metadata (author, description, tags, category, etc.)
 * @returns The published SharedTemplate as returned by the server
 */
export async function publishTemplate(
  template: { name: string; template: string; variables: string[] },
  options: PublishOptions,
  config?: { baseUrl?: string; authToken?: string },
): Promise<SharedTemplate> {
  if (!template.name || template.name.trim() === '') {
    throw new Error('Template name is required.');
  }
  if (!template.template || template.template.trim() === '') {
    throw new Error('Template content is required.');
  }
  if (!options.author) {
    throw new Error('Author is required to publish a template.');
  }
  if (!options.description) {
    throw new Error('Description is required to publish a template.');
  }

  const headers: Record<string, string> = {};
  if (config?.authToken) {
    headers['Authorization'] = `Bearer ${config.authToken}`;
  }

  const payload = {
    name: template.name,
    template: template.template,
    variables: template.variables,
    author: options.author,
    authorName: options.authorName || options.author,
    description: options.description,
    category: options.category,
    tags: options.tags || [],
    isPublic: options.isPublic !== false,
    sourceTemplateId: options.sourceTemplateId,
    version: '1.0.0',
  };

  const result = await marketplaceFetch<SharedTemplate>(
    config?.baseUrl || DEFAULT_BASE_URL,
    '',
    { method: 'POST', body: payload, headers },
  );

  // Update local cache
  const cache = loadCache();
  cache.templates[result.id] = result;
  cache.lastSync = new Date().toISOString();
  saveCache(cache);

  return result;
}

/**
 * Update a previously published template.
 */
export async function updatePublishedTemplate(
  id: string,
  updates: Partial<Pick<SharedTemplate, 'name' | 'template' | 'variables' | 'description' | 'category' | 'tags' | 'isPublic'>>,
  config?: { baseUrl?: string; authToken?: string },
): Promise<SharedTemplate> {
  const headers: Record<string, string> = {};
  if (config?.authToken) {
    headers['Authorization'] = `Bearer ${config.authToken}`;
  }

  const result = await marketplaceFetch<SharedTemplate>(
    config?.baseUrl || DEFAULT_BASE_URL,
    `/${encodeURIComponent(id)}`,
    { method: 'PATCH', body: updates, headers },
  );

  // Update local cache
  const cache = loadCache();
  cache.templates[result.id] = result;
  cache.lastSync = new Date().toISOString();
  saveCache(cache);

  return result;
}

/**
 * Unpublish (remove) a template from the marketplace.
 */
export async function unpublishTemplate(
  id: string,
  config?: { baseUrl?: string; authToken?: string },
): Promise<void> {
  const headers: Record<string, string> = {};
  if (config?.authToken) {
    headers['Authorization'] = `Bearer ${config.authToken}`;
  }

  await marketplaceFetch<void>(
    config?.baseUrl || DEFAULT_BASE_URL,
    `/${encodeURIComponent(id)}`,
    { method: 'DELETE', headers },
  );

  // Remove from cache
  const cache = loadCache();
  delete cache.templates[id];
  // Remove associated reviews
  for (const key of Object.keys(cache.reviews)) {
    if (cache.reviews[key].some((r) => r.templateId === id)) {
      cache.reviews[key] = cache.reviews[key].filter((r) => r.templateId !== id);
    }
  }
  saveCache(cache);
}

/**
 * Fork someone else's template, creating a copy under your own authorship.
 *
 * The forked template is published to the marketplace and also installed locally.
 * Returns the new SharedTemplate from the marketplace.
 */
export async function forkTemplate(
  id: string,
  options: { author: string; authorName?: string; newName?: string; description?: string },
  config?: { baseUrl?: string; authToken?: string },
): Promise<SharedTemplate> {
  const source = await getTemplate(id, { baseUrl: config?.baseUrl });

  // Install locally (with optional new name)
  const localName = options.newName || `${source.name}-fork`;
  await downloadTemplate(id, { name: localName, overwrite: true, baseUrl: config?.baseUrl });

  // Publish forked copy
  return publishTemplate(
    {
      name: localName,
      template: source.template,
      variables: source.variables,
    },
    {
      author: options.author,
      authorName: options.authorName,
      description: options.description || `Fork of "${source.name}" by ${source.authorName}`,
      category: source.category,
      tags: [...source.tags, 'fork'],
      sourceTemplateId: id,
    },
    config,
  );
}

/**
 * Rate a template (1-5 stars) with an optional comment.
 */
export async function rateTemplate(
  id: string,
  rating: number,
  comment?: string,
  config?: { baseUrl?: string; authToken?: string; author?: string },
): Promise<TemplateReview> {
  if (rating < 1 || rating > 5) {
    throw new Error('Rating must be between 1 and 5.');
  }

  const headers: Record<string, string> = {};
  if (config?.authToken) {
    headers['Authorization'] = `Bearer ${config.authToken}`;
  }

  const body: Record<string, unknown> = { rating };
  if (comment) body.comment = comment;
  if (config?.author) body.author = config.author;

  const result = await marketplaceFetch<TemplateReview>(
    config?.baseUrl || DEFAULT_BASE_URL,
    `/${encodeURIComponent(id)}/reviews`,
    { method: 'POST', body, headers },
  );

  // Update cache
  const cache = loadCache();
  if (!cache.reviews[id]) cache.reviews[id] = [];
  cache.reviews[id].push(result);
  saveCache(cache);

  return result;
}

/**
 * Get reviews for a template.
 */
export async function getTemplateReviews(
  id: string,
  config?: { baseUrl?: string; useCache?: boolean },
): Promise<TemplateReview[]> {
  const useCache = config?.useCache !== false;

  // Check cache
  if (useCache) {
    const cache = loadCache();
    if (cache.reviews[id] && cache.reviews[id].length > 0) {
      return cache.reviews[id];
    }
  }

  const results = await marketplaceFetch<TemplateReview[]>(
    config?.baseUrl || DEFAULT_BASE_URL,
    `/${encodeURIComponent(id)}/reviews`,
  );

  // Update cache
  if (useCache) {
    const cache = loadCache();
    cache.reviews[id] = results;
    saveCache(cache);
  }

  return results;
}

/**
 * List templates published by the current user.
 */
export async function getMyTemplates(
  author: string,
  config?: { baseUrl?: string; limit?: number },
): Promise<SharedTemplate[]> {
  return searchTemplates(
    { author, sort: 'newest', limit: config?.limit || 50 },
    { baseUrl: config?.baseUrl },
  );
}

/**
 * Get popular templates sorted by download count.
 */
export async function getPopularTemplates(
  limit: number = 10,
  config?: { baseUrl?: string; category?: TemplateCategory },
): Promise<SharedTemplate[]> {
  return searchTemplates(
    { sort: 'downloads', limit, category: config?.category },
    { baseUrl: config?.baseUrl },
  );
}

/**
 * Get trending templates (recent high-download, high-rating).
 */
export async function getTrendingTemplates(
  limit: number = 10,
  config?: { baseUrl?: string; category?: TemplateCategory },
): Promise<SharedTemplate[]> {
  return searchTemplates(
    { sort: 'rating', limit, category: config?.category },
    { baseUrl: config?.baseUrl },
  );
}

/**
 * Get verified templates only.
 */
export async function getVerifiedTemplates(
  limit: number = 10,
  config?: { baseUrl?: string; category?: TemplateCategory },
): Promise<SharedTemplate[]> {
  const results = await searchTemplates(
    { sort: 'rating', limit: limit * 3, category: config?.category },
    { baseUrl: config?.baseUrl },
  );
  return results.filter((t) => t.verified).slice(0, limit);
}

/**
 * List all template categories with template counts.
 *
 * Falls back to returning all categories with 0 counts if the
 * marketplace API is unreachable.
 */
export async function getCategories(
  config?: { baseUrl?: string },
): Promise<{ category: TemplateCategory; count: number }[]> {
  try {
    const results = await marketplaceFetch<{ category: TemplateCategory; count: number }[]>(
      config?.baseUrl || DEFAULT_BASE_URL,
      '/categories',
    );
    return results;
  } catch {
    // Return all categories with 0 counts as fallback
    return ALL_CATEGORIES.map((category) => ({ category, count: 0 }));
  }
}

// ── Helpers ────────────────────────────────────────────────────────

function baseUrl(url?: string): string {
  return url || DEFAULT_BASE_URL;
}

/**
 * Clear the local template marketplace cache.
 */
export function clearMarketplaceCache(): void {
  const cacheFile = CACHE_FILE();
  if (fs.existsSync(cacheFile)) {
    fs.unlinkSync(cacheFile);
  }
}
