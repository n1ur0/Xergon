/**
 * Xergon SDK -- Deploy API
 *
 * Deploy models as services on the Xergon Network.
 * Manage deployments, stream logs, and stop running services.
 */

import { XergonClientCore } from './client';

// ── Types ───────────────────────────────────────────────────────────

export interface DeployConfig {
  model: string;
  port?: number;
  gpu?: number;
  memory_limit?: string;
  env?: Record<string, string>;
}

export interface Deployment {
  id: string;
  model: string;
  status: 'starting' | 'running' | 'stopping' | 'stopped' | 'failed';
  url: string;
  port: number;
  gpu?: number;
  memory_limit?: string;
  env?: Record<string, string>;
  created_at: string;
  started_at?: string;
  stopped_at?: string;
  error?: string;
}

export interface DeploymentLog {
  id: string;
  deployment_id: string;
  timestamp: string;
  level: 'info' | 'warn' | 'error' | 'debug';
  message: string;
}

// ── API Functions ──────────────────────────────────────────────────

/**
 * Deploy a model as a service on the Xergon Network.
 */
export async function deploy(
  core: XergonClientCore,
  config: DeployConfig,
  options?: { signal?: AbortSignal },
): Promise<Deployment> {
  return core.post<Deployment>('/v1/deployments', config, options);
}

/**
 * List all running deployments for the authenticated user.
 */
export async function listDeployments(
  core: XergonClientCore,
  options?: { signal?: AbortSignal },
): Promise<Deployment[]> {
  return core.get<Deployment[]>('/v1/deployments', options);
}

/**
 * Get the status of a specific deployment.
 */
export async function getDeployment(
  core: XergonClientCore,
  id: string,
  options?: { signal?: AbortSignal },
): Promise<Deployment> {
  return core.get<Deployment>(`/v1/deployments/${id}`, options);
}

/**
 * Stop a running deployment.
 */
export async function stopDeployment(
  core: XergonClientCore,
  id: string,
  options?: { signal?: AbortSignal },
): Promise<Deployment> {
  return core.post<Deployment>(`/v1/deployments/${id}/stop`, {}, options);
}

/**
 * Get logs for a deployment. Returns recent log entries.
 */
export async function getDeploymentLogs(
  core: XergonClientCore,
  id: string,
  params?: { limit?: number; level?: string },
  options?: { signal?: AbortSignal },
): Promise<DeploymentLog[]> {
  const query = new URLSearchParams();
  if (params?.limit) query.set('limit', String(params.limit));
  if (params?.level) query.set('level', params.level);
  const qs = query.toString();
  const path = `/v1/deployments/${id}/logs${qs ? `?${qs}` : ''}`;
  return core.get<DeploymentLog[]>(path, options);
}
