/**
 * CLI command: template
 *
 * Manage prompt templates -- list, show, use, create, remove,
 * and marketplace operations (search, download, publish, fork, rate, etc.).
 *
 * Usage:
 *   xergon template list
 *   xergon template show <name>
 *   xergon template use <name> --var <key>=<value> ...
 *   xergon template create --name X --template "..." --vars a,b
 *   xergon template remove <name>
 *   xergon template search <query>
 *   xergon template info <id>
 *   xergon template download <id>
 *   xergon template publish --name X --template "..." --vars a,b
 *   xergon template update <id> --template "..."
 *   xergon template unpublish <id>
 *   xergon template fork <id>
 *   xergon template rate <id> --rating 5
 *   xergon template my
 *   xergon template popular
 *   xergon template trending
 *   xergon template categories
 */

import type { Command, CommandOption, ParsedArgs, CLIContext } from '../mod';
import {
  listTemplates,
  getTemplate,
  renderTemplate,
  addTemplate,
  removeTemplate,
} from '../../prompt-templates';
import type { PromptTemplate } from '../../prompt-templates';
import {
  searchTemplates,
  getTemplate as getMarketplaceTemplate,
  downloadTemplate as downloadMarketplaceTemplate,
  publishTemplate,
  updatePublishedTemplate,
  unpublishTemplate,
  forkTemplate,
  rateTemplate,
  getTemplateReviews,
  getMyTemplates,
  getPopularTemplates,
  getTrendingTemplates,
  getVerifiedTemplates,
  getCategories,
} from '../../template-marketplace';
import type { TemplateCategory, SharedTemplate } from '../../template-marketplace';

const templateOptions: CommandOption[] = [
  {
    name: 'json',
    short: '',
    long: '--json',
    description: 'Output as JSON',
    required: false,
    type: 'boolean',
  },
  {
    name: 'name',
    short: '',
    long: '--name',
    description: 'Template name (for create/publish)',
    required: false,
    type: 'string',
  },
  {
    name: 'template',
    short: '',
    long: '--template',
    description: 'Template content with {{var}} placeholders (for create/publish)',
    required: false,
    type: 'string',
  },
  {
    name: 'vars',
    short: '',
    long: '--vars',
    description: 'Comma-separated variable names (for create/publish)',
    required: false,
    type: 'string',
  },
  {
    name: 'description',
    short: '',
    long: '--description',
    description: 'Template description (for create/publish)',
    required: false,
    type: 'string',
  },
  {
    name: 'category',
    short: '',
    long: '--category',
    description: 'Template category (for create/publish)',
    required: false,
    type: 'string',
    default: 'custom',
  },
  {
    name: 'var',
    short: '',
    long: '--var',
    description: 'Variable assignment: --var key=value (for use)',
    required: false,
    type: 'string',
  },
  {
    name: 'rating',
    short: '',
    long: '--rating',
    description: 'Rating 1-5 (for rate)',
    required: false,
    type: 'number',
  },
  {
    name: 'comment',
    short: '',
    long: '--comment',
    description: 'Review comment (for rate)',
    required: false,
    type: 'string',
  },
  {
    name: 'author',
    short: '',
    long: '--author',
    description: 'Author address (for publish/rate)',
    required: false,
    type: 'string',
  },
  {
    name: 'authorName',
    short: '',
    long: '--author-name',
    description: 'Author display name (for publish)',
    required: false,
    type: 'string',
  },
  {
    name: 'tags',
    short: '',
    long: '--tags',
    description: 'Comma-separated tags (for publish)',
    required: false,
    type: 'string',
  },
  {
    name: 'limit',
    short: '',
    long: '--limit',
    description: 'Max results to show (for search/popular/trending)',
    required: false,
    type: 'number',
  },
  {
    name: 'newName',
    short: '',
    long: '--new-name',
    description: 'New name for forked template (for fork)',
    required: false,
    type: 'string',
  },
  {
    name: 'overwrite',
    short: '',
    long: '--overwrite',
    description: 'Overwrite local template if it exists (for download)',
    required: false,
    type: 'boolean',
  },
  {
    name: 'token',
    short: '',
    long: '--token',
    description: 'Auth token for marketplace operations',
    required: false,
    type: 'string',
  },
];

