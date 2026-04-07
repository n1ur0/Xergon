/**
 * Xergon SDK -- Plugin Marketplace
 *
 * Provides a marketplace interface for discovering, installing, publishing,
 * and reviewing Xergon SDK plugins. Connects to the Xergon relay API for
 * remote marketplace operations, with local caching.
 */

import * as fs from 'node:fs';
import * as path from 'node:path';
import * as os from 'node:os';
import * as crypto from 'node:crypto';

// ── Types ───────────────────────────────────────────────────────────

export interface MarketplacePluginManifest {
  name: string;
  version: string;
  description: string;
  author: string;
  repository?: string;
  main: string;
  hooks: string[];
  dependencies?: Record<string, string>;
  keywords?: string[];
  license?: string;
  icon?: string;
}

export interface MarketplacePlugin extends MarketplacePluginManifest {
  id: string;
  downloads: number;
  rating: number;
  reviews: number;
  verified: boolean;
  publishedAt: string;
  updatedAt: string;
  authorAddress?: string;
  category: string;
}

export interface PluginReview {
  id: string;
  pluginId: string;
  author: string;
  rating: number;
  comment: string;
  createdAt: string;
}

export type PluginSortField = 'relevance' | 'downloads' | 'rating' | 'newest' | 'updated';

export interface SearchOptions {
  query?: string;
  category?: string;
  sort?: PluginSortField;
  limit?: number;
  offset?: number;
  verified?: boolean;
}

// ── Marketplace Client ─────────────────────────────────────────────

export class PluginMarketplace {
  private baseUrl: string;
  private cacheDir: string;
  private pluginDir: string;

  constructor(options?: { baseUrl?: string; cacheDir?: string; pluginDir?: string }) {
    this.baseUrl = options?.baseUrl || 'https://relay.xergon.gg';
    this.cacheDir = options?.cacheDir || path.join(os.homedir(), '.xergon', 'cache', 'marketplace');
    this.pluginDir = options?.pluginDir || path.join(os.homedir(), '.xergon', 'plugins');

    // Ensure directories exist
    if (!fs.existsSync(this.cacheDir)) {
      fs.mkdirSync(this.cacheDir, { recursive: true });
    }
    if (!fs.existsSync(this.pluginDir)) {
      fs.mkdirSync(this.pluginDir, { recursive: true });
    }
  }

  /**
   * Search the marketplace for plugins.
   */
  async searchPlugins(options: SearchOptions = {}): Promise<MarketplacePlugin[]> {
    const params = new URLSearchParams();
    if (options.query) params.set('q', options.query);
    if (options.category) params.set('category', options.category);
    if (options.sort) params.set('sort', options.sort);
    if (options.limit) params.set('limit', String(options.limit));
    if (options.offset) params.set('offset', String(options.offset));
    if (options.verified) params.set('verified', 'true');

    const url = `${this.baseUrl}/v1/marketplace/plugins?${params.toString()}`;

    try {
      const response = await fetch(url);
      if (!response.ok) {
        throw new Error(`Marketplace search failed: ${response.status} ${response.statusText}`);
      }
      const data = await response.json();
      return (data.plugins || data || []) as MarketplacePlugin[];
    } catch (err) {
      // Return cached results on network failure
      return this.getCachedSearch(options);
    }
  }

  /**
   * Get detailed information about a specific plugin.
   */
  async getPlugin(id: string): Promise<MarketplacePlugin | null> {
    const url = `${this.baseUrl}/v1/marketplace/plugins/${encodeURIComponent(id)}`;

    try {
      const response = await fetch(url);
      if (!response.ok) {
        if (response.status === 404) return null;
        throw new Error(`Failed to get plugin: ${response.status} ${response.statusText}`);
      }
      return await response.json() as MarketplacePlugin;
    } catch (err) {
      return this.getCachedPlugin(id);
    }
  }

