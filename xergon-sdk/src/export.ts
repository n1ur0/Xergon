/**
 * Xergon SDK -- Data Export / Portability
 *
 * Export and import user data (conversations, aliases, templates, config,
 * credentials, workspaces) in multiple formats with optional encryption
 * and compression.
 *
 * @example
 * ```ts
 * import { exportData, importData, listExportScopes } from '@xergon/sdk';
 *
 * const scopes = listExportScopes();
 * await exportData({ format: 'json', include: ['conversations', 'aliases'], outputPath: './backup.json' });
 * await importData('./backup.json', 'json');
 * ```
 */

import * as fs from 'node:fs';
import * as path from 'node:path';
import * as os from 'node:os';
import * as crypto from 'node:crypto';
import * as zlib from 'node:zlib';

// ── Types ───────────────────────────────────────────────────────────

export enum ExportFormat {
  JSON = 'json',
  YAML = 'yaml',
  CSV = 'csv',
  Markdown = 'markdown',
  SQLite = 'sqlite',
}

export type ExportScope =
  | 'conversations'
  | 'aliases'
  | 'templates'
  | 'config'
  | 'credentials'
  | 'workspaces'
  | 'all';

export interface ExportConfig {
  format: ExportFormat;
  include: ExportScope[];
  outputPath: string;
  encrypt: boolean;
  compress: boolean;
}

export interface ExportManifest {
  version: string;
  exportedAt: string;
  format: ExportFormat;
  scopes: ExportScope[];
  checksum: string;
  encrypted: boolean;
  compressed: boolean;
}

export interface ExportResult {
  path: string;
  size: number;
  manifest: ExportManifest;
}

export interface ScopeInfo {
  name: ExportScope;
  description: string;
  estimatedSize: string;
}

// ── Scope Definitions ───────────────────────────────────────────────

const SCOPE_DEFINITIONS: ScopeInfo[] = [
  { name: 'conversations', description: 'All conversation history and messages', estimatedSize: 'varies' },
  { name: 'aliases', description: 'Model name aliases', estimatedSize: '< 1 KB' },
  { name: 'templates', description: 'Custom prompt templates', estimatedSize: '< 10 KB' },
  { name: 'config', description: 'Configuration files and profiles', estimatedSize: '< 5 KB' },
  { name: 'credentials', description: 'Stored API keys and authentication data', estimatedSize: '< 1 KB' },
  { name: 'workspaces', description: 'Workspace configurations and variables', estimatedSize: '< 5 KB' },
  { name: 'all', description: 'Everything above', estimatedSize: 'varies' },
];

// ── Helpers ─────────────────────────────────────────────────────────

const XERGON_DIR = () => path.join(os.homedir(), '.xergon');

function readFileSafe(filePath: string): string | null {
  try {
    return fs.readFileSync(filePath, 'utf-8');
  } catch {
    return null;
  }
}

function writeFileSafe(filePath: string, data: string): void {
  const dir = path.dirname(filePath);
  if (!fs.existsSync(dir)) {
    fs.mkdirSync(dir, { recursive: true });
  }
  fs.writeFileSync(filePath, data, 'utf-8');
}

function computeChecksum(data: string): string {
  return crypto.createHash('sha256').update(data).digest('hex');
}

/**
 * Gather data for a specific scope.
 */
