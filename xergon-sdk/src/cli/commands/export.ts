/**
 * CLI command: export / import
 *
 * Export and import Xergon SDK data for portability and backup.
 *
 * Usage:
 *   xergon export all --output backup.json
 *   xergon export conversations --format json
 *   xergon export config --format yaml
 *   xergon import backup.json
 *   xergon import --validate backup.json
 *   xergon export list-scopes
 */

import type { Command, ParsedArgs, CLIContext } from '../mod';
import { ExportFormat } from '../../export';

async function exportAction(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const sub = args.positional[0];

  if (!sub) {
    ctx.output.writeError('Usage: xergon export <all|conversations|aliases|templates|config|credentials|workspaces|import|list-scopes> [options]');
    process.exit(1);
    return;
  }

  switch (sub) {
    case 'all':
    case 'conversations':
    case 'aliases':
    case 'templates':
    case 'config':
    case 'credentials':
    case 'workspaces':
      await handleExport(args, ctx, sub);
      break;
    case 'import':
      await handleImport(args, ctx);
      break;
    case 'list-scopes':
      await handleListScopes(args, ctx);
      break;
    default:
      ctx.output.writeError(`Unknown subcommand: ${sub}`);
      process.exit(1);
  }
}

// ── export ─────────────────────────────────────────────────────────

async function handleExport(args: ParsedArgs, ctx: CLIContext, scope: string): Promise<void> {
  const outputPath = String(args.options.output || `./xergon-export-${scope}.json`);
  const formatStr = String(args.options.format || 'json');
  const encrypt = args.options.encrypt === true;
  const compress = args.options.compress === true;

  let format: ExportFormat;
  switch (formatStr) {
    case 'json': format = ExportFormat.JSON; break;
    case 'yaml': format = ExportFormat.YAML; break;
    case 'csv': format = ExportFormat.CSV; break;
    case 'markdown': format = ExportFormat.Markdown; break;
    case 'sqlite': format = ExportFormat.SQLite; break;
    default:
      ctx.output.writeError(`Invalid format: ${formatStr}. Use json, yaml, csv, markdown, or sqlite.`);
      process.exit(1);
      return;
  }

  const { exportData, getExportSize } = await import('../../export');

  const size = getExportSize([scope as any]);
  ctx.output.info(`Exporting "${scope}" (estimated ${formatBytes(size)})...`);

  try {
    const result = await exportData({
      format,
      include: [scope as any],
      outputPath,
      encrypt,
      compress,
    });

    ctx.output.success(`Export complete`);
    ctx.output.write(`  Path:      ${result.path}`);
    ctx.output.write(`  Size:      ${formatBytes(result.size)}`);
    ctx.output.write(`  Format:    ${result.manifest.format}`);
    ctx.output.write(`  Scopes:    ${result.manifest.scopes.join(', ')}`);
    ctx.output.write(`  Encrypted: ${result.manifest.encrypted}`);
    ctx.output.write(`  Compressed:${result.manifest.compressed}`);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Export failed: ${message}`);
    process.exit(1);
  }
}

// ── import ─────────────────────────────────────────────────────────

async function handleImport(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const filePath = args.positional[1];
  const validateOnly = args.options.validate === true;

  if (!filePath) {
    ctx.output.writeError('Usage: xergon export import <file> [--validate]');
    process.exit(1);
    return;
  }

  const { validateExport, importData } = await import('../../export');

  if (validateOnly) {
    ctx.output.info(`Validating export file: ${filePath}`);
    const result = validateExport(filePath);

    if (result.valid) {
      ctx.output.success('Export file is valid');
      ctx.output.write(ctx.output.formatText({
        Version: result.manifest?.version,
        Exported: result.manifest?.exportedAt,
        Format: result.manifest?.format,
        Scopes: result.manifest?.scopes?.join(', '),
        Encrypted: result.manifest?.encrypted,
        Compressed: result.manifest?.compressed,
      }, 'Validation Result'));
    } else {
      ctx.output.writeError(`Export file is invalid: ${result.error}`);
      process.exit(1);
    }
    return;
  }

  // Validate first
  const validation = validateExport(filePath);
  if (!validation.valid) {
    ctx.output.writeError(`Cannot import: ${validation.error}`);
    process.exit(1);
    return;
  }

  ctx.output.info(`Importing from: ${filePath}`);

  try {
    const result = importData(filePath);

    ctx.output.success('Import complete');
    ctx.output.write(`  Scopes found:  ${result.scopes.join(', ')}`);
    ctx.output.write(`  Imported:      ${result.imported.join(', ')}`);

    if (result.errors.length > 0) {
      ctx.output.write('');
      ctx.output.writeError('Errors:');
      for (const e of result.errors) {
        ctx.output.write(`  - ${e}`);
      }
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Import failed: ${message}`);
    process.exit(1);
  }
}

// ── list-scopes ────────────────────────────────────────────────────

async function handleListScopes(_args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const { listExportScopes } = await import('../../export');
  const scopes = listExportScopes();

  const tableData = scopes.map(s => ({
    Scope: s.name,
    Size: s.estimatedSize,
    Description: s.description,
  }));

  ctx.output.write(ctx.output.formatTable(tableData, 'Export Scopes'));
}

// ── Helpers ─────────────────────────────────────────────────────────

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export const exportCommand: Command = {
  name: 'export',
  description: 'Export and import Xergon SDK data',
  aliases: ['import'],
  options: [
    {
      name: 'output',
      short: '-o',
      long: '--output',
      description: 'Output file path',
      required: false,
      type: 'string',
    },
    {
      name: 'format',
      short: '-f',
      long: '--format',
      description: 'Export format: json, yaml, csv, markdown (default: json)',
      required: false,
      type: 'string',
    },
    {
      name: 'encrypt',
      short: '',
      long: '--encrypt',
      description: 'Encrypt export with XERGON_EXPORT_KEY',
      required: false,
      type: 'boolean',
    },
    {
      name: 'compress',
      short: '',
      long: '--compress',
      description: 'Compress export data',
      required: false,
      type: 'boolean',
    },
    {
      name: 'validate',
      short: '',
      long: '--validate',
      description: 'Validate export file without importing',
      required: false,
      type: 'boolean',
    },
  ],
  action: exportAction,
};