  /**
   * Install a plugin from the marketplace.
   */
  async installPlugin(id: string, version?: string): Promise<{ success: boolean; pluginDir: string; error?: string }> {
    // First, get plugin info
    const plugin = await this.getPlugin(id);
    if (!plugin) {
      return { success: false, pluginDir: '', error: `Plugin "${id}" not found in marketplace` };
    }

    const installVersion = version || plugin.version;
    const pluginDir = path.join(this.pluginDir, plugin.name);

    // Check if already installed
    if (fs.existsSync(pluginDir)) {
      return { success: false, pluginDir, error: `Plugin "${plugin.name}" is already installed` };
    }

    try {
      // Download plugin package
      const downloadUrl = `${this.baseUrl}/v1/marketplace/plugins/${encodeURIComponent(id)}/download?version=${encodeURIComponent(installVersion)}`;
      const response = await fetch(downloadUrl);
      if (!response.ok) {
        throw new Error(`Download failed: ${response.status}`);
      }

      // Create plugin directory
      fs.mkdirSync(pluginDir, { recursive: true });

      // Save the manifest
      const manifest: Record<string, any> = {
        name: plugin.name,
        version: installVersion,
        description: plugin.description,
        author: plugin.author,
        main: plugin.main,
        hooks: plugin.hooks,
        keywords: plugin.keywords,
        license: plugin.license,
        marketplaceId: plugin.id,
        enabled: true,
      };

      fs.writeFileSync(
        path.join(pluginDir, 'plugin.json'),
        JSON.stringify(manifest, null, 2),
        'utf-8',
      );

      // Save the plugin code if provided
      if (response.headers.get('content-type')?.includes('application/json')) {
        const data = await response.json();
        if (data.code) {
          fs.writeFileSync(path.join(pluginDir, plugin.main || 'index.js'), data.code, 'utf-8');
        }
      } else {
        // Binary/tarball handling would go here
        const buffer = Buffer.from(await response.arrayBuffer());
        fs.writeFileSync(path.join(pluginDir, plugin.main || 'index.js'), buffer);
      }

      // Cache plugin info
      this.cachePlugin(plugin);

      return { success: true, pluginDir };
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      // Clean up on failure
      if (fs.existsSync(pluginDir)) {
        fs.rmSync(pluginDir, { recursive: true, force: true });
      }
      return { success: false, pluginDir: '', error: message };
    }
  }

  /**
   * Uninstall a plugin by name.
   */
  async uninstallPlugin(name: string): Promise<{ success: boolean; error?: string }> {
    const pluginDir = path.join(this.pluginDir, name);

    if (!fs.existsSync(pluginDir)) {
      return { success: false, error: `Plugin "${name}" is not installed` };
    }

    try {
      fs.rmSync(pluginDir, { recursive: true, force: true });
      return { success: true };
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      return { success: false, error: message };
    }
  }

  /**
   * Update a plugin to the latest version from the marketplace.
   */
  async updatePlugin(name: string): Promise<{ success: boolean; updated: boolean; version?: string; error?: string }> {
    // Read current manifest
    const manifestPath = path.join(this.pluginDir, name, 'plugin.json');
    if (!fs.existsSync(manifestPath)) {
      return { success: false, updated: false, error: `Plugin "${name}" is not installed` };
    }

    let manifest: Record<string, any>;
    try {
      manifest = JSON.parse(fs.readFileSync(manifestPath, 'utf-8'));
    } catch {
      return { success: false, updated: false, error: 'Failed to read plugin manifest' };
    }

    const marketplaceId = manifest.marketplaceId || name;
    const plugin = await this.getPlugin(marketplaceId);

    if (!plugin) {
      return { success: false, updated: false, error: `Plugin "${name}" not found in marketplace` };
    }

    if (plugin.version === manifest.version) {
      return { success: true, updated: false, version: manifest.version };
    }

    // Uninstall and reinstall
    const uninstallResult = await this.uninstallPlugin(name);
    if (!uninstallResult.success) {
      return { success: false, updated: false, error: uninstallResult.error };
    }

    const installResult = await this.installPlugin(marketplaceId, plugin.version);
    if (!installResult.success) {
      return { success: false, updated: false, error: installResult.error };
    }

    return { success: true, updated: true, version: plugin.version };
  }

