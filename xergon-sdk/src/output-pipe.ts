/**
 * Xergon SDK -- Output Piping System.
 *
 * Provides functions for formatting output in multiple formats and
 * piping results to various destinations (stdout, file, clipboard,
 * external commands).
 *
 * @example
 * ```ts
 * import { pipeOutput, formatOutput } from '@xergon/sdk';
 *
 * // Format as markdown
 * const md = formatOutput(response, 'markdown');
 *
 * // Pipe to file
 * await pipeOutput(response, {
 *   format: 'markdown',
 *   destination: 'file',
 *   path: 'output.md',
 * });
 * ```
 */

import { execSync } from 'node:child_process';
import * as fs from 'node:fs';
import * as path from 'node:path';

// ── Types ───────────────────────────────────────────────────────────

export type OutputFormat = 'text' | 'json' | 'markdown' | 'csv';

export type PipeDestination = 'stdout' | 'file' | 'clipboard' | 'command';

export interface PipeConfig {
  format: OutputFormat;
  destination: PipeDestination;
  path?: string;       // file path for 'file' destination
  command?: string;    // shell command for 'command' destination
}

// ── Format Conversion ──────────────────────────────────────────────

/**
 * Convert content between output formats.
 *
 * - text -> json: wraps in a JSON object { content: string }
 * - text -> markdown: wraps in a markdown code block
 * - text -> csv: single-column CSV
 * - json -> text: pretty-prints JSON
 * - json -> markdown: formats as a markdown table or code block
 * - json -> csv: flattens to CSV rows
 */
export function formatOutput(content: string, format: OutputFormat): string {
  // Try to detect if content is JSON
  let isJson = false;
  let parsed: unknown = null;
  try {
    parsed = JSON.parse(content);
    isJson = true;
  } catch {
    // Not JSON -- treat as plain text
  }

  switch (format) {
    case 'json':
      if (isJson) {
        return JSON.stringify(parsed, null, 2);
      }
      return JSON.stringify({ content }, null, 2);

    case 'markdown':
      if (isJson) {
        return jsonToMarkdown(parsed);
      }
      return content;

    case 'csv':
      if (isJson) {
        return jsonToCsv(parsed);
      }
      // Single-column CSV for plain text (line by line)
      return 'content\n' + content.split('\n').map(line => `"${line.replace(/"/g, '""')}"`).join('\n');

    case 'text':
    default:
      if (isJson) {
        return JSON.stringify(parsed, null, 2);
      }
      return content;
  }
}

/**
 * Pipe output to a specified destination.
 */
export async function pipeOutput(content: string, config: PipeConfig): Promise<void> {
  const formatted = formatOutput(content, config.format);

  switch (config.destination) {
    case 'stdout':
      process.stdout.write(formatted);
      break;

    case 'file':
      if (!config.path) {
        throw new Error('File path is required for file destination. Use --pipe "file:<path>".');
      }
      appendToFile(config.path, formatted);
      break;

    case 'clipboard':
      await copyToClipboard(formatted);
      break;

    case 'command':
      if (!config.command) {
        throw new Error('Command is required for command destination. Use --pipe "command:<cmd>".');
      }
      pipeToCommand(config.command, formatted);
      break;
  }
}

/**
 * Copy text to the system clipboard.
 */
export async function copyToClipboard(text: string): Promise<void> {
  // Try pbcopy (macOS), xclip (Linux), or clip (Windows)
  const cmd = process.platform === 'darwin'
    ? 'pbcopy'
    : process.platform === 'win32'
      ? 'clip'
      : 'xclip -selection clipboard';

  try {
    execSync(cmd, { input: text, stdio: ['pipe', 'ignore', 'pipe'] });
  } catch {
    // Fallback: no clipboard tool available
    throw new Error(
      'No clipboard tool found. Install pbcopy (macOS), xclip (Linux), or use another pipe destination.',
    );
  }
}

/**
 * Append content to a file (creates if not exists).
 */
export function appendToFile(filePath: string, content: string): void {
  const dir = path.dirname(filePath);
  if (!fs.existsSync(dir)) {
    fs.mkdirSync(dir, { recursive: true });
  }

  fs.appendFileSync(filePath, content + '\n');
}

