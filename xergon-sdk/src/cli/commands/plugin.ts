/**
 * CLI command: plugin
 *
 * Manage Xergon SDK plugins including marketplace operations.
 *
 * Usage:
 *   xergon plugin list                          -- list installed plugins
 *   xergon plugin install <name|id>             -- install from marketplace
 *   xergon plugin remove <name>                 -- remove installed plugin
 *   xergon plugin enable <name>                 -- enable a plugin
 *   xergon plugin disable <name>                -- disable a plugin
 *   xergon plugin search <query>                -- search marketplace
 *   xergon plugin info <name|id>                -- show plugin info
 *   xergon plugin update [name]                 -- update plugins
 *   xergon plugin publish                       -- publish to marketplace
 *   xergon plugin reviews <name|id>             -- show reviews
 *   xergon plugin categories                    -- list categories
 */

import type { Command, ParsedArgs, CLIContext } from '../mod';
import * as fs from 'node:fs';
import * as path from 'node:path';
import * as os from 'node:os';

const PLUGIN_DIR = path.join(os.homedir(), '.xergon', 'plugins');

async function pluginAction(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const sub = args.positional[0];

  if (!sub) {
    ctx.output.writeError('Usage: xergon plugin <list|install|remove|enable|disable|search|info|update|publish|reviews|categories> [args]');
    process.exit(1);
    return;
  }

  switch (sub) {
    case 'list':
      await handleList(args, ctx);
      break;
    case 'install':
      await handleInstall(args, ctx);
      break;
    case 'remove':
      await handleRemove(args, ctx);
      break;
    case 'enable':
      await handleEnable(args, ctx);
      break;
    case 'disable':
      await handleDisable(args, ctx);
      break;
    case 'search':
      await handleSearch(args, ctx);
      break;
    case 'info':
      await handleInfo(args, ctx);
      break;
    case 'update':
      await handleUpdate(args, ctx);
      break;
    case 'publish':
      await handlePublish(args, ctx);
      break;
    case 'reviews':
      await handleReviews(args, ctx);
      break;
    case 'categories':
      await handleCategories(args, ctx);
      break;
    default:
      ctx.output.writeError(`Unknown subcommand: ${sub}`);
      ctx.output.write('Usage: xergon plugin <list|install|remove|enable|disable|search|info|update|publish|reviews|categories> [args]');
      process.exit(1);
  }
}

// ── list ───────────────────────────────────────────────────────────

async function handleList(_args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const plugins = getInstalledPlugins();

  if (plugins.length === 0) {
    ctx.output.info('No plugins installed.');
    ctx.output.write(`  Plugin directory: ${PLUGIN_DIR}`);
    return;
  }

  const tableData = plugins.map(p => ({
    Name: p.name,
    Version: p.version,
    Enabled: p.enabled ? ctx.output.colorize('yes', 'green') : ctx.output.colorize('no', 'red'),
    Hooks: p.hooks.join(', ') || '-',
  }));
  ctx.output.write(ctx.output.formatTable(tableData, `Installed Plugins (${plugins.length})`));
}

// ── install ────────────────────────────────────────────────────────

