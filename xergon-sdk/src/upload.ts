/**
 * Upload -- file upload management for the Xergon relay.
 *
 * Provides methods for uploading, listing, retrieving, deleting,
 * and downloading files via the relay's OpenAI-compatible files endpoint.
 *
 * @example
 * ```ts
 * import { XergonClient } from '@xergon/sdk';
 *
 * const client = new XergonClient({ baseUrl: 'https://relay.xergon.gg' });
 *
 * // Upload a file
 * const file = await client.files.upload({
 *   file: './training-data.jsonl',
 *   purpose: 'fine-tune',
 * });
 * console.log(file.id);
 *
 * // List files
 * const files = await client.files.list();
 *
 * // Download a file
 * const buffer = await client.files.download(file.id);
 * ```
 */

import * as fs from 'node:fs';
import * as path from 'node:path';
import { XergonClientCore } from './client';

// ── Types ───────────────────────────────────────────────────────────

export interface UploadRequest {
  /** File path (string) or Buffer to upload. */
  file: string | Buffer;
  /** Intended purpose of the uploaded file. */
  purpose: 'fine-tune' | 'assistants' | 'batch';
}

export interface FileObject {
  /** Unique file identifier. */
  id: string;
  /** Object type, always 'file'. */
  object: 'file';
  /** Size of the file in bytes. */
  bytes: number;
  /** Unix timestamp of when the file was created. */
  created_at: number;
  /** Original filename. */
  filename: string;
  /** Purpose the file was uploaded for. */
  purpose: string;
  /** Processing status: 'uploaded', 'processed', 'error', etc. */
  status: string;
}

// ── Client Methods ────────────────────────────────────────────────

/**
 * Upload a file to the relay.
 * POST /v1/files (multipart/form-data)
 */
export async function uploadFile(
  client: XergonClientCore,
  request: UploadRequest,
  options?: { signal?: AbortSignal },
): Promise<FileObject> {
  const url = `${client.getBaseUrl()}/v1/files`;
  const formData = new FormData();
  formData.append('purpose', request.purpose);

  const fileSource = request.file;
  if (typeof fileSource === 'string') {
    // File path
    const buffer = fs.readFileSync(fileSource);
    const filename = path.basename(fileSource);
    formData.append('file', new Blob([buffer.buffer.slice(buffer.byteOffset, buffer.byteOffset + buffer.byteLength) as ArrayBuffer]), filename);
  } else if (Buffer.isBuffer(fileSource)) {
    formData.append('file', new Blob([fileSource.buffer.slice(fileSource.byteOffset, fileSource.byteOffset + fileSource.byteLength) as ArrayBuffer]), 'file');
  } else {
    // Web File object
    const webFile = fileSource as File;
    formData.append('file', webFile, webFile.name);
  }

  const res = await fetch(url, {
    method: 'POST',
    headers: {
      ...(client.getPublicKey() ? { 'X-Xergon-Public-Key': client.getPublicKey()! } : {}),
    },
    body: formData,
    signal: options?.signal,
  });

  if (!res.ok) {
    let errorData: unknown;
    try {
      errorData = await res.json();
    } catch {
      errorData = { message: res.statusText };
    }
    const { XergonError } = await import('./errors');
    throw XergonError.fromResponse(errorData);
  }

  return await res.json() as FileObject;
}

/**
 * List all uploaded files.
 * GET /v1/files
 */
export async function listFiles(
  client: XergonClientCore,
  options?: { signal?: AbortSignal },
): Promise<FileObject[]> {
  const res = await client.get<{ data: FileObject[] }>(
    '/v1/files',
    { signal: options?.signal },
  );
  return res.data;
}

/**
 * Get metadata for a specific uploaded file.
 * GET /v1/files/:fileId
 */
export async function getFile(
  client: XergonClientCore,
  fileId: string,
  options?: { signal?: AbortSignal },
): Promise<FileObject> {
  return client.get<FileObject>(
    `/v1/files/${fileId}`,
    { signal: options?.signal },
  );
}

/**
 * Delete an uploaded file.
 * DELETE /v1/files/:fileId
 */
export async function deleteFile(
  client: XergonClientCore,
  fileId: string,
  options?: { signal?: AbortSignal },
): Promise<void> {
  await client.request<void>(
    'DELETE',
    `/v1/files/${fileId}`,
    undefined,
    { signal: options?.signal },
  );
}

/**
 * Download the contents of an uploaded file.
 * GET /v1/files/:fileId/content
 */
export async function downloadFile(
  client: XergonClientCore,
  fileId: string,
  options?: { signal?: AbortSignal },
): Promise<Buffer> {
  const url = `${client.getBaseUrl()}/v1/files/${fileId}/content`;

  const res = await fetch(url, {
    method: 'GET',
    headers: {
      ...(client.getPublicKey() ? { 'X-Xergon-Public-Key': client.getPublicKey()! } : {}),
    },
    signal: options?.signal,
  });

  if (!res.ok) {
    let errorData: unknown;
    try {
      errorData = await res.json();
    } catch {
      errorData = { message: res.statusText };
    }
    const { XergonError } = await import('./errors');
    throw XergonError.fromResponse(errorData);
  }

  const arrayBuffer = await res.arrayBuffer();
  return Buffer.from(arrayBuffer);
}
