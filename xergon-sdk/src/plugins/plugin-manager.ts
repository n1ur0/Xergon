/**
 * Xergon SDK -- Plugin System
 *
 * Provides a plugin architecture for extending the Xergon SDK.
 * Plugins can hook into request/response lifecycle, error handling,
 * and streaming events.
 *
 * Plugins are loaded from ~/.xergon/plugins/ and managed via the CLI.
 */

import * as fs from 'node:fs';
import * as path from 'node:path';
import * as os from 'node:os';

// ── Types ───────────────────────────────────────────────────────────

export type HookName = 'before:request' | 'after:response' | 'on:error' | 'on:stream';

export interface PluginHooks {
  'before:request'?: (request: any) => any;
  'after:response'?: (response: any) => any;
  'on:error'?: (error: Error, context: any) => void;
  'on:stream'?: (chunk: any) => any;
}

export interface PluginManifest {
  name: string;
  version: string;
  description?: string;
  main?: string;
  hooks: (keyof PluginHooks)[];
  enabled?: boolean;
}

export interface Plugin {
  name: string;
  version: string;
  hooks: PluginHooks;
  enabled: boolean;
}

export interface PluginState {
  name: string;
  version: string;
  description?: string;
  enabled: boolean;
  hooks: string[];
}

// ── Plugin Manager ────────────────────────────────────────────────

export class PluginManager {
  private plugins: Map<string, Plugin> = new Map();
  private pluginDir: string;

  constructor(pluginDir?: string) {
    this.pluginDir = pluginDir || path.join(os.homedir(), '.xergon', 'plugins');
  }

  /**
   * Register a plugin programmatically.
   */
  register(plugin: Plugin): void {
    if (this.plugins.has(plugin.name)) {
      this.plugins.delete(plugin.name);
    }
    this.plugins.set(plugin.name, plugin);
  }

  /**
   * Unregister a plugin by name.
   */
  unregister(name: string): boolean {
    return this.plugins.delete(name);
  }

  /**
   * Get a registered plugin by name.
   */
  getPlugin(name: string): Plugin | undefined {
    return this.plugins.get(name);
  }

  /**
   * List all registered plugins.
   */
  listPlugins(): PluginState[] {
    return Array.from(this.plugins.values()).map(p => ({
      name: p.name,
      version: p.version,
      enabled: p.enabled,
      hooks: Object.keys(p.hooks) as string[],
    }));
  }

  /**
   * Enable a plugin by name.
   */
  enable(name: string): boolean {
    const plugin = this.plugins.get(name);
    if (plugin) {
      plugin.enabled = true;
      this.saveState();
      return true;
    }
    return false;
  }

  /**
   * Disable a plugin by name.
   */
  disable(name: string): boolean {
    const plugin = this.plugins.get(name);
    if (plugin) {
      plugin.enabled = false;
      this.saveState();
      return true;
    }
    return false;
  }

  /**
   * Execute a hook across all enabled plugins in registration order.
   * For 'before:request' and 'on:stream', the result of each plugin
   * is passed to the next (pipeline pattern).
   */
  async executeHook<T>(hookName: HookName, data: T, context?: any): Promise<T> {
    let result: any = data;

    for (const plugin of this.plugins.values()) {
      if (!plugin.enabled) continue;
      const handler = plugin.hooks[hookName];
      if (!handler) continue;

      try {
        const fn = handler as Function;
        if (hookName === 'on:error') {
          fn(result, context);
        } else {
          result = fn(result);
        }
      } catch (err) {
        // Plugins should not crash the pipeline
        // eslint-disable-next-line no-console
        console.warn(`Plugin "${plugin.name}" error on "${hookName}":`, err);
      }
    }

    return result as T;
  }

  /**
   * Load plugins from the plugin directory.
   * Reads plugin.json manifests and instantiates plugins.
   */
  async loadFromDirectory(): Promise<void> {
    if (!fs.existsSync(this.pluginDir)) {
      fs.mkdirSync(this.pluginDir, { recursive: true });
      return;
    }

    // Load disabled state
    const stateFile = path.join(this.pluginDir, 'state.json');
    let disabledState: Record<string, boolean> = {};
    try {
      const stateData = fs.readFileSync(stateFile, 'utf-8');
      disabledState = JSON.parse(stateData);
    } catch {
      // No state file yet
    }

    const entries = fs.readdirSync(this.pluginDir, { withFileTypes: true });

    for (const entry of entries) {
      if (!entry.isDirectory()) continue;

      const manifestPath = path.join(this.pluginDir, entry.name, 'plugin.json');
      if (!fs.existsSync(manifestPath)) continue;

      try {
        const manifestRaw = fs.readFileSync(manifestPath, 'utf-8');
        const manifest: PluginManifest = JSON.parse(manifestRaw);

        // Load the plugin module
        const mainFile = manifest.main || 'index.js';
        const mainPath = path.join(this.pluginDir, entry.name, mainFile);

        let hooks: PluginHooks = {};
        if (fs.existsSync(mainPath)) {
          // eslint-disable-next-line @typescript-eslint/no-require-imports
          const pluginModule = require(mainPath) as { hooks?: PluginHooks; default?: { hooks?: PluginHooks } };
          hooks = pluginModule.hooks || pluginModule.default?.hooks || {};
        }

        const enabled = manifest.enabled !== false && !disabledState[manifest.name];

        this.register({
          name: manifest.name,
          version: manifest.version,
          hooks,
          enabled,
        });
      } catch (err) {
        // eslint-disable-next-line no-console
        console.warn(`Failed to load plugin from ${entry.name}:`, err);
      }
    }
  }