async function handleInstall(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const nameOrId = args.positional[1];

  if (!nameOrId) {
    ctx.output.writeError('No plugin name or ID specified. Use: xergon plugin install <name|id>');
    process.exit(1);
    return;
  }

  // Ensure plugin directory exists
  if (!fs.existsSync(PLUGIN_DIR)) {
    fs.mkdirSync(PLUGIN_DIR, { recursive: true });
  }

  // Check if already installed locally
  const pluginDir = path.join(PLUGIN_DIR, nameOrId);
  if (fs.existsSync(pluginDir)) {
    ctx.output.writeError(`Plugin "${nameOrId}" is already installed locally.`);
    process.exit(1);
    return;
  }

  const thinkingMsg = ctx.output.colorize('Installing plugin', 'cyan');
  process.stderr.write(`${thinkingMsg}...\\r`);

  try {
    // Try marketplace install first
    const { PluginMarketplace } = await import('../../plugins/plugin-marketplace');
    const marketplace = new PluginMarketplace();

    const result = await marketplace.installPlugin(nameOrId);

    if (!result.success) {
      // If marketplace fails, try local stub creation (non-URL names)
      if (result.error?.includes('not found') && !nameOrId.startsWith('http')) {
        process.stderr.write(' '.repeat(40) + '\\r');

        const localDir = path.join(PLUGIN_DIR, nameOrId);
        fs.mkdirSync(localDir, { recursive: true });

        const manifest = {
          name: nameOrId,
          version: '0.1.0',
          description: 'Custom Xergon plugin',
          main: 'index.js',
          hooks: [],
          enabled: true,
        };

        fs.writeFileSync(
          path.join(localDir, 'plugin.json'),
          JSON.stringify(manifest, null, 2),
          'utf-8',
        );

        const stub = `/**
 * Xergon Plugin: ${nameOrId}
 * Version: ${manifest.version}
 *
 * Define your hooks in the exported \`hooks\` object.
 * Available hooks: before:request, after:response, on:error, on:stream
 */

module.exports.hooks = {
  // 'before:request': (request) => request,
  // 'after:response': (response) => response,
  // 'on:error': (error, context) => console.error(error),
  // 'on:stream': (chunk) => chunk,
};
`;
        fs.writeFileSync(path.join(localDir, 'index.js'), stub, 'utf-8');

        ctx.output.success(`Plugin "${nameOrId}" created locally`);
        ctx.output.info(`Plugin directory: ${localDir}`);
        ctx.output.info('Edit plugin.json to configure hooks, then edit index.js to implement them.');
        return;
      }

      process.stderr.write(' '.repeat(40) + '\\r');
      ctx.output.writeError(`Failed to install plugin: ${result.error}`);
      process.exit(1);
      return;
    }

    process.stderr.write(' '.repeat(40) + '\\r');
    ctx.output.success(`Plugin "${nameOrId}" installed from marketplace`);
    ctx.output.info(`Plugin directory: ${result.pluginDir}`);
  } catch (err) {
    process.stderr.write(' '.repeat(40) + '\\r');
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to install plugin: ${message}`);
    process.exit(1);
  }
}

// ── remove ─────────────────────────────────────────────────────────

async function handleRemove(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const name = args.positional[1];

  if (!name) {
    ctx.output.writeError('No plugin name specified. Use: xergon plugin remove <name>');
    process.exit(1);
    return;
  }

  const pluginDir = path.join(PLUGIN_DIR, name);

  if (!fs.existsSync(pluginDir)) {
    ctx.output.writeError(`Plugin "${name}" is not installed.`);
    process.exit(1);
    return;
  }

  try {
    fs.rmSync(pluginDir, { recursive: true, force: true });
    ctx.output.success(`Plugin "${name}" removed successfully`);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to remove plugin: ${message}`);
    process.exit(1);
  }
}

// ── enable ─────────────────────────────────────────────────────────

async function handleEnable(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const name = args.positional[1];

  if (!name) {
    ctx.output.writeError('No plugin name specified. Use: xergon plugin enable <name>');
    process.exit(1);
    return;
  }

  const manifestPath = path.join(PLUGIN_DIR, name, 'plugin.json');

  if (!fs.existsSync(manifestPath)) {
    ctx.output.writeError(`Plugin "${name}" is not installed.`);
    process.exit(1);
    return;
  }

  try {
    const manifestRaw = fs.readFileSync(manifestPath, 'utf-8');
    const manifest = JSON.parse(manifestRaw);
    manifest.enabled = true;
    fs.writeFileSync(manifestPath, JSON.stringify(manifest, null, 2), 'utf-8');
    ctx.output.success(`Plugin "${name}" enabled`);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to enable plugin: ${message}`);
    process.exit(1);
  }
}

// ── disable ────────────────────────────────────────────────────────

async function handleDisable(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const name = args.positional[1];

  if (!name) {
    ctx.output.writeError('No plugin name specified. Use: xergon plugin disable <name>');
    process.exit(1);
    return;
  }

  const manifestPath = path.join(PLUGIN_DIR, name, 'plugin.json');

  if (!fs.existsSync(manifestPath)) {
    ctx.output.writeError(`Plugin "${name}" is not installed.`);
    process.exit(1);
    return;
  }

  try {
    const manifestRaw = fs.readFileSync(manifestPath, 'utf-8');
    const manifest = JSON.parse(manifestRaw);
    manifest.enabled = false;
    fs.writeFileSync(manifestPath, JSON.stringify(manifest, null, 2), 'utf-8');
    ctx.output.success(`Plugin "${name}" disabled`);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to disable plugin: ${message}`);
    process.exit(1);
  }
}

// ── search ─────────────────────────────────────────────────────────