function gatherScope(scope: ExportScope): Record<string, unknown> {
  const dir = XERGON_DIR();

  switch (scope) {
    case 'conversations': {
      const raw = readFileSafe(path.join(dir, 'conversations.json'));
      return { conversations: raw ? JSON.parse(raw) : {} };
    }
    case 'aliases': {
      const raw = readFileSafe(path.join(dir, 'aliases.json'));
      return { aliases: raw ? JSON.parse(raw) : [] };
    }
    case 'templates': {
      const raw = readFileSafe(path.join(dir, 'templates.json'));
      return { templates: raw ? JSON.parse(raw) : [] };
    }
    case 'config': {
      const config = readFileSafe(path.join(dir, 'config.json'));
      const profiles = readFileSafe(path.join(dir, 'profiles.json'));
      return { config: config ? JSON.parse(config) : {}, profiles: profiles ? JSON.parse(profiles) : {} };
    }
    case 'credentials': {
      const creds = readFileSafe(path.join(dir, 'credentials.json'));
      return { credentials: creds ? JSON.parse(creds) : {} };
    }
    case 'workspaces': {
      const ws = readFileSafe(path.join(dir, 'workspaces.json'));
      return { workspaces: ws ? JSON.parse(ws) : {} };
    }
    case 'all': {
      return {
        conversations: JSON.parse(readFileSafe(path.join(dir, 'conversations.json')) || '{}'),
        aliases: JSON.parse(readFileSafe(path.join(dir, 'aliases.json')) || '[]'),
        templates: JSON.parse(readFileSafe(path.join(dir, 'templates.json')) || '[]'),
        config: JSON.parse(readFileSafe(path.join(dir, 'config.json')) || '{}'),
        profiles: JSON.parse(readFileSafe(path.join(dir, 'profiles.json')) || '{}'),
        credentials: JSON.parse(readFileSafe(path.join(dir, 'credentials.json')) || '{}'),
        workspaces: JSON.parse(readFileSafe(path.join(dir, 'workspaces.json')) || '{}'),
      };
    }
    default:
      return {};
  }
}

/**
 * Format data for export.
 */
function formatForExport(data: Record<string, unknown>, format: ExportFormat): string {
  switch (format) {
    case ExportFormat.JSON:
      return JSON.stringify(data, null, 2);

    case ExportFormat.YAML: {
      // Simple YAML serializer (no dependency needed)
      const lines: string[] = [];
      function toYaml(obj: unknown, indent: string = ''): void {
        if (obj === null || obj === undefined) {
          lines.push(`${indent}null`);
          return;
        }
        if (typeof obj === 'string') {
          lines.push(`${indent}${obj.includes('\n') ? `"${obj.replace(/"/g, '\\"')}"` : obj}`);
          return;
        }
        if (typeof obj === 'number' || typeof obj === 'boolean') {
          lines.push(`${indent}${obj}`);
          return;
        }
        if (Array.isArray(obj)) {
          if (obj.length === 0) { lines.push(`${indent}[]`); return; }
          for (const item of obj) {
            if (typeof item === 'object' && item !== null) {
              lines.push(`${indent}-`);
              toYaml(item, indent + '  ');
            } else {
              lines.push(`${indent}- ${item}`);
            }
          }
          return;
        }
        if (typeof obj === 'object') {
          const entries = Object.entries(obj as Record<string, unknown>);
          if (entries.length === 0) { lines.push(`${indent}{}`); return; }
          for (const [key, val] of entries) {
            if (typeof val === 'object' && val !== null) {
              lines.push(`${indent}${key}:`);
              toYaml(val, indent + '  ');
            } else {
              lines.push(`${indent}${key}: ${val}`);
            }
          }
          return;
        }
      }
      toYaml(data);
      return lines.join('\n');
    }

    case ExportFormat.CSV: {
      const rows: string[] = [];
      for (const [scope, value] of Object.entries(data)) {
        rows.push(`scope,${scope}`);
        rows.push(`data,${JSON.stringify(value)}`);
        rows.push('');
      }
      return rows.join('\n');
    }

    case ExportFormat.Markdown: {
      const lines: string[] = ['# Xergon Data Export\n'];
      lines.push(`Exported: ${new Date().toISOString()}\n`);
      for (const [scope, value] of Object.entries(data)) {
        lines.push(`## ${scope}`);
        if (typeof value === 'object' && value !== null) {
          lines.push('```json');
          lines.push(JSON.stringify(value, null, 2));
          lines.push('```');
        } else {
          lines.push(String(value));
        }
        lines.push('');
      }
      return lines.join('\n');
    }

    case ExportFormat.SQLite:
      throw new Error('SQLite export requires the better-sqlite3 package. Use JSON format instead.');

    default:
      throw new Error(`Unsupported export format: ${format}`);
  }
}

// ── Public API ──────────────────────────────────────────────────────

/**
 * List available export scopes with descriptions.
 */
export function listExportScopes(): ScopeInfo[] {
  return [...SCOPE_DEFINITIONS];
}

/**
 * Estimate the export size for given scopes.
 */