async function templateAction(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const sub = args.positional[0];

  if (!sub) {
    ctx.output.writeError('Usage: xergon template <list|show|use|create|remove|search|info|download|publish|update|unpublish|fork|rate|my|popular|trending|categories> [args]');
    ctx.output.info('Run "xergon template --help" for details.');
    process.exit(1);
    return;
  }

  switch (sub) {
    case 'list':
    case 'ls':
      await handleList(args, ctx);
      break;
    case 'show':
      await handleShow(args, ctx);
      break;
    case 'use':
    case 'render':
      await handleUse(args, ctx);
      break;
    case 'create':
    case 'add':
    case 'new':
      await handleCreate(args, ctx);
      break;
    case 'remove':
    case 'rm':
    case 'delete':
      await handleRemove(args, ctx);
      break;
    case 'search':
    case 'find':
      await handleSearch(args, ctx);
      break;
    case 'info':
    case 'marketplace-info':
      await handleMarketplaceInfo(args, ctx);
      break;
    case 'download':
    case 'install':
      await handleDownload(args, ctx);
      break;
    case 'publish':
    case 'share':
      await handlePublish(args, ctx);
      break;
    case 'update':
      await handleUpdate(args, ctx);
      break;
    case 'unpublish':
    case 'remove-marketplace':
      await handleUnpublish(args, ctx);
      break;
    case 'fork':
      await handleFork(args, ctx);
      break;
    case 'rate':
    case 'review':
      await handleRate(args, ctx);
      break;
    case 'my':
    case 'mine':
      await handleMyTemplates(args, ctx);
      break;
    case 'popular':
      await handlePopular(args, ctx);
      break;
    case 'trending':
      await handleTrending(args, ctx);
      break;
    case 'categories':
    case 'cats':
      await handleCategories(args, ctx);
      break;
    case 'verified':
      await handleVerified(args, ctx);
      break;
    default:
      ctx.output.writeError(`Unknown template subcommand: ${sub}`);
      ctx.output.info('Available: list, show, use, create, remove, search, info, download, publish, update, unpublish, fork, rate, my, popular, trending, categories, verified');
      process.exit(1);
  }
}

// ── list ───────────────────────────────────────────────────────────

async function handleList(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const outputJson = args.options.json === true;
  const templates = listTemplates();

  if (outputJson) {
    ctx.output.setFormat('json');
    ctx.output.write(ctx.output.formatOutput(templates));
    return;
  }

  const o = ctx.output;

  if (templates.length === 0) {
    o.info('No templates available.');
    return;
  }

  o.write(o.colorize('Prompt Templates', 'bold'));
  o.write(o.colorize('═══════════════════════════════════════════════════════════════════', 'dim'));
  o.write('');

  // Group by category
  const byCategory = new Map<string, PromptTemplate[]>();
  for (const t of templates) {
    const cat = t.category;
    if (!byCategory.has(cat)) byCategory.set(cat, []);
    byCategory.get(cat)!.push(t);
  }

  for (const [category, items] of byCategory) {
    o.write(o.colorize(`  ${category.toUpperCase()}`, 'cyan'));
    for (const t of items) {
      const vars = t.variables.length > 0
        ? o.colorize(` [${t.variables.join(', ')}]`, 'dim')
        : '';
      o.write(`    ${o.colorize(t.name, 'green')}${vars}`);
      o.write(`      ${t.description}`);
    }
    o.write('');
  }

  o.info(`${templates.length} template(s) available.`);
}

// ── show ───────────────────────────────────────────────────────────