async function handleSearch(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const query = args.positional[1];
  const category = args.options.category as string | undefined;
  const sort = (args.options.sort as string) || 'relevance';

  if (!query && !category) {
    ctx.output.writeError('Provide a search query or --category. Use: xergon plugin search <query> [--category cat] [--sort field]');
    process.exit(1);
    return;
  }

  try {
    const { PluginMarketplace } = await import('../../plugins/plugin-marketplace');
    const marketplace = new PluginMarketplace();

    const plugins = await marketplace.searchPlugins({
      query,
      category,
      sort: sort as any,
    });

    if (plugins.length === 0) {
      ctx.output.info('No plugins found matching your search.');
      return;
    }

    const tableData = plugins.map(p => ({
      ID: p.id.length > 12 ? p.id.slice(0, 12) + '...' : p.id,
      Name: p.name,
      Version: p.version,
      Category: p.category,
      Downloads: String(p.downloads),
      Rating: `${p.rating.toFixed(1)}`,
      Verified: p.verified ? ctx.output.colorize('yes', 'green') : 'no',
    }));
    ctx.output.write(ctx.output.formatTable(tableData, `Marketplace Results (${plugins.length})`));
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Search failed: ${message}`);
    process.exit(1);
  }
}

// ── info ───────────────────────────────────────────────────────────

async function handleInfo(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const nameOrId = args.positional[1];

  if (!nameOrId) {
    ctx.output.writeError('No plugin name or ID specified. Use: xergon plugin info <name|id>');
    process.exit(1);
    return;
  }

  try {
    const { PluginMarketplace } = await import('../../plugins/plugin-marketplace');
    const marketplace = new PluginMarketplace();

    const plugin = await marketplace.getPlugin(nameOrId);

    if (!plugin) {
      ctx.output.writeError(`Plugin "${nameOrId}" not found in marketplace.`);
      return;
    }

    ctx.output.write(`  ${ctx.output.colorize(plugin.name, 'bold')} v${plugin.version}`);
    ctx.output.write(`  ${plugin.description}`);
    ctx.output.write('');
    ctx.output.write(`  Author:      ${plugin.author}`);
    ctx.output.write(`  Category:    ${plugin.category}`);
    ctx.output.write(`  Downloads:   ${plugin.downloads}`);
    ctx.output.write(`  Rating:      ${plugin.rating.toFixed(1)} (${plugin.reviews} reviews)`);
    ctx.output.write(`  Verified:    ${plugin.verified ? ctx.output.colorize('yes', 'green') : 'no'}`);
    ctx.output.write(`  Published:   ${plugin.publishedAt}`);
    ctx.output.write(`  Updated:     ${plugin.updatedAt}`);
    if (plugin.repository) {
      ctx.output.write(`  Repository:  ${plugin.repository}`);
    }
    if (plugin.license) {
      ctx.output.write(`  License:     ${plugin.license}`);
    }
    ctx.output.write(`  Hooks:       ${plugin.hooks.join(', ') || 'none'}`);
    if (plugin.keywords?.length) {
      ctx.output.write(`  Keywords:    ${plugin.keywords.join(', ')}`);
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to get plugin info: ${message}`);
    process.exit(1);
  }
}

// ── update ─────────────────────────────────────────────────────────

async function handleUpdate(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const name = args.positional[1];

  try {
    const { PluginMarketplace } = await import('../../plugins/plugin-marketplace');
    const marketplace = new PluginMarketplace();

    if (name) {
      // Update a specific plugin
      const result = await marketplace.updatePlugin(name);

      if (!result.success) {
        ctx.output.writeError(`Failed to update "${name}": ${result.error}`);
        process.exit(1);
        return;
      }

      if (result.updated) {
        ctx.output.success(`Plugin "${name}" updated to v${result.version}`);
      } else {
        ctx.output.info(`Plugin "${name}" is already at the latest version (v${result.version})`);
      }
    } else {
      // Update all installed plugins
      const installed = marketplace.listInstalled();
      if (installed.length === 0) {
        ctx.output.info('No plugins installed.');
        return;
      }

      let updatedCount = 0;
      let skippedCount = 0;
      let failedCount = 0;

      for (const plugin of installed) {
        if (!plugin.marketplaceId) {
          skippedCount++;
          continue;
        }

        ctx.output.write(`  Checking ${plugin.name}...`);
        const result = await marketplace.updatePlugin(plugin.name);

        if (!result.success) {
          ctx.output.writeError(`    Failed: ${result.error}`);
          failedCount++;
        } else if (result.updated) {
          ctx.output.success(`    Updated to v${result.version}`);
          updatedCount++;
        } else {
          ctx.output.info(`    Already up to date (v${result.version})`);
          skippedCount++;
        }
      }

      ctx.output.write('');
      ctx.output.write(`  ${ctx.output.colorize(String(updatedCount), 'green')} updated, ${skippedCount} up to date, ${ctx.output.colorize(String(failedCount), failedCount > 0 ? 'red' : 'dim')} failed`);
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Update failed: ${message}`);
    process.exit(1);
  }
}

// ── publish ────────────────────────────────────────────────────────

async function handlePublish(_args: ParsedArgs, ctx: CLIContext): Promise<void> {
  // Look for plugin.json in the current directory
  const manifestPath = path.join(process.cwd(), 'plugin.json');

  if (!fs.existsSync(manifestPath)) {
    ctx.output.writeError('No plugin.json found in the current directory.');
    ctx.output.info('Create a plugin.json manifest to publish your plugin.');
    process.exit(1);
    return;
  }

  try {
    const { PluginMarketplace } = await import('../../plugins/plugin-marketplace');
    const marketplace = new PluginMarketplace();

    const thinkingMsg = ctx.output.colorize('Publishing plugin', 'cyan');
    process.stderr.write(`${thinkingMsg}...\\r`);

    const result = await marketplace.publishPlugin(manifestPath);

    process.stderr.write(' '.repeat(40) + '\\r');

    if (!result.success) {
      ctx.output.writeError(`Publish failed: ${result.error}`);
      process.exit(1);
      return;
    }

    ctx.output.success(`Plugin published successfully! ID: ${result.id}`);
  } catch (err) {
    process.stderr.write(' '.repeat(40) + '\\r');
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Publish failed: ${message}`);
    process.exit(1);
  }
}