export function getExportSize(scopes: ExportScope[]): number {
  let totalBytes = 0;
  for (const scope of scopes) {
    const data = gatherScope(scope === 'all' ? 'all' : scope);
    const jsonStr = JSON.stringify(data);
    totalBytes += Buffer.byteLength(jsonStr, 'utf-8');
  }
  // Deduplicate if 'all' is included alongside individual scopes
  if (scopes.includes('all') && scopes.length > 1) {
    // Just return the 'all' size since it covers everything
    const data = gatherScope('all');
    return Buffer.byteLength(JSON.stringify(data), 'utf-8');
  }
  return totalBytes;
}

/**
 * Export specified data scopes to a file.
 */
export function exportData(config: ExportConfig): ExportResult {
  // Resolve scopes -- expand 'all'
  const allScopes: ExportScope[] = ['conversations', 'aliases', 'templates', 'config', 'credentials', 'workspaces'];
  const effectiveScopes = config.include.includes('all')
    ? allScopes
    : config.include;

  // Gather all data
  const allData: Record<string, unknown> = {};
  for (const scope of effectiveScopes) {
    const scopeData = gatherScope(scope);
    Object.assign(allData, scopeData);
  }

  // Format
  let output = formatForExport(allData, config.format);

  // Create manifest
  const manifest: ExportManifest = {
    version: '1.0.0',
    exportedAt: new Date().toISOString(),
    format: config.format,
    scopes: effectiveScopes,
    checksum: computeChecksum(output),
    encrypted: false,
    compressed: false,
  };

  // Encrypt if requested
  if (config.encrypt) {
    const passphrase = process.env.XERGON_EXPORT_KEY;
    if (!passphrase) {
      throw new Error('Encryption requires XERGON_EXPORT_KEY environment variable');
    }
    const key = crypto.scryptSync(passphrase, 'xergon-salt', 32);
    const iv = crypto.randomBytes(16);
    const cipher = crypto.createCipheriv('aes-256-cbc', key, iv);
    const encrypted = Buffer.concat([iv, cipher.update(output, 'utf-8'), cipher.final()]);
    output = encrypted.toString('base64');
    manifest.encrypted = true;
  }

  // Compress note (actual compression would require zlib, we mark it but skip
  // deep compression to avoid dependency; the JSON is already reasonably compact)
  if (config.compress) {
    const compressed = zlibDeflateSync(output);
    output = compressed.toString('base64');
    manifest.compressed = true;
    // Recompute checksum after compression
    manifest.checksum = computeChecksum(output);
  }

  // Write output
  const outPath = path.resolve(config.outputPath);
  const dir = path.dirname(outPath);
  if (!fs.existsSync(dir)) {
    fs.mkdirSync(dir, { recursive: true });
  }

  // Write as JSON envelope with manifest
  const envelope = JSON.stringify({ manifest, data: output }, null, 2);
  fs.writeFileSync(outPath, envelope, 'utf-8');

  const stat = fs.statSync(outPath);

  return {
    path: outPath,
    size: stat.size,
    manifest,
  };
}

/**
 * Minimal deflate (base64 is "compression enough" for JSON; real gzip
 * requires zlib import). We use a simple approach.
 */
function zlibDeflateSync(data: string): Buffer {
  try {
    return zlib.deflateSync(Buffer.from(data, 'utf-8'));
  } catch {
    return Buffer.from(data, 'utf-8');
  }
}

/**
 * Validate an export file's integrity.
 */
export function validateExport(filePath: string): { valid: boolean; manifest?: ExportManifest; error?: string } {
  try {
    const raw = fs.readFileSync(filePath, 'utf-8');
    const envelope = JSON.parse(raw);

    if (!envelope.manifest || !envelope.data) {
      return { valid: false, error: 'Invalid export file format: missing manifest or data' };
    }

    const manifest = envelope.manifest as ExportManifest;

    if (!manifest.version || !manifest.exportedAt) {
      return { valid: false, error: 'Invalid manifest: missing required fields' };
    }

    if (manifest.encrypted) {
      // Can't verify checksum of encrypted data without the key
      return { valid: true, manifest };
    }

    const actualChecksum = computeChecksum(envelope.data);
    if (manifest.checksum && actualChecksum !== manifest.checksum) {
      return { valid: false, manifest, error: `Checksum mismatch: expected ${manifest.checksum}, got ${actualChecksum}` };
    }

    return { valid: true, manifest };
  } catch (err) {
    return { valid: false, error: `Failed to read export file: ${err instanceof Error ? err.message : String(err)}` };
  }
}