async function handleShow(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const name = args.positional[1];
  const outputJson = args.options.json === true;

  if (!name) {
    ctx.output.writeError('Usage: xergon template show <name>');
    process.exit(1);
    return;
  }

  const template = getTemplate(name);
  if (!template) {
    ctx.output.writeError(`Template not found: ${name}`);
    ctx.output.info(`Available: ${listTemplates().map(t => t.name).join(', ')}`);
    process.exit(1);
    return;
  }

  if (outputJson) {
    ctx.output.setFormat('json');
    ctx.output.write(ctx.output.formatOutput(template));
    return;
  }

  const o = ctx.output;
  o.write('');
  o.write(o.colorize(template.name, 'bold'));
  o.write(o.colorize('─'.repeat(template.name.length), 'dim'));
  o.write(`  ${o.colorize('Description:', 'cyan')}  ${template.description}`);
  o.write(`  ${o.colorize('Category:', 'cyan')}    ${template.category}`);
  o.write(`  ${o.colorize('Variables:', 'cyan')}   ${template.variables.join(', ') || '(none)'}`);
  o.write('');
  o.write(o.colorize('  Template:', 'cyan'));
  o.write('');
  // Indent the template for readability
  for (const line of template.template.split('\n')) {
    o.write(`    ${line}`);
  }
  o.write('');
}

// ── use ────────────────────────────────────────────────────────────