  /**
   * Publish a plugin to the marketplace.
   */
  async publishPlugin(manifestPathOrObj: string | MarketplacePluginManifest): Promise<{ success: boolean; id?: string; error?: string }> {
    let manifest: MarketplacePluginManifest;

    if (typeof manifestPathOrObj === 'string') {
      // Read from file
      if (!fs.existsSync(manifestPathOrObj)) {
        return { success: false, error: `Manifest file not found: ${manifestPathOrObj}` };
      }
      try {
        manifest = JSON.parse(fs.readFileSync(manifestPathOrObj, 'utf-8')) as MarketplacePluginManifest;
      } catch {
        return { success: false, error: 'Failed to parse manifest file' };
      }
    } else {
      manifest = manifestPathOrObj;
    }

    // Validate manifest
    const validation = this.validateManifest(manifest);
    if (!validation.valid) {
      return { success: false, error: `Invalid manifest: ${validation.errors.join(', ')}` };
    }

    try {
      const url = `${this.baseUrl}/v1/marketplace/plugins/publish`;
      const response = await fetch(url, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(manifest),
      });

      if (!response.ok) {
        const data = await response.json().catch(() => ({}));
        throw new Error(data.message || `Publish failed: ${response.status}`);
      }

      const data = await response.json() as { id: string };
      return { success: true, id: data.id };
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      return { success: false, error: message };
    }
  }

  /**
   * List all installed plugins with their marketplace metadata.
   */
  listInstalled(): Array<{
    name: string;
    version: string;
    description?: string;
    enabled: boolean;
    marketplaceId?: string;
  }> {
    if (!fs.existsSync(this.pluginDir)) {
      return [];
    }

    const plugins: Array<{
      name: string;
      version: string;
      description?: string;
      enabled: boolean;
      marketplaceId?: string;
    }> = [];

    const entries = fs.readdirSync(this.pluginDir, { withFileTypes: true });

    for (const entry of entries) {
      if (!entry.isDirectory()) continue;

      const mp = path.join(this.pluginDir, entry.name, 'plugin.json');
      if (!fs.existsSync(mp)) continue;

      try {
        const manifest = JSON.parse(fs.readFileSync(mp, 'utf-8'));
        plugins.push({
          name: manifest.name || entry.name,
          version: manifest.version || '0.0.0',
          description: manifest.description,
          enabled: manifest.enabled !== false,
          marketplaceId: manifest.marketplaceId,
        });
      } catch {
        // Skip malformed
      }
    }

    return plugins;
  }

  /**
   * Get reviews for a plugin.
   */
  async getPluginReviews(id: string, limit?: number): Promise<PluginReview[]> {
    const params = new URLSearchParams();
    if (limit) params.set('limit', String(limit));

    const url = `${this.baseUrl}/v1/marketplace/plugins/${encodeURIComponent(id)}/reviews?${params.toString()}`;

    try {
      const response = await fetch(url);
      if (!response.ok) {
        throw new Error(`Failed to get reviews: ${response.status}`);
      }
      const data = await response.json();
      return (data.reviews || data || []) as PluginReview[];
    } catch (err) {
      return [];
    }
  }

  /**
   * Rate and review a plugin.
   */
  async ratePlugin(id: string, rating: number, comment: string): Promise<{ success: boolean; reviewId?: string; error?: string }> {
    if (rating < 1 || rating > 5) {
      return { success: false, error: 'Rating must be between 1 and 5' };
    }

    try {
      const url = `${this.baseUrl}/v1/marketplace/plugins/${encodeURIComponent(id)}/reviews`;
      const response = await fetch(url, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ rating, comment }),
      });

      if (!response.ok) {
        const data = await response.json().catch(() => ({}));
        throw new Error(data.message || `Rating failed: ${response.status}`);
      }

      const data = await response.json() as { id: string };
      return { success: true, reviewId: data.id };
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      return { success: false, error: message };
    }
  }

  /**
   * Get all available plugin categories.
   */
  async getCategories(): Promise<string[]> {
    const url = `${this.baseUrl}/v1/marketplace/categories`;

    try {
      const response = await fetch(url);
      if (!response.ok) {
        throw new Error(`Failed to get categories: ${response.status}`);
      }
      const data = await response.json();
      return (data.categories || data || []) as string[];
    } catch (err) {
      // Return default categories
      return [
        'logging',
        'monitoring',
        'caching',
        'authentication',
        'rate-limiting',
        'transform',
        'utility',
        'integration',
        'testing',
        'security',
      ];
    }
  }

  /**
   * Get popular/trending plugins.
   */
  async getPopular(limit?: number): Promise<MarketplacePlugin[]> {
    return this.searchPlugins({ sort: 'downloads', limit: limit || 10 });
  }

  /**
   * Get featured plugins curated by the Xergon team.
   */
  async getFeatured(limit?: number): Promise<MarketplacePlugin[]> {
    const url = `${this.baseUrl}/v1/marketplace/featured`;
    if (limit) url + `?limit=${limit}`;

    try {
      const response = await fetch(limit ? `${url}?limit=${limit}` : url);
      if (!response.ok) {
        throw new Error(`Failed to get featured: ${response.status}`);
      }
      const data = await response.json();
      return (data.plugins || data || []) as MarketplacePlugin[];
    } catch (err) {
      // Fallback to popular
      return this.getPopular(limit);
    }
  }

  // ── Private Helpers ───────────────────────────────────────────────

  private validateManifest(manifest: MarketplacePluginManifest): { valid: boolean; errors: string[] } {
    const errors: string[] = [];

    if (!manifest.name || typeof manifest.name !== 'string') errors.push('name is required');
    if (!manifest.version || typeof manifest.version !== 'string') errors.push('version is required');
    if (!manifest.description || typeof manifest.description !== 'string') errors.push('description is required');
    if (!manifest.author || typeof manifest.author !== 'string') errors.push('author is required');
    if (!manifest.main || typeof manifest.main !== 'string') errors.push('main is required');
    if (!Array.isArray(manifest.hooks)) errors.push('hooks must be an array');

    return { valid: errors.length === 0, errors };
  }

  private getCachePath(key: string): string {
    const hash = crypto.createHash('sha256').update(key).digest('hex').slice(0, 16);
    return path.join(this.cacheDir, `${hash}.json`);
  }

  private cachePlugin(plugin: MarketplacePlugin): void {
    try {
      const cachePath = this.getCachePath(`plugin:${plugin.id}`);
      fs.writeFileSync(cachePath, JSON.stringify({ data: plugin, cachedAt: Date.now() }), 'utf-8');
    } catch {
      // Cache write failure is non-critical
    }
  }

  private getCachedPlugin(id: string): MarketplacePlugin | null {
    try {
      const cachePath = this.getCachePath(`plugin:${id}`);
      if (!fs.existsSync(cachePath)) return null;
      const cached = JSON.parse(fs.readFileSync(cachePath, 'utf-8'));
      // Cache valid for 1 hour
      if (Date.now() - cached.cachedAt > 3600_000) return null;
      return cached.data as MarketplacePlugin;
    } catch {
      return null;
    }
  }

  private getCachedSearch(options: SearchOptions): MarketplacePlugin[] {
    try {
      const key = `search:${JSON.stringify(options)}`;
      const cachePath = this.getCachePath(key);
      if (!fs.existsSync(cachePath)) return [];
      const cached = JSON.parse(fs.readFileSync(cachePath, 'utf-8'));
      if (Date.now() - cached.cachedAt > 3600_000) return [];
      return cached.data as MarketplacePlugin[];
    } catch {
      return [];
    }
  }
}