/**
 * Import data from an exported file.
 */
export function importData(
  filePath: string,
  _format?: string, // format is auto-detected from manifest
): { scopes: ExportScope[]; imported: string[]; errors: string[] } {
  const validation = validateExport(filePath);
  if (!validation.valid) {
    throw new Error(`Cannot import invalid export file: ${validation.error}`);
  }

  const envelope = JSON.parse(fs.readFileSync(filePath, 'utf-8'));
  const manifest = envelope.manifest as ExportManifest;
  let data = envelope.data as string;

  // Decompress if needed
  if (manifest.compressed) {
    try {
      data = zlib.inflateSync(Buffer.from(data, 'base64')).toString('utf-8');
    } catch {
      throw new Error('Failed to decompress export data');
    }
  }

  // Decrypt if needed
  if (manifest.encrypted) {
    const passphrase = process.env.XERGON_EXPORT_KEY;
    if (!passphrase) {
      throw new Error('Decryption requires XERGON_EXPORT_KEY environment variable');
    }
    try {
      const key = crypto.scryptSync(passphrase, 'xergon-salt', 32);
      const encrypted = Buffer.from(data, 'base64');
      const iv = encrypted.subarray(0, 16);
      const ciphertext = encrypted.subarray(16);
      const decipher = crypto.createDecipheriv('aes-256-cbc', key, iv);
      data = decipher.update(ciphertext) + decipher.final('utf-8');
    } catch {
      throw new Error('Failed to decrypt export data. Check your XERGON_EXPORT_KEY.');
    }
  }

  // Parse the actual data (might be in the export format)
  let parsedData: Record<string, unknown>;
  try {
    parsedData = typeof data === 'string' ? JSON.parse(data) : data;
  } catch {
    throw new Error('Failed to parse export data');
  }

  // Write to local files
  const dir = XERGON_DIR();
  const imported: string[] = [];
  const errors: string[] = [];

  const scopeToFile: Record<string, string> = {
    conversations: 'conversations.json',
    aliases: 'aliases.json',
    templates: 'templates.json',
    config: 'config.json',
    credentials: 'credentials.json',
    workspaces: 'workspaces.json',
  };

  for (const scope of manifest.scopes) {
    const key = scope === 'all' ? null : scope;

    // For 'all', write each sub-scope separately
    if (scope === 'all') {
      for (const [subScope, fileName] of Object.entries(scopeToFile)) {
        if (parsedData[subScope] !== undefined) {
          try {
            writeFileSafe(path.join(dir, fileName), JSON.stringify(parsedData[subScope], null, 2));
            imported.push(subScope);
          } catch (err) {
            errors.push(`Failed to import ${subScope}: ${err instanceof Error ? err.message : String(err)}`);
          }
        }
      }
    } else if (parsedData[scope] !== undefined) {
      const fileName = scopeToFile[scope];
      if (fileName) {
        try {
          writeFileSafe(path.join(dir, fileName), JSON.stringify(parsedData[scope], null, 2));
          imported.push(scope);
        } catch (err) {
          errors.push(`Failed to import ${scope}: ${err instanceof Error ? err.message : String(err)}`);
        }
      }
    }
  }

  // Handle config/profiles separately
  if (parsedData.config) {
    try {
      writeFileSafe(path.join(dir, 'config.json'), JSON.stringify(parsedData.config, null, 2));
      if (!imported.includes('config')) imported.push('config');
    } catch (err) {
      errors.push(`Failed to import config: ${err instanceof Error ? err.message : String(err)}`);
    }
  }
  if (parsedData.profiles) {
    try {
      writeFileSafe(path.join(dir, 'profiles.json'), JSON.stringify(parsedData.profiles, null, 2));
      if (!imported.includes('profiles')) imported.push('config');
    } catch (err) {
      errors.push(`Failed to import profiles: ${err instanceof Error ? err.message : String(err)}`);
    }
  }

  return { scopes: manifest.scopes, imported, errors };
}