async function handleUse(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const name = args.positional[1];

  if (!name) {
    ctx.output.writeError('Usage: xergon template use <name> --var key=value ...');
    process.exit(1);
    return;
  }

  // Parse --var key=value flags
  const variables: Record<string, string> = {};
  const varFlag = args.options.var;
  if (varFlag) {
    // The parser may give us a single string or we may need to handle multiple
    const varStr = String(varFlag);
    const eqIdx = varStr.indexOf('=');
    if (eqIdx !== -1) {
      const key = varStr.substring(0, eqIdx).trim();
      const value = varStr.substring(eqIdx + 1).trim();
      if (key) variables[key] = value;
    }
  }

  // Also check for var assignments in positional args (e.g., --var key=value repeated)
  // The parser collects them; we handle the primary one above.
  // For additional vars, users can pass multiple --var flags which get accumulated
  // by the parser into the options object. Since our parser overwrites, we'll
  // also scan raw args.
  const rawArgs = process.argv.slice(process.argv.indexOf('use') + 1);
  for (let i = 0; i < rawArgs.length; i++) {
    if (rawArgs[i] === '--var' && rawArgs[i + 1]) {
      const assignment = rawArgs[i + 1];
      const eqIdx = assignment.indexOf('=');
      if (eqIdx !== -1) {
        const key = assignment.substring(0, eqIdx).trim();
        const value = assignment.substring(eqIdx + 1).trim();
        if (key) variables[key] = value;
      }
      i++; // skip the value
    }
  }

  try {
    const rendered = renderTemplate(name, variables);

    // Send as a chat completion
    const model = String(ctx.config.defaultModel || 'llama-3.3-70b');
    const messages = [
      { role: 'system' as const, content: rendered.system },
      { role: 'user' as const, content: rendered.user },
    ];

    const thinkingMsg = ctx.output.colorize('Thinking', 'cyan');
    let dots = 0;
    const spinner = setInterval(() => {
      dots = (dots + 1) % 4;
      process.stderr.write(`\r${thinkingMsg}${'.'.repeat(dots)}   `);
    }, 300);

    try {
      const response = await ctx.client.chat.completions.create({
        model,
        messages,
      });

      clearInterval(spinner);
      process.stderr.write('\r' + ' '.repeat(20) + '\r');

      const content = response.choices?.[0]?.message?.content || '(no content)';
      ctx.output.write(content + '\n');

      if (response.usage) {
        ctx.output.info(
          `Tokens: ${response.usage.promptTokens} prompt + ${response.usage.completionTokens} completion = ${response.usage.totalTokens} total`
        );
      }
    } catch (err) {
      clearInterval(spinner);
      process.stderr.write('\r' + ' '.repeat(20) + '\r');
      throw err;
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Template render failed: ${message}`);
    process.exit(1);
  }
}

// ── create ─────────────────────────────────────────────────────────

async function handleCreate(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const name = args.options.name ? String(args.options.name) : undefined;
  const template = args.options.template ? String(args.options.template) : undefined;
  const varsStr = args.options.vars ? String(args.options.vars) : undefined;
  const description = args.options.description ? String(args.options.description) : '';
  const category = (String(args.options.category || 'custom')) as PromptTemplate['category'];

  if (!name) {
    ctx.output.writeError('Usage: xergon template create --name <name> --template "..." --vars a,b,c');
    process.exit(1);
    return;
  }

  if (!template) {
    ctx.output.writeError('Usage: xergon template create --name <name> --template "..." --vars a,b,c');
    ctx.output.info('The --template flag should contain your prompt with {{variable}} placeholders.');
    process.exit(1);
    return;
  }

  // Extract variables from template if --vars not provided
  let variables: string[] = [];
  if (varsStr) {
    variables = varsStr.split(',').map(v => v.trim()).filter(Boolean);
  } else {
    // Auto-detect {{var}} patterns
    const matches = template.match(/\{\{(\w+)\}\}/g);
    if (matches) {
      variables = [...new Set(matches.map(m => m.slice(2, -2)))];
    }
  }

  const validCategories: PromptTemplate['category'][] = ['system', 'creative', 'code', 'analysis', 'custom'];
  if (!validCategories.includes(category)) {
    ctx.output.writeError(`Invalid category: ${category}. Must be one of: ${validCategories.join(', ')}`);
    process.exit(1);
    return;
  }

  try {
    addTemplate({
      name,
      description: description || `Custom template: ${name}`,
      template,
      variables,
      category,
    });
    ctx.output.success(`Template "${name}" created.`);
    ctx.output.info(`Variables: ${variables.join(', ') || '(none)'}`);
    ctx.output.info(`Category: ${category}`);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(message);
    process.exit(1);
  }
}

// ── remove ─────────────────────────────────────────────────────────

async function handleRemove(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const name = args.positional[1];

  if (!name) {
    ctx.output.writeError('Usage: xergon template remove <name>');
    process.exit(1);
    return;
  }

  try {
    const removed = removeTemplate(name);
    if (removed) {
      ctx.output.success(`Template "${name}" removed.`);
    } else {
      ctx.output.writeError(`Template not found: ${name}`);
      process.exit(1);
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(message);
    process.exit(1);
  }
}

// ── Marketplace: search ────────────────────────────────────────────

async function handleSearch(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const query = args.positional.slice(1).join(' ') || undefined;
  const outputJson = args.options.json === true;
  const category = args.options.category ? String(args.options.category) as TemplateCategory : undefined;
  const limit = args.options.limit ? Number(args.options.limit) : 20;

  if (!query) {
    ctx.output.writeError('Usage: xergon template search <query>');
    ctx.output.info('Search the template marketplace by keywords.');
    process.exit(1);
    return;
  }

  try {
    const results = await searchTemplates({
      query,
      category,
      limit,
      sort: 'popular',
    });

    if (outputJson) {
      ctx.output.setFormat('json');
      ctx.output.write(ctx.output.formatOutput(results));
      return;
    }

    const o = ctx.output;

    if (results.length === 0) {
      o.info(`No templates found for "${query}".`);
      return;
    }

    o.write(o.colorize(`Marketplace Search: "${query}"`, 'bold'));
    o.write(o.colorize('─'.repeat(40), 'dim'));
    o.write('');

    for (const tpl of results) {
      const verified = tpl.verified ? o.colorize(' [verified]', 'green') : '';
      const rating = formatRating(tpl.rating, o);
      o.write(`  ${o.colorize(tpl.name, 'green')}${verified}  ${rating}`);
      o.write(`    ${o.colorize(tpl.id, 'dim')}`);
      o.write(`    ${tpl.description}`);
      const meta = [
        o.colorize(tpl.category, 'cyan'),
        `${tpl.downloads} downloads`,
        `${tpl.forkCount} forks`,
      ];
      if (tpl.tags.length > 0) {
        meta.push(tpl.tags.slice(0, 3).map(t => `#${t}`).join(' '));
      }
      o.write(`    ${meta.join('  ·  ')}`);
      o.write('');
    }

    o.info(`${results.length} template(s) found.`);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Search failed: ${message}`);
    process.exit(1);
  }
}

// ── Marketplace: info ──────────────────────────────────────────────

async function handleMarketplaceInfo(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const id = args.positional[1];
  const outputJson = args.options.json === true;

  if (!id) {
    ctx.output.writeError('Usage: xergon template info <id>');
    process.exit(1);
    return;
  }

  try {
    const tpl = await getMarketplaceTemplate(id);

    if (outputJson) {
      ctx.output.setFormat('json');
      ctx.output.write(ctx.output.formatOutput(tpl));
      return;
    }

    const o = ctx.output;
    o.write('');
    o.write(o.colorize(tpl.name, 'bold'));
    o.write(o.colorize('─'.repeat(tpl.name.length), 'dim'));
    o.write(`  ${o.colorize('ID:', 'cyan')}          ${tpl.id}`);
    o.write(`  ${o.colorize('Description:', 'cyan')}  ${tpl.description}`);
    o.write(`  ${o.colorize('Author:', 'cyan')}      ${tpl.authorName} (${o.colorize(tpl.author, 'dim')})`);
    o.write(`  ${o.colorize('Category:', 'cyan')}    ${tpl.category}`);
    o.write(`  ${o.colorize('Variables:', 'cyan')}   ${tpl.variables.join(', ') || '(none)'}`);
    o.write(`  ${o.colorize('Version:', 'cyan')}     ${tpl.version}`);
    o.write(`  ${o.colorize('Rating:', 'cyan')}      ${formatRating(tpl.rating, o)} (${tpl.downloads} downloads)`);
    o.write(`  ${o.colorize('Forks:', 'cyan')}       ${tpl.forkCount}`);
    o.write(`  ${o.colorize('Verified:', 'cyan')}    ${tpl.verified ? o.colorize('Yes', 'green') : o.colorize('No', 'dim')}`);
    o.write(`  ${o.colorize('Public:', 'cyan')}      ${tpl.isPublic ? 'Yes' : 'No'}`);
    if (tpl.sourceTemplateId) {
      o.write(`  ${o.colorize('Forked from:', 'cyan')} ${tpl.sourceTemplateId}`);
    }
    if (tpl.tags.length > 0) {
      o.write(`  ${o.colorize('Tags:', 'cyan')}       ${tpl.tags.join(', ')}`);
    }
    o.write(`  ${o.colorize('Created:', 'cyan')}     ${tpl.createdAt}`);
    o.write(`  ${o.colorize('Updated:', 'cyan')}     ${tpl.updatedAt}`);
    o.write('');
    o.write(o.colorize('  Template Content:', 'cyan'));
    o.write('');
    for (const line of tpl.template.split('\n')) {
      o.write(`    ${line}`);
    }
    o.write('');

    // Show reviews
    try {
      const reviews = await getTemplateReviews(id);
      if (reviews.length > 0) {
        o.write(o.colorize('  Reviews:', 'cyan'));
        o.write('');
        for (const review of reviews.slice(0, 5)) {
          o.write(`    ${formatRating(review.rating, o)}  ${o.colorize(review.author, 'dim')}`);
          if (review.comment) {
            o.write(`    "${review.comment}"`);
          }
          o.write('');
        }
      }
    } catch {
      // Reviews are optional, don't fail on error
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to get template: ${message}`);
    process.exit(1);
  }
}

