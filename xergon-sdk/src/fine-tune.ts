/**
 * Xergon SDK -- Fine-Tuning API
 *
 * Create, monitor, cancel, and export fine-tuning jobs on the Xergon Network.
 * Supports LoRA, QLoRA, and full fine-tuning methods.
 */

import { XergonClientCore } from './client';

// ── Types ───────────────────────────────────────────────────────────

export interface FineTuneCreateRequest {
  model: string;
  dataset: string; // file path or URL
  method: 'lora' | 'qlora' | 'full';
  epochs?: number;
  learning_rate?: number;
  batch_size?: number;
  lora_r?: number;
  lora_alpha?: number;
  output_name?: string;
}

export interface FineTuneJob {
  id: string;
  model: string;
  status: 'queued' | 'running' | 'completed' | 'failed' | 'cancelled';
  progress: number;
  epoch: number;
  total_epochs: number;
  loss: number;
  created_at: string;
  error?: string;
  method?: string;
  dataset?: string;
  output_name?: string;
}

export interface FineTuneExportResult {
  job_id: string;
  adapter_path: string;
  size_bytes: number;
  format: string;
  exported_at: string;
}

// ── API Functions ──────────────────────────────────────────────────

/**
 * Create a new fine-tuning job.
 */
export async function createFineTuneJob(
  core: XergonClientCore,
  request: FineTuneCreateRequest,
  options?: { signal?: AbortSignal },
): Promise<FineTuneJob> {
  return core.post<FineTuneJob>('/v1/fine-tune/jobs', request, options);
}

/**
 * List all fine-tuning jobs for the authenticated user.
 */
export async function listFineTuneJobs(
  core: XergonClientCore,
  options?: { signal?: AbortSignal },
): Promise<FineTuneJob[]> {
  return core.get<FineTuneJob[]>('/v1/fine-tune/jobs', options);
}

/**
 * Get the status of a specific fine-tuning job.
 */
export async function getFineTuneJob(
  core: XergonClientCore,
  id: string,
  options?: { signal?: AbortSignal },
): Promise<FineTuneJob> {
  return core.get<FineTuneJob>(`/v1/fine-tune/jobs/${id}`, options);
}

/**
 * Cancel a running or queued fine-tuning job.
 */
export async function cancelFineTuneJob(
  core: XergonClientCore,
  id: string,
  options?: { signal?: AbortSignal },
): Promise<FineTuneJob> {
  return core.post<FineTuneJob>(`/v1/fine-tune/jobs/${id}/cancel`, {}, options);
}

/**
 * Export a completed fine-tuning job as a downloadable adapter.
 */
export async function exportFineTuneJob(
  core: XergonClientCore,
  id: string,
  options?: { signal?: AbortSignal },
): Promise<FineTuneExportResult> {
  return core.get<FineTuneExportResult>(`/v1/fine-tune/jobs/${id}/export`, options);
}
