/**
 * CLI command: upload
 *
 * File upload management for the Xergon relay.
 *
 * Usage:
 *   xergon upload <file> --purpose fine-tune
 *   xergon upload list
 *   xergon upload delete <file_id>
 *   xergon upload download <file_id> --output <path>
 */

import type { Command, CommandOption, ParsedArgs, CLIContext } from '../mod';
import type { FileObject } from '../../upload';
import * as fs from 'node:fs';

async function uploadAction(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const sub = args.positional[0];

  if (!sub) {
    ctx.output.writeError('Usage: xergon upload <file|list|delete|download> [options]');
    process.exit(1);
    return;
  }

  switch (sub) {
    case 'list':
      await handleList(args, ctx);
      break;
    case 'delete':
      await handleDelete(args, ctx);
      break;
    case 'download':
      await handleDownload(args, ctx);
      break;
    default:
      // Treat as file path to upload
      await handleUpload(sub, args, ctx);
      break;
  }
}

// ── upload file ────────────────────────────────────────────────────

async function handleUpload(filePath: string, args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const purpose = (String(args.options.purpose || 'assistants')) as 'fine-tune' | 'assistants' | 'batch';

  if (!fs.existsSync(filePath)) {
    ctx.output.writeError(`File not found: ${filePath}`);
    process.exit(1);
    return;
  }

  const thinkingMsg = ctx.output.colorize('Uploading file', 'cyan');
  process.stderr.write(`${thinkingMsg}...\r`);

  try {
    const { uploadFile } = await import('../../upload');
    const fileObj = await uploadFile(ctx.client._core || ctx.client.core, {
      file: filePath,
      purpose,
    });

    process.stderr.write(' '.repeat(40) + '\r');

    ctx.output.success(`File uploaded successfully`);
    ctx.output.write('');
    const tableData = [{
      ID: fileObj.id,
      Filename: fileObj.filename,
      Bytes: String(fileObj.bytes),
      Purpose: fileObj.purpose,
      Status: fileObj.status,
    }];
    ctx.output.write(ctx.output.formatTable(tableData, 'Uploaded File'));
  } catch (err) {
    process.stderr.write(' '.repeat(40) + '\r');
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Upload failed: ${message}`);
    process.exit(1);
  }
}

// ── list files ─────────────────────────────────────────────────────

async function handleList(_args: ParsedArgs, ctx: CLIContext): Promise<void> {
  try {
    const { listFiles } = await import('../../upload');
    const files = await listFiles(ctx.client._core || ctx.client.core);

    if (files.length === 0) {
      ctx.output.info('No uploaded files found.');
      return;
    }

    const tableData = files.map((f: FileObject) => ({
      ID: f.id,
      Filename: f.filename,
      Bytes: String(f.bytes),
      Purpose: f.purpose,
      Status: f.status,
      Created: new Date(f.created_at * 1000).toISOString().slice(0, 10),
    }));
    ctx.output.write(ctx.output.formatTable(tableData, `Uploaded Files (${files.length})`));
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to list files: ${message}`);
    process.exit(1);
  }
}

// ── delete file ────────────────────────────────────────────────────

async function handleDelete(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const fileId = args.positional[1];

  if (!fileId) {
    ctx.output.writeError('No file ID specified. Use: xergon upload delete <file_id>');
    process.exit(1);
    return;
  }

  try {
    const { deleteFile } = await import('../../upload');
    await deleteFile(ctx.client._core || ctx.client.core, fileId);
    ctx.output.success(`File ${fileId} deleted successfully`);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to delete file: ${message}`);
    process.exit(1);
  }
}

// ── download file ──────────────────────────────────────────────────

async function handleDownload(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const fileId = args.positional[1];
  const outputPath = args.options.output ? String(args.options.output) : undefined;

  if (!fileId) {
    ctx.output.writeError('No file ID specified. Use: xergon upload download <file_id> --output <path>');
    process.exit(1);
    return;
  }

  if (!outputPath) {
    ctx.output.writeError('No output path specified. Use: --output <path>');
    process.exit(1);
    return;
  }

  const thinkingMsg = ctx.output.colorize('Downloading file', 'cyan');
  process.stderr.write(`${thinkingMsg}...\r`);

  try {
    const { downloadFile } = await import('../../upload');
    const buffer = await downloadFile(ctx.client._core || ctx.client.core, fileId);

    process.stderr.write(' '.repeat(40) + '\r');

    fs.writeFileSync(outputPath, buffer);
    ctx.output.success(`File downloaded to ${outputPath} (${buffer.length} bytes)`);
  } catch (err) {
    process.stderr.write(' '.repeat(40) + '\r');
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Download failed: ${message}`);
    process.exit(1);
  }
}

export const uploadCommand: Command = {
  name: 'upload',
  description: 'Upload, list, delete, and download files',
  aliases: ['files'],
  options: [
    {
      name: 'purpose',
      short: '-p',
      long: '--purpose',
      description: 'File purpose: fine-tune, assistants, or batch (default: assistants)',
      required: false,
      type: 'string',
    },
    {
      name: 'output',
      short: '-o',
      long: '--output',
      description: 'Output file path (for download)',
      required: false,
      type: 'string',
    },
  ],
  action: uploadAction,
};