// ── Marketplace: download ──────────────────────────────────────────

async function handleDownload(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const id = args.positional[1];
  const overwrite = args.options.overwrite === true;

  if (!id) {
    ctx.output.writeError('Usage: xergon template download <id>');
    process.exit(1);
    return;
  }

  try {
    const localName = await downloadMarketplaceTemplate(id, { overwrite });
    ctx.output.success(`Template downloaded and installed as "${localName}".`);
    ctx.output.info('Use it with: xergon template use ' + localName);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Download failed: ${message}`);
    process.exit(1);
  }
}

// ── Marketplace: publish ───────────────────────────────────────────

async function handlePublish(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const name = args.options.name ? String(args.options.name) : undefined;
  const template = args.options.template ? String(args.options.template) : undefined;
  const varsStr = args.options.vars ? String(args.options.vars) : undefined;
  const description = args.options.description ? String(args.options.description) : undefined;
  const category = args.options.category ? String(args.options.category) as TemplateCategory : undefined;
  const author = args.options.author ? String(args.options.author) : undefined;
  const authorName = args.options.authorName ? String(args.options.authorName) : undefined;
  const tagsStr = args.options.tags ? String(args.options.tags) : undefined;
  const token = args.options.token ? String(args.options.token) : undefined;

  if (!name) {
    ctx.output.writeError('Usage: xergon template publish --name <name> --template "..." --description "..." --author <address>');
    process.exit(1);
    return;
  }

  if (!template) {
    ctx.output.writeError('Usage: xergon template publish --name <name> --template "..." --description "..." --author <address>');
    process.exit(1);
    return;
  }

  if (!description) {
    ctx.output.writeError('The --description flag is required for publishing.');
    process.exit(1);
    return;
  }

  if (!author) {
    ctx.output.writeError('The --author flag is required for publishing (your wallet address or username).');
    process.exit(1);
    return;
  }

  // Extract variables if --vars not provided
  let variables: string[] = [];
  if (varsStr) {
    variables = varsStr.split(',').map(v => v.trim()).filter(Boolean);
  } else {
    const matches = template.match(/\{\{(\w+)\}\}/g);
    if (matches) {
      variables = [...new Set(matches.map(m => m.slice(2, -2)))];
    }
  }

  let tags: string[] = [];
  if (tagsStr) {
    tags = tagsStr.split(',').map(t => t.trim()).filter(Boolean);
  }

  try {
    const result = await publishTemplate(
      { name, template, variables },
      {
        author,
        authorName,
        description,
        category: category || 'custom',
        tags,
      },
      { authToken: token },
    );

    ctx.output.success(`Template "${name}" published to marketplace.`);
    ctx.output.info(`ID: ${result.id}`);
    ctx.output.info(`Category: ${result.category}`);
    if (tags.length > 0) {
      ctx.output.info(`Tags: ${tags.join(', ')}`);
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Publish failed: ${message}`);
    process.exit(1);
  }
}

