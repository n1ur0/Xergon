/**
 * Xergon SDK -- Browser-safe Output Piping System.
 *
 * Browser-compatible version that excludes Node.js-only modules.
 * Provides limited output piping functionality suitable for browser environments.
 */

// ── Types ───────────────────────────────────────────────────────────

export type OutputFormat = 'text' | 'json' | 'markdown' | 'csv';

export type PipeDestination = 'stdout' | 'file' | 'clipboard';

export interface PipeConfig {
  format: OutputFormat;
  destination: PipeDestination;
  path?: string;       // file path for 'file' destination
}

// ─ Format Conversion ──────────────────────────────────────────────

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
 * Pipe output to a specified destination (browser-safe).
 * Note: File writing and command execution are not available in browsers.
 */
export async function pipeOutput(content: string, config: PipeConfig): Promise<void> {
  const formatted = formatOutput(content, config.format);

  switch (config.destination) {
    case 'stdout':
      console.log(formatted);
      break;

    case 'file':
      // Browser limitation: Cannot write to filesystem directly
      // Offer download instead
      if (config.path) {
        downloadFile(config.path, formatted);
      } else {
        console.warn('File path required for file destination in browser');
      }
      break;

    case 'clipboard':
      await copyToClipboard(formatted);
      break;
  }
}

/**
 * Copy text to the browser clipboard using Clipboard API.
 */
export async function copyToClipboard(text: string): Promise<void> {
  if (navigator.clipboard && navigator.clipboard.writeText) {
    try {
      await navigator.clipboard.writeText(text);
    } catch {
      throw new Error('Failed to write to clipboard. User may have denied permission.');
    }
  } else {
    throw new Error('Clipboard API not available. Use HTTPS and modern browser.');
  }
}

/**
 * Download content as a file (browser-safe).
 */
export function downloadFile(filename: string, content: string): void {
  const blob = new Blob([content], { type: 'text/plain' });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  document.body.removeChild(a);
  URL.revokeObjectURL(url);
}

/**
 * Append content to a file - NOT AVAILABLE in browser.
 * Use downloadFile() instead.
 */
export function appendToFile(filePath: string, content: string): void {
  throw new Error('File system access not available in browser. Use downloadFile() instead.');
}

/**
 * Pipe to external command - NOT AVAILABLE in browser.
 */
export function pipeToCommand(command: string, content: string): void {
  throw new Error('Command execution not available in browser environment.');
}

/**
 * Parse a pipe string like "file:output.md", "clipboard" into a PipeConfig.
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
      throw new Error('Command execution not available in browser environment.');
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