/**
 * Pipe content to an external command via stdin.
 */
export function pipeToCommand(command: string, content: string): void {
  try {
    const result = execSync(command, { input: content, stdio: ['pipe', 'pipe', 'pipe'] });
    if (result.length > 0) {
      process.stdout.write(result.toString());
    }
  } catch (err: any) {
    // Command failed -- write stderr
    if (err.stderr?.length) {
      process.stderr.write(err.stderr.toString());
    }
    throw new Error(`Command failed: ${command} (exit code ${err.status ?? 1})`);
  }
}

/**
 * Parse a pipe string like "file:output.md", "clipboard", "command:grep error"
 * into a PipeConfig.
 */
export function parsePipeString(pipeStr: string, defaultFormat: OutputFormat = 'text'): PipeConfig {
  if (pipeStr === 'clipboard' || pipeStr === 'clip') {
    return { format: defaultFormat, destination: 'clipboard' };
  }

  const colonIdx = pipeStr.indexOf(':');
  if (colonIdx === -1) {
    // No colon -- treat as file path
    return { format: defaultFormat, destination: 'file', path: pipeStr };
  }

  const rawDest = pipeStr.substring(0, colonIdx).toLowerCase();
  const value = pipeStr.substring(colonIdx + 1);

  // Normalize shorthand destination names
  let dest: PipeDestination;
  switch (rawDest) {
    case 'file':
    case 'f':
      dest = 'file';
      return { format: defaultFormat, destination: dest, path: value };
    case 'command':
    case 'cmd':
    case 'c':
      dest = 'command';
      return { format: defaultFormat, destination: dest, command: value };
    case 'clipboard':
    case 'clip':
      dest = 'clipboard';
      return { format: defaultFormat, destination: dest };
    default:
      // Unknown prefix -- treat entire string as file path
      return { format: defaultFormat, destination: 'file', path: pipeStr };
  }
}

// ── Conversion Helpers ─────────────────────────────────────────────

function jsonToMarkdown(data: unknown): string {
  if (Array.isArray(data) && data.length > 0 && typeof data[0] === 'object' && data[0] !== null) {
    // Array of objects -> markdown table
    const headers = Object.keys(data[0]);
    const lines: string[] = [];

    // Header row
    lines.push('| ' + headers.join(' | ') + ' |');
    lines.push('| ' + headers.map(() => '---').join(' | ') + ' |');

    // Data rows
    for (const row of data) {
      const cells = headers.map(h => {
        const val = (row as Record<string, unknown>)[h];
        return String(val ?? '').replace(/\|/g, '\\|').replace(/\n/g, '<br>');
      });
      lines.push('| ' + cells.join(' | ') + ' |');
    }

    return lines.join('\n');
  }

  if (typeof data === 'object' && data !== null && !Array.isArray(data)) {
    // Object -> markdown key-value pairs
    const lines: string[] = [];
    for (const [key, value] of Object.entries(data as Record<string, unknown>)) {
      if (typeof value === 'string') {
        lines.push(`**${key}:** ${value}`);
      } else if (Array.isArray(value)) {
        lines.push(`**${key}:**\n${value.map(v => `- ${JSON.stringify(v)}`).join('\n')}`);
      } else {
        lines.push(`**${key}:** \`${JSON.stringify(value)}\``);
      }
    }
    return lines.join('\n');
  }

  // Fallback: code block
  return '```json\n' + JSON.stringify(data, null, 2) + '\n```';
}

function jsonToCsv(data: unknown): string {
  if (Array.isArray(data) && data.length > 0 && typeof data[0] === 'object' && data[0] !== null) {
    const headers = Object.keys(data[0]);
    const lines: string[] = [];

    // Header row
    lines.push(headers.map(csvEscape).join(','));

    // Data rows
    for (const row of data) {
      const cells = headers.map(h => csvEscape(String((row as Record<string, unknown>)[h] ?? '')));
      lines.push(cells.join(','));
    }

    return lines.join('\n');
  }

  // Fallback: single value
  return csvEscape(JSON.stringify(data));
}

function csvEscape(value: string): string {
  if (value.includes(',') || value.includes('"') || value.includes('\n')) {
    return `"${value.replace(/"/g, '""')}"`;
  }
  return value;
}