// ── Marketplace: update ────────────────────────────────────────────

async function handleUpdate(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const id = args.positional[1];
  const template = args.options.template ? String(args.options.template) : undefined;
  const description = args.options.description ? String(args.options.description) : undefined;
  const category = args.options.category ? String(args.options.category) as TemplateCategory : undefined;
  const tagsStr = args.options.tags ? String(args.options.tags) : undefined;
  const token = args.options.token ? String(args.options.token) : undefined;

  if (!id) {
    ctx.output.writeError('Usage: xergon template update <id> [--template "..."] [--description "..."]');
    process.exit(1);
    return;
  }

  const updates: Record<string, unknown> = {};
  if (template) updates.template = template;
  if (description) updates.description = description;
  if (category) updates.category = category;
  if (tagsStr) updates.tags = tagsStr.split(',').map(t => t.trim()).filter(Boolean);

  if (Object.keys(updates).length === 0) {
    ctx.output.writeError('Provide at least one field to update (--template, --description, --category, --tags).');
    process.exit(1);
    return;
  }

  try {
    const result = await updatePublishedTemplate(
      id,
      updates as Partial<Pick<SharedTemplate, 'name' | 'template' | 'variables' | 'description' | 'category' | 'tags' | 'isPublic'>>,
      { authToken: token },
    );
    ctx.output.success(`Template "${result.name}" updated.`);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Update failed: ${message}`);
    process.exit(1);
  }
}

// ── Marketplace: unpublish ─────────────────────────────────────────

async function handleUnpublish(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const id = args.positional[1];
  const token = args.options.token ? String(args.options.token) : undefined;

  if (!id) {
    ctx.output.writeError('Usage: xergon template unpublish <id>');
    process.exit(1);
    return;
  }

  try {
    await unpublishTemplate(id, { authToken: token });
    ctx.output.success(`Template ${id} unpublished from marketplace.`);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Unpublish failed: ${message}`);
    process.exit(1);
  }
}

// ── Marketplace: fork ──────────────────────────────────────────────