// ── reviews ────────────────────────────────────────────────────────

async function handleReviews(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const nameOrId = args.positional[1];

  if (!nameOrId) {
    ctx.output.writeError('No plugin name or ID specified. Use: xergon plugin reviews <name|id>');
    process.exit(1);
    return;
  }

  try {
    const { PluginMarketplace } = await import('../../plugins/plugin-marketplace');
    const marketplace = new PluginMarketplace();

    const reviews = await marketplace.getPluginReviews(nameOrId);

    if (reviews.length === 0) {
      ctx.output.info('No reviews found for this plugin.');
      return;
    }

    const tableData = reviews.map(r => ({
      Rating: `${'★'.repeat(r.rating)}${'☆'.repeat(5 - r.rating)}`,
      Author: r.author.length > 16 ? r.author.slice(0, 16) + '...' : r.author,
      Comment: r.comment.length > 60 ? r.comment.slice(0, 60) + '...' : r.comment,
      Date: r.createdAt.split('T')[0],
    }));
    ctx.output.write(ctx.output.formatTable(tableData, `Reviews for "${nameOrId}" (${reviews.length})`));
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to get reviews: ${message}`);
    process.exit(1);
  }
}

// ── categories ─────────────────────────────────────────────────────

async function handleCategories(_args: ParsedArgs, ctx: CLIContext): Promise<void> {
  try {
    const { PluginMarketplace } = await import('../../plugins/plugin-marketplace');
    const marketplace = new PluginMarketplace();

    const categories = await marketplace.getCategories();

    ctx.output.write(ctx.output.colorize('Plugin Categories:', 'bold'));
    for (const cat of categories) {
      ctx.output.write(`  ${ctx.output.colorize('•', 'cyan')} ${cat}`);
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to get categories: ${message}`);
    process.exit(1);
  }
}

// ── Helpers ────────────────────────────────────────────────────────

interface InstalledPluginInfo {
  name: string;
  version: string;
  description?: string;
  enabled: boolean;
  hooks: string[];
}

function getInstalledPlugins(): InstalledPluginInfo[] {
  if (!fs.existsSync(PLUGIN_DIR)) {
    return [];
  }

  const plugins: InstalledPluginInfo[] = [];
  const entries = fs.readdirSync(PLUGIN_DIR, { withFileTypes: true });

  for (const entry of entries) {
    if (!entry.isDirectory()) continue;

    const manifestPath = path.join(PLUGIN_DIR, entry.name, 'plugin.json');
    if (!fs.existsSync(manifestPath)) continue;

    try {
      const manifestRaw = fs.readFileSync(manifestPath, 'utf-8');
      const manifest = JSON.parse(manifestRaw);
      plugins.push({
        name: manifest.name || entry.name,
        version: manifest.version || '0.0.0',
        description: manifest.description,
        enabled: manifest.enabled !== false,
        hooks: Array.isArray(manifest.hooks) ? manifest.hooks : [],
      });
    } catch {
      // Skip malformed manifests
    }
  }

  return plugins;
}

export const pluginCommand: Command = {
  name: 'plugin',
  description: 'Manage Xergon SDK plugins and marketplace',
  aliases: ['plugins'],
  options: [],
  action: pluginAction,
};
