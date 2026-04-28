/**
 * Xergon SDK Plugin Marketplace - Unit Tests
 * Tests for PluginMarketplace class functionality
 */

import * as fs from 'node:fs';
import * as path from 'node:path';
import * as os from 'node:os';
import { PluginMarketplace, MarketplacePlugin, SearchOptions } from '../src/plugins/plugin-marketplace';

// Mock fetch for testing
global.fetch = jest.fn();

describe('PluginMarketplace', () => {
  let marketplace: PluginMarketplace;
  const testCacheDir = path.join(os.tmpdir(), 'xergon-test-cache');
  const testPluginDir = path.join(os.tmpdir(), 'xergon-test-plugins');

  beforeEach(() => {
    // Clean up test directories
    if (fs.existsSync(testCacheDir)) {
      fs.rmSync(testCacheDir, { recursive: true, force: true });
    }
    if (fs.existsSync(testPluginDir)) {
      fs.rmSync(testPluginDir, { recursive: true, force: true });
    }

    // Initialize marketplace with test directories
    marketplace = new PluginMarketplace({
      baseUrl: 'http://localhost:9090',
      cacheDir: testCacheDir,
      pluginDir: testPluginDir,
    });

    // Mock fetch
    jest.clearAllMocks();
  });

  afterEach(() => {
    // Clean up
    if (fs.existsSync(testCacheDir)) {
      fs.rmSync(testCacheDir, { recursive: true, force: true });
    }
    if (fs.existsSync(testPluginDir)) {
      fs.rmSync(testPluginDir, { recursive: true, force: true });
    }
  });

  describe('searchPlugins', () => {
    it('should search plugins with query parameter', async () => {
      const mockPlugins: MarketplacePlugin[] = [
        {
          id: 'plugin-1',
          name: 'test-plugin',
          version: '1.0.0',
          description: 'A test plugin',
          author: 'Test Author',
          main: 'index.js',
          hooks: ['onInit'],
          downloads: 100,
          rating: 4.5,
          reviews: 10,
          verified: true,
          publishedAt: '2024-01-01T00:00:00Z',
          updatedAt: '2024-01-01T00:00:00Z',
          category: 'testing',
        },
      ];

      (global.fetch as jest.Mock).mockResolvedValue({
        ok: true,
        json: async () => ({ plugins: mockPlugins }),
      });

      const result = await marketplace.searchPlugins({ query: 'test' });

      expect(global.fetch).toHaveBeenCalled();
      expect(result).toEqual(mockPlugins);
    });

    it('should filter by category', async () => {
      const mockPlugins: MarketplacePlugin[] = [
        {
          id: 'plugin-1',
          name: 'logging-plugin',
          version: '1.0.0',
          description: 'A logging plugin',
          author: 'Test Author',
          main: 'index.js',
          hooks: ['onLog'],
          downloads: 50,
          rating: 4.0,
          reviews: 5,
          verified: true,
          publishedAt: '2024-01-01T00:00:00Z',
          updatedAt: '2024-01-01T00:00:00Z',
          category: 'logging',
        },
      ];

      (global.fetch as jest.Mock).mockResolvedValue({
        ok: true,
        json: async () => ({ plugins: mockPlugins }),
      });

      const result = await marketplace.searchPlugins({ category: 'logging' });

      expect(result.length).toBe(1);
      expect(result[0].category).toBe('logging');
    });

    it('should return cached results on network failure', async () => {
      // First, populate cache
      const cachedData = {
        data: [
          {
            id: 'cached-plugin',
            name: 'cached-plugin',
            version: '1.0.0',
            description: 'Cached',
            author: 'Test',
            main: 'index.js',
            hooks: [],
            downloads: 0,
            rating: 0,
            reviews: 0,
            verified: false,
            publishedAt: '2024-01-01T00:00:00Z',
            updatedAt: '2024-01-01T00:00:00Z',
            category: 'utility',
          },
        ],
        cachedAt: Date.now(),
      };

      const cachePath = path.join(testCacheDir, 'search-test.json');
      fs.mkdirSync(testCacheDir, { recursive: true });
      fs.writeFileSync(cachePath, JSON.stringify(cachedData));

      // Simulate network failure
      (global.fetch as jest.Mock).mockRejectedValue(new Error('Network error'));

      const result = await marketplace.searchPlugins({ query: 'test' });

      // Should return cached results
      expect(result.length).toBeGreaterThan(0);
    });

    it('should handle empty results', async () => {
      (global.fetch as jest.Mock).mockResolvedValue({
        ok: true,
        json: async () => ({ plugins: [] }),
      });

      const result = await marketplace.searchPlugins({ query: 'nonexistent' });

      expect(result).toEqual([]);
    });
  });

  describe('getPlugin', () => {
    it('should fetch plugin by ID', async () => {
      const mockPlugin: MarketplacePlugin = {
        id: 'plugin-1',
        name: 'test-plugin',
        version: '1.0.0',
        description: 'A test plugin',
        author: 'Test Author',
        main: 'index.js',
        hooks: ['onInit'],
        downloads: 100,
        rating: 4.5,
        reviews: 10,
        verified: true,
        publishedAt: '2024-01-01T00:00:00Z',
        updatedAt: '2024-01-01T00:00:00Z',
        category: 'testing',
      };

      (global.fetch as jest.Mock).mockResolvedValue({
        ok: true,
        json: async () => mockPlugin,
      });

      const result = await marketplace.getPlugin('plugin-1');

      expect(result).toEqual(mockPlugin);
    });

    it('should return null for non-existent plugin', async () => {
      (global.fetch as jest.Mock).mockResolvedValue({
        ok: false,
        status: 404,
        statusText: 'Not Found',
      });

      const result = await marketplace.getPlugin('nonexistent');

      expect(result).toBeNull();
    });
  });

  describe('installPlugin', () => {
    it('should install plugin successfully', async () => {
      const mockPlugin: MarketplacePlugin = {
        id: 'plugin-1',
        name: 'test-plugin',
        version: '1.0.0',
        description: 'A test plugin',
        author: 'Test Author',
        main: 'index.js',
        hooks: ['onInit'],
        downloads: 100,
        rating: 4.5,
        reviews: 10,
        verified: true,
        publishedAt: '2024-01-01T00:00:00Z',
        updatedAt: '2024-01-01T00:00:00Z',
        category: 'testing',
      };

      (global.fetch as jest.Mock)
        .mockResolvedValueOnce({
          ok: true,
          json: async () => mockPlugin,
        })
        .mockResolvedValueOnce({
          ok: true,
          headers: new Map([['content-type', 'application/json']]),
          json: async () => ({ code: 'console.log("hello")' }),
        });

      const result = await marketplace.installPlugin('plugin-1');

      expect(result.success).toBe(true);
      expect(fs.existsSync(path.join(testPluginDir, 'test-plugin'))).toBe(true);
    });

    it('should fail if plugin already installed', async () => {
      // Create plugin directory
      fs.mkdirSync(path.join(testPluginDir, 'existing-plugin'), { recursive: true });
      fs.writeFileSync(
        path.join(testPluginDir, 'existing-plugin', 'plugin.json'),
        JSON.stringify({ name: 'existing-plugin', version: '1.0.0', enabled: true })
      );

      const result = await marketplace.installPlugin('existing-plugin');

      expect(result.success).toBe(false);
      expect(result.error).toContain('already installed');
    });

    it('should clean up on installation failure', async () => {
      (global.fetch as jest.Mock).mockRejectedValue(new Error('Download failed'));

      const result = await marketplace.installPlugin('failing-plugin');

      expect(result.success).toBe(false);
      expect(fs.existsSync(path.join(testPluginDir, 'failing-plugin'))).toBe(false);
    });
  });

  describe('uninstallPlugin', () => {
    it('should uninstall plugin successfully', async () => {
      // Create plugin directory
      fs.mkdirSync(path.join(testPluginDir, 'to-uninstall'), { recursive: true });
      fs.writeFileSync(
        path.join(testPluginDir, 'to-uninstall', 'plugin.json'),
        JSON.stringify({ name: 'to-uninstall', version: '1.0.0' })
      );

      const result = await marketplace.uninstallPlugin('to-uninstall');

      expect(result.success).toBe(true);
      expect(fs.existsSync(path.join(testPluginDir, 'to-uninstall'))).toBe(false);
    });

    it('should fail if plugin not installed', async () => {
      const result = await marketplace.uninstallPlugin('nonexistent');

      expect(result.success).toBe(false);
      expect(result.error).toContain('not installed');
    });
  });

  describe('updatePlugin', () => {
    it('should update plugin to latest version', async () => {
      // Create installed plugin
      fs.mkdirSync(path.join(testPluginDir, 'to-update'), { recursive: true });
      fs.writeFileSync(
        path.join(testPluginDir, 'to-update', 'plugin.json'),
        JSON.stringify({ name: 'to-update', version: '1.0.0', marketplaceId: 'plugin-1' })
      );

      const mockPlugin: MarketplacePlugin = {
        id: 'plugin-1',
        name: 'to-update',
        version: '2.0.0',
        description: 'Updated plugin',
        author: 'Test Author',
        main: 'index.js',
        hooks: ['onInit'],
        downloads: 200,
        rating: 5.0,
        reviews: 20,
        verified: true,
        publishedAt: '2024-01-01T00:00:00Z',
        updatedAt: '2024-01-02T00:00:00Z',
        category: 'testing',
      };

      (global.fetch as jest.Mock)
        .mockResolvedValueOnce({
          ok: true,
          json: async () => mockPlugin,
        })
        .mockResolvedValueOnce({
          ok: true,
          headers: new Map([['content-type', 'application/json']]),
          json: async () => ({ code: 'console.log("updated")' }),
        });

      const result = await marketplace.updatePlugin('to-update');

      expect(result.success).toBe(true);
      expect(result.updated).toBe(true);
      expect(result.version).toBe('2.0.0');
    });

    it('should not update if already on latest version', async () => {
      // Create installed plugin
      fs.mkdirSync(path.join(testPluginDir, 'latest-plugin'), { recursive: true });
      fs.writeFileSync(
        path.join(testPluginDir, 'latest-plugin', 'plugin.json'),
        JSON.stringify({ name: 'latest-plugin', version: '1.0.0', marketplaceId: 'plugin-1' })
      );

      const mockPlugin: MarketplacePlugin = {
        id: 'plugin-1',
        name: 'latest-plugin',
        version: '1.0.0',
        description: 'Latest plugin',
        author: 'Test Author',
        main: 'index.js',
        hooks: ['onInit'],
        downloads: 100,
        rating: 4.5,
        reviews: 10,
        verified: true,
        publishedAt: '2024-01-01T00:00:00Z',
        updatedAt: '2024-01-01T00:00:00Z',
        category: 'testing',
      };

      (global.fetch as jest.Mock).mockResolvedValue({
        ok: true,
        json: async () => mockPlugin,
      });

      const result = await marketplace.updatePlugin('latest-plugin');

      expect(result.success).toBe(true);
      expect(result.updated).toBe(false);
      expect(result.version).toBe('1.0.0');
    });
  });

  describe('publishPlugin', () => {
    it('should publish plugin successfully', async () => {
      const manifest = {
        name: 'new-plugin',
        version: '1.0.0',
        description: 'A new plugin',
        author: 'Test Author',
        main: 'index.js',
        hooks: ['onInit'],
      };

      (global.fetch as jest.Mock).mockResolvedValue({
        ok: true,
        json: async () => ({ id: 'new-plugin-id' }),
      });

      const result = await marketplace.publishPlugin(manifest);

      expect(result.success).toBe(true);
      expect(result.id).toBe('new-plugin-id');
    });

    it('should validate manifest before publishing', async () => {
      const invalidManifest = {
        name: '', // Invalid: empty name
        version: '1.0.0',
        description: 'Invalid plugin',
        author: 'Test',
        main: 'index.js',
        hooks: [],
      };

      const result = await marketplace.publishPlugin(invalidManifest as any);

      expect(result.success).toBe(false);
      expect(result.error).toContain('Invalid manifest');
    });
  });

  describe('listInstalled', () => {
    it('should list all installed plugins', async () => {
      // Create multiple plugins
      const plugins = ['plugin-a', 'plugin-b', 'plugin-c'];
      for (const name of plugins) {
        fs.mkdirSync(path.join(testPluginDir, name), { recursive: true });
        fs.writeFileSync(
          path.join(testPluginDir, name, 'plugin.json'),
          JSON.stringify({
            name,
            version: '1.0.0',
            description: `${name} description`,
            enabled: name !== 'plugin-b',
          })
        );
      }

      const result = marketplace.listInstalled();

      expect(result.length).toBe(3);
      expect(result.map((p) => p.name)).toEqual(expect.arrayContaining(plugins));
    });

    it('should return empty array if no plugins installed', async () => {
      const result = marketplace.listInstalled();

      expect(result).toEqual([]);
    });
  });

  describe('getPluginReviews', () => {
    it('should fetch plugin reviews', async () => {
      const mockReviews = [
        {
          id: 'review-1',
          pluginId: 'plugin-1',
          author: 'User 1',
          rating: 5,
          comment: 'Great plugin!',
          createdAt: '2024-01-01T00:00:00Z',
        },
        {
          id: 'review-2',
          pluginId: 'plugin-1',
          author: 'User 2',
          rating: 4,
          comment: 'Good but needs improvements',
          createdAt: '2024-01-02T00:00:00Z',
        },
      ];

      (global.fetch as jest.Mock).mockResolvedValue({
        ok: true,
        json: async () => ({ reviews: mockReviews }),
      });

      const result = await marketplace.getPluginReviews('plugin-1');

      expect(result).toEqual(mockReviews);
    });

    it('should return empty array on error', async () => {
      (global.fetch as jest.Mock).mockRejectedValue(new Error('Failed'));

      const result = await marketplace.getPluginReviews('plugin-1');

      expect(result).toEqual([]);
    });
  });

  describe('ratePlugin', () => {
    it('should rate plugin successfully', async () => {
      (global.fetch as jest.Mock).mockResolvedValue({
        ok: true,
        json: async () => ({ id: 'review-1' }),
      });

      const result = await marketplace.ratePlugin('plugin-1', 5, 'Excellent!');

      expect(result.success).toBe(true);
      expect(result.reviewId).toBe('review-1');
    });

    it('should validate rating range', async () => {
      const result = await marketplace.ratePlugin('plugin-1', 6, 'Invalid rating');

      expect(result.success).toBe(false);
      expect(result.error).toContain('between 1 and 5');
    });
  });

  describe('getCategories', () => {
    it('should fetch categories from marketplace', async () => {
      const mockCategories = ['testing', 'logging', 'monitoring'];

      (global.fetch as jest.Mock).mockResolvedValue({
        ok: true,
        json: async () => ({ categories: mockCategories }),
      });

      const result = await marketplace.getCategories();

      expect(result).toEqual(mockCategories);
    });

    it('should return default categories on error', async () => {
      (global.fetch as jest.Mock).mockRejectedValue(new Error('Failed'));

      const result = await marketplace.getCategories();

      expect(result.length).toBeGreaterThan(0);
      expect(result).toContain('testing');
    });
  });

  describe('getPopular', () => {
    it('should return popular plugins sorted by downloads', async () => {
      const mockPlugins: MarketplacePlugin[] = [
        {
          id: 'plugin-1',
          name: 'popular-plugin',
          version: '1.0.0',
          description: 'Popular',
          author: 'Test',
          main: 'index.js',
          hooks: [],
          downloads: 1000,
          rating: 4.5,
          reviews: 100,
          verified: true,
          publishedAt: '2024-01-01T00:00:00Z',
          updatedAt: '2024-01-01T00:00:00Z',
          category: 'testing',
        },
      ];

      (global.fetch as jest.Mock).mockResolvedValue({
        ok: true,
        json: async () => ({ plugins: mockPlugins }),
      });

      const result = await marketplace.getPopular(5);

      expect(result.length).toBe(1);
      expect((global.fetch as jest.Mock).mock.calls[0][0]).toContain('sort=downloads');
    });
  });

  describe('getFeatured', () => {
    it('should fetch featured plugins', async () => {
      const mockPlugins: MarketplacePlugin[] = [
        {
          id: 'featured-1',
          name: 'featured-plugin',
          version: '1.0.0',
          description: 'Featured',
          author: 'Test',
          main: 'index.js',
          hooks: [],
          downloads: 500,
          rating: 5.0,
          reviews: 50,
          verified: true,
          publishedAt: '2024-01-01T00:00:00Z',
          updatedAt: '2024-01-01T00:00:00Z',
          category: 'testing',
        },
      ];

      (global.fetch as jest.Mock).mockResolvedValue({
        ok: true,
        json: async () => ({ plugins: mockPlugins }),
      });

      const result = await marketplace.getFeatured(3);

      expect(result.length).toBe(1);
    });

    it('should fallback to popular on error', async () => {
      (global.fetch as jest.Mock)
        .mockRejectedValueOnce(new Error('Featured failed'))
        .mockResolvedValueOnce({
          ok: true,
          json: async () => ({ plugins: [] }),
        });

      const result = await marketplace.getFeatured();

      expect(result.length).toBe(0);
    });
  });
});