async function handleFork(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const id = args.positional[1];
  const author = args.options.author ? String(args.options.author) : undefined;
  const authorName = args.options.authorName ? String(args.options.authorName) : undefined;
  const newName = args.options.newName ? String(args.options.newName) : undefined;
  const token = args.options.token ? String(args.options.token) : undefined;

  if (!id) {
    ctx.output.writeError('Usage: xergon template fork <id> --author <address>');
    process.exit(1);
    return;
  }

  if (!author) {
    ctx.output.writeError('The --author flag is required for forking (your wallet address or username).');
    process.exit(1);
    return;
  }

  try {
    const result = await forkTemplate(
      id,
      { author, authorName, newName },
      { authToken: token },
    );
    ctx.output.success(`Forked template as "${result.name}" (ID: ${result.id}).`);
    ctx.output.info('The template is installed locally and published to the marketplace.');
    ctx.output.info('Use it with: xergon template use ' + result.name);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Fork failed: ${message}`);
    process.exit(1);
  }
}

// ── Marketplace: rate ──────────────────────────────────────────────

async function handleRate(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const id = args.positional[1];
  const rating = args.options.rating ? Number(args.options.rating) : undefined;
  const comment = args.options.comment ? String(args.options.comment) : undefined;
  const author = args.options.author ? String(args.options.author) : undefined;
  const token = args.options.token ? String(args.options.token) : undefined;

  if (!id) {
    ctx.output.writeError('Usage: xergon template rate <id> --rating <1-5> [--comment "..."]');
    process.exit(1);
    return;
  }

  if (!rating || rating < 1 || rating > 5) {
    ctx.output.writeError('Rating must be between 1 and 5. Use --rating N.');
    process.exit(1);
    return;
  }

  try {
    const review = await rateTemplate(id, rating, comment, {
      authToken: token,
      author,
    });
    ctx.output.success(`Rating submitted: ${'*'.repeat(review.rating)}${'·'.repeat(5 - review.rating)} (${review.rating}/5)`);
    if (comment) {
      ctx.output.info(`Comment: "${comment}"`);
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Rating failed: ${message}`);
    process.exit(1);
  }
}

// ── Marketplace: my templates ──────────────────────────────────────

async function handleMyTemplates(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const outputJson = args.options.json === true;
  const author = args.options.author ? String(args.options.author) : undefined;

  if (!author) {
    ctx.output.writeError('Usage: xergon template my --author <address>');
    process.exit(1);
    return;
  }

  try {
    const results = await getMyTemplates(author);

    if (outputJson) {
      ctx.output.setFormat('json');
      ctx.output.write(ctx.output.formatOutput(results));
      return;
    }

    const o = ctx.output;

    if (results.length === 0) {
      o.info('No templates published yet.');
      return;
    }

    o.write(o.colorize('My Published Templates', 'bold'));
    o.write(o.colorize('─'.repeat(40), 'dim'));
    o.write('');

    for (const tpl of results) {
      const rating = formatRating(tpl.rating, o);
      o.write(`  ${o.colorize(tpl.name, 'green')}  ${rating}`);
      o.write(`    ${o.colorize(tpl.id, 'dim')}`);
      o.write(`    ${tpl.description}`);
      o.write(`    ${tpl.category}  ·  ${tpl.downloads} downloads  ·  ${tpl.forkCount} forks`);
      o.write('');
    }

    o.info(`${results.length} template(s) published.`);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to list templates: ${message}`);
    process.exit(1);
  }
}

// ── Marketplace: popular ───────────────────────────────────────────

async function handlePopular(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const outputJson = args.options.json === true;
  const limit = args.options.limit ? Number(args.options.limit) : 10;
  const category = args.options.category ? String(args.options.category) as TemplateCategory : undefined;

  try {
    const results = await getPopularTemplates(limit, { category });

    if (outputJson) {
      ctx.output.setFormat('json');
      ctx.output.write(ctx.output.formatOutput(results));
      return;
    }

    const o = ctx.output;

    if (results.length === 0) {
      o.info('No popular templates found.');
      return;
    }

    o.write(o.colorize('Popular Templates', 'bold'));
    o.write(o.colorize('─'.repeat(40), 'dim'));
    o.write('');

    for (const tpl of results) {
      const verified = tpl.verified ? o.colorize(' [verified]', 'green') : '';
      const rating = formatRating(tpl.rating, o);
      o.write(`  ${o.colorize(tpl.name, 'green')}${verified}  ${rating}`);
      o.write(`    ${tpl.description}`);
      o.write(`    ${tpl.category}  ·  ${tpl.downloads} downloads  ·  ${tpl.forkCount} forks`);
      o.write('');
    }

    o.info(`Top ${results.length} template(s) by downloads.`);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to get popular templates: ${message}`);
    process.exit(1);
  }
}