// ── Convenience Functions ──────────────────────────────────────────

let defaultMarketplace: PluginMarketplace | null = null;

function getMarketplace(): PluginMarketplace {
  if (!defaultMarketplace) {
    defaultMarketplace = new PluginMarketplace();
  }
  return defaultMarketplace;
}

export async function searchPlugins(options?: SearchOptions): Promise<MarketplacePlugin[]> {
  return getMarketplace().searchPlugins(options);
}

export async function getPlugin(id: string): Promise<MarketplacePlugin | null> {
  return getMarketplace().getPlugin(id);
}

export async function installPlugin(id: string, version?: string): Promise<{ success: boolean; pluginDir: string; error?: string }> {
  return getMarketplace().installPlugin(id, version);
}

export async function uninstallPlugin(name: string): Promise<{ success: boolean; error?: string }> {
  return getMarketplace().uninstallPlugin(name);
}

export async function updatePlugin(name: string): Promise<{ success: boolean; updated: boolean; version?: string; error?: string }> {
  return getMarketplace().updatePlugin(name);
}

export async function publishPlugin(manifest: string | MarketplacePluginManifest): Promise<{ success: boolean; id?: string; error?: string }> {
  return getMarketplace().publishPlugin(manifest);
}

export function listInstalledPlugins(): ReturnType<PluginMarketplace['listInstalled']> {
  return getMarketplace().listInstalled();
}

export async function getPluginReviews(id: string, limit?: number): Promise<PluginReview[]> {
  return getMarketplace().getPluginReviews(id, limit);
}

export async function ratePlugin(id: string, rating: number, comment: string): Promise<{ success: boolean; reviewId?: string; error?: string }> {
  return getMarketplace().ratePlugin(id, rating, comment);
}

export async function getCategories(): Promise<string[]> {
  return getMarketplace().getCategories();
}

export async function getPopularPlugins(limit?: number): Promise<MarketplacePlugin[]> {
  return getMarketplace().getPopular(limit);
}

export async function getFeaturedPlugins(limit?: number): Promise<MarketplacePlugin[]> {
  return getMarketplace().getFeatured(limit);
}