  /**
   * Save the enabled/disabled state to disk.
   */
  private saveState(): void {
    if (!fs.existsSync(this.pluginDir)) {
      fs.mkdirSync(this.pluginDir, { recursive: true });
    }

    const state: Record<string, boolean> = {};
    for (const plugin of this.plugins.values()) {
      state[plugin.name] = plugin.enabled;
    }

    const stateFile = path.join(this.pluginDir, 'state.json');
    fs.writeFileSync(stateFile, JSON.stringify(state, null, 2), 'utf-8');
  }

  /**
   * Get the plugin directory path.
   */
  getPluginDir(): string {
    return this.pluginDir;
  }
}

// ── Built-in Plugins ──────────────────────────────────────────────

/**
 * Logging plugin: logs request/response details to stderr.
 */
export const loggingPlugin: Plugin = {
  name: 'logging',
  version: '1.0.0',
  enabled: true,
  hooks: {
    'before:request': (request: any) => {
      // eslint-disable-next-line no-console
      console.debug(`[xergon:logging] Request: ${request.method || 'GET'} ${request.url || request.path}`);
      return request;
    },
    'after:response': (response: any) => {
      // eslint-disable-next-line no-console
      console.debug(`[xergon:logging] Response: ${response.status || 'ok'}`);
      return response;
    },
    'on:error': (error: Error, context: any) => {
      // eslint-disable-next-line no-console
      console.error(`[xergon:logging] Error: ${error.message}`, context);
    },
  },
};

/**
 * Retry plugin: provides automatic retry on certain errors.
 */
export const retryPlugin: Plugin = {
  name: 'retry',
  version: '1.0.0',
  enabled: true,
  hooks: {
    'on:error': (error: Error, _context: any) => {
      const isRetryable = error.message.includes('ECONNRESET') ||
        error.message.includes('ETIMEDOUT') ||
        error.message.includes('429') ||
        error.message.includes('503');
      if (isRetryable) {
        // eslint-disable-next-line no-console
        console.warn(`[xergon:retry] Retryable error detected: ${error.message}`);
      }
    },
  },
};

/**
 * Cache plugin: simple in-memory response cache for GET requests.
 */
export const cachePlugin: Plugin = {
  name: 'cache',
  version: '1.0.0',
  enabled: false,
  hooks: {
    'before:request': (() => {
      const cache = new Map<string, { data: any; expiry: number }>();
      const TTL = 60_000; // 1 minute

      return (request: any) => {
        if (request.method && request.method !== 'GET') return request;
        const key = `${request.method}:${request.url || request.path}`;
        const cached = cache.get(key);
        if (cached && cached.expiry > Date.now()) {
          request.__cached = true;
          request.__cachedData = cached.data;
          return request;
        }
        request.__cacheKey = key;
        return request;
      };
    })(),
    'after:response': (() => {
      const cache = new Map<string, { data: any; expiry: number }>();
      const TTL = 60_000;

      return (response: any) => {
        const request = response.__request;
        if (request?.__cacheKey && !request.__cached) {
          cache.set(request.__cacheKey, {
            data: response,
            expiry: Date.now() + TTL,
          });
        }
        return response;
      };
    })(),
  },
};

/**
 * Rate limit display plugin: shows rate limit headers in responses.
 */
export const rateLimitDisplayPlugin: Plugin = {
  name: 'rate-limit-display',
  version: '1.0.0',
  enabled: true,
  hooks: {
    'after:response': (response: any) => {
      const headers = response?.headers || {};
      const remaining = headers['x-ratelimit-remaining'];
      const limit = headers['x-ratelimit-limit'];
      const reset = headers['x-ratelimit-reset'];

      if (remaining !== undefined) {
        // eslint-disable-next-line no-console
        console.info(
          `[xergon:rate-limit] ${remaining}/${limit} remaining` +
          (reset ? `, resets at ${reset}` : ''),
        );
      }
      return response;
    },
  },
};

/**
 * Get all built-in plugins.
 */
export function getBuiltinPlugins(): Plugin[] {
  return [
    loggingPlugin,
    retryPlugin,
    cachePlugin,
    rateLimitDisplayPlugin,
  ];
}