// ── Marketplace: trending ──────────────────────────────────────────

async function handleTrending(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const outputJson = args.options.json === true;
  const limit = args.options.limit ? Number(args.options.limit) : 10;
  const category = args.options.category ? String(args.options.category) as TemplateCategory : undefined;

  try {
    const results = await getTrendingTemplates(limit, { category });

    if (outputJson) {
      ctx.output.setFormat('json');
      ctx.output.write(ctx.output.formatOutput(results));
      return;
    }

    const o = ctx.output;

    if (results.length === 0) {
      o.info('No trending templates found.');
      return;
    }

    o.write(o.colorize('Trending Templates', 'bold'));
    o.write(o.colorize('─'.repeat(40), 'dim'));
    o.write('');

    for (const tpl of results) {
      const verified = tpl.verified ? o.colorize(' [verified]', 'green') : '';
      const rating = formatRating(tpl.rating, o);
      o.write(`  ${o.colorize(tpl.name, 'green')}${verified}  ${rating}`);
      o.write(`    ${tpl.description}`);
      o.write(`    ${tpl.category}  ·  ${tpl.downloads} downloads  ·  ${tpl.forkCount} forks`);
      o.write('');
    }

    o.info(`Top ${results.length} trending template(s).`);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to get trending templates: ${message}`);
    process.exit(1);
  }
}

// ── Marketplace: categories ────────────────────────────────────────

async function handleCategories(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const outputJson = args.options.json === true;

  try {
    const cats = await getCategories();

    if (outputJson) {
      ctx.output.setFormat('json');
      ctx.output.write(ctx.output.formatOutput(cats));
      return;
    }

    const o = ctx.output;
    o.write(o.colorize('Template Categories', 'bold'));
    o.write(o.colorize('─'.repeat(40), 'dim'));
    o.write('');

    for (const cat of cats) {
      const count = o.colorize(`(${cat.count})`, 'dim');
      o.write(`  ${o.colorize(cat.category, 'cyan').padEnd(20)} ${count}`);
    }

    o.write('');
    o.info(`${cats.length} categories.`);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to get categories: ${message}`);
    process.exit(1);
  }
}

// ── Marketplace: verified ──────────────────────────────────────────

async function handleVerified(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const outputJson = args.options.json === true;
  const limit = args.options.limit ? Number(args.options.limit) : 10;
  const category = args.options.category ? String(args.options.category) as TemplateCategory : undefined;

  try {
    const results = await getVerifiedTemplates(limit, { category });

    if (outputJson) {
      ctx.output.setFormat('json');
      ctx.output.write(ctx.output.formatOutput(results));
      return;
    }

    const o = ctx.output;

    if (results.length === 0) {
      o.info('No verified templates found.');
      return;
    }

    o.write(o.colorize('Verified Templates', 'bold'));
    o.write(o.colorize('─'.repeat(40), 'dim'));
    o.write('');

    for (const tpl of results) {
      const rating = formatRating(tpl.rating, o);
      o.write(`  ${o.colorize(tpl.name, 'green')}  ${o.colorize('[verified]', 'green')}  ${rating}`);
      o.write(`    ${tpl.description}`);
      o.write(`    ${tpl.category}  ·  ${tpl.downloads} downloads  ·  ${tpl.forkCount} forks`);
      o.write('');
    }

    o.info(`${results.length} verified template(s).`);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to get verified templates: ${message}`);
    process.exit(1);
  }
}

// ── Helpers ────────────────────────────────────────────────────────

function formatRating(rating: number, o: import('../mod').OutputFormatter): string {
  const stars = Math.round(rating);
  return o.colorize('*'.repeat(stars) + '·'.repeat(5 - stars), 'yellow') + ` ${rating.toFixed(1)}`;
}

export const templateCommand: Command = {
  name: 'template',
  description: 'Manage prompt templates and marketplace',
  aliases: ['tpl'],
  options: templateOptions,
  action: templateAction,
};
