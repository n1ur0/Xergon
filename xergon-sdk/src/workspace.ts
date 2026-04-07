/**
 * Workspace Management -- named workspace contexts for the Xergon SDK/CLI.
 *
 * Workspaces group together environment variables, default model/provider,
 * and a working path. Stored in ~/.xergon/workspaces.json.
 *
 * @example
 * ```ts
 * import { createWorkspace, switchWorkspace, listWorkspaces } from '@xergon/sdk';
 *
 * await createWorkspace('production', '/app');
 * await switchWorkspace('production');
 * const workspaces = listWorkspaces();
 * ```
 */

import * as fs from 'node:fs';
import * as path from 'node:path';
import * as os from 'node:os';

// ── Types ───────────────────────────────────────────────────────────

export interface Workspace {
  /** Unique workspace name. */
  name: string;
  /** Filesystem path associated with this workspace. */
  path: string;
  /** Default model for this workspace. */
  defaultModel?: string;
  /** Default provider for this workspace. */
  defaultProvider?: string;
  /** Environment variables scoped to this workspace. */
  environment: Record<string, string>;
  /** ISO timestamp of creation. */
  createdAt: string;
}

export interface WorkspaceConfig {
  /** Name of the currently active workspace. */
  current: string;
  /** Map of workspace name to Workspace. */
  workspaces: Record<string, Workspace>;
}

// ── Paths ───────────────────────────────────────────────────────────

function getConfigDir(): string {
  return path.join(os.homedir(), '.xergon');
}

function getWorkspacesPath(): string {
  return path.join(getConfigDir(), 'workspaces.json');
}

// ── Persistence ─────────────────────────────────────────────────────

function loadWorkspaceConfig(): WorkspaceConfig {
  try {
    const data = fs.readFileSync(getWorkspacesPath(), 'utf-8');
    return JSON.parse(data) as WorkspaceConfig;
  } catch {
    return {
      current: 'default',
      workspaces: {},
    };
  }
}

function saveWorkspaceConfig(config: WorkspaceConfig): void {
  const dir = getConfigDir();
  if (!fs.existsSync(dir)) {
    fs.mkdirSync(dir, { recursive: true });
  }
  fs.writeFileSync(getWorkspacesPath(), JSON.stringify(config, null, 2) + '\n');
}

// ── Public API ──────────────────────────────────────────────────────

/**
 * Create a new workspace.
 * If a workspace with the same name exists, throws an error.
 */
export function createWorkspace(name: string, workspacePath?: string): Workspace {
  const config = loadWorkspaceConfig();

  if (config.workspaces[name]) {
    throw new Error(`Workspace "${name}" already exists.`);
  }

  const workspace: Workspace = {
    name,
    path: workspacePath ?? process.cwd(),
    environment: {},
    createdAt: new Date().toISOString(),
  };

  config.workspaces[name] = workspace;

  // If this is the first workspace, set it as current
  if (Object.keys(config.workspaces).length === 1) {
    config.current = name;
  }

  saveWorkspaceConfig(config);
  return workspace;
}

/**
 * Switch the active workspace.
 * Throws if the workspace does not exist.
 */
export function switchWorkspace(name: string): Workspace {
  const config = loadWorkspaceConfig();

  if (!config.workspaces[name]) {
    throw new Error(`Workspace "${name}" does not exist.`);
  }

  config.current = name;
  saveWorkspaceConfig(config);
  return config.workspaces[name];
}

/**
 * List all workspaces.
 * Returns an array of Workspace objects with an `active` flag added.
 */
export function listWorkspaces(): Array<Workspace & { active: boolean }> {
  const config = loadWorkspaceConfig();
  return Object.values(config.workspaces).map(ws => ({
    ...ws,
    active: ws.name === config.current,
  }));
}

/**
 * Delete a workspace by name.
 * Cannot delete the last remaining workspace.
 * If the deleted workspace was active, switches to the first available.
 */
export function deleteWorkspace(name: string): void {
  const config = loadWorkspaceConfig();

  if (!config.workspaces[name]) {
    throw new Error(`Workspace "${name}" does not exist.`);
  }

  const remaining = Object.keys(config.workspaces).filter(k => k !== name);
  if (remaining.length === 0) {
    throw new Error('Cannot delete the last workspace.');
  }

  delete config.workspaces[name];

  if (config.current === name) {
    config.current = remaining[0];
  }

  saveWorkspaceConfig(config);
}

/**
 * Set an environment variable in a workspace.
 * If the workspace doesn't exist, throws an error.
 */
export function setWorkspaceVar(name: string, key: string, value: string): void {
  const config = loadWorkspaceConfig();

  if (!config.workspaces[name]) {
    throw new Error(`Workspace "${name}" does not exist.`);
  }

  config.workspaces[name].environment[key] = value;
  saveWorkspaceConfig(config);
}

/**
 * Get an environment variable from a workspace.
 * Returns undefined if the variable or workspace doesn't exist.
 */
export function getWorkspaceVar(name: string, key: string): string | undefined {
  const config = loadWorkspaceConfig();
  return config.workspaces[name]?.environment[key];
}

/**
 * Get the currently active workspace.
 * Returns null if no workspace is active.
 */
export function getCurrentWorkspace(): Workspace | null {
  const config = loadWorkspaceConfig();
  return config.workspaces[config.current] ?? null;
}

/**
 * Get a workspace by name.
 * Returns null if not found.
 */
export function getWorkspace(name: string): Workspace | null {
  const config = loadWorkspaceConfig();
  return config.workspaces[name] ?? null;
}
