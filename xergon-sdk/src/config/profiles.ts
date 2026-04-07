/**
 * Config Profiles -- named configuration profiles for the Xergon SDK/CLI.
 *
 * Profiles allow switching between environments (dev, staging, production)
 * with a single command. Profiles are stored in ~/.xergon/profiles.json.
 *
 * @example
 * ```ts
 * import { useProfile, getCurrentProfile, setProfile } from '@xergon/sdk/config/profiles';
 *
 * // Switch to staging
 * await useProfile('staging');
 *
 * // Get current active profile (with fallback to default)
 * const profile = getCurrentProfile();
 * console.log(profile.baseUrl); // https://staging.xergon.gg
 *
 * // Create a custom profile
 * await setProfile('my-custom', { baseUrl: 'http://localhost:3000' });
 * ```
 */

import * as fs from 'node:fs';
import * as path from 'node:path';
import * as os from 'node:os';

// ── Types ───────────────────────────────────────────────────────────

export interface ProfileConfig {
  /** Relay base URL. */
  baseUrl: string;
  /** API key (public key) for authentication. */
  apiKey?: string;
  /** Public key for HMAC auth. */
  publicKey?: string;
  /** Default model to use. */
  defaultModel?: string;
  /** Request timeout in milliseconds. */
  timeout?: number;
}

export interface ProfilesData {
  /** Map of profile name to config. */
  profiles: Record<string, ProfileConfig>;
  /** Name of the currently active profile. */
  activeProfile: string | null;
}

// ── Paths ───────────────────────────────────────────────────────────

function getConfigDir(): string {
  return path.join(os.homedir(), '.xergon');
}

function getProfilesPath(): string {
  return path.join(getConfigDir(), 'profiles.json');
}

// ── Defaults ────────────────────────────────────────────────────────

const DEFAULT_PROFILES: Record<string, ProfileConfig> = {
  default: {
    baseUrl: 'https://relay.xergon.gg',
    defaultModel: 'llama-3.3-70b',
    timeout: 30000,
  },
  dev: {
    baseUrl: 'http://localhost:3000',
    defaultModel: 'llama-3.3-70b',
    timeout: 60000,
  },
  staging: {
    baseUrl: 'https://staging.xergon.gg',
    defaultModel: 'llama-3.3-70b',
    timeout: 30000,
  },
  production: {
    baseUrl: 'https://relay.xergon.gg',
    defaultModel: 'llama-3.3-70b',
    timeout: 30000,
  },
};

// ── Persistence ─────────────────────────────────────────────────────

/**
 * Load profiles from disk. If the file doesn't exist, returns defaults.
 */
function loadProfilesData(): ProfilesData {
  try {
    const data = fs.readFileSync(getProfilesPath(), 'utf-8');
    return JSON.parse(data) as ProfilesData;
  } catch {
    // No profiles file -- return defaults
    return {
      profiles: { ...DEFAULT_PROFILES },
      activeProfile: 'default',
    };
  }
}

/**
 * Save profiles data to disk.
 */
function saveProfilesData(data: ProfilesData): void {
  const dir = getConfigDir();
  if (!fs.existsSync(dir)) {
    fs.mkdirSync(dir, { recursive: true });
  }
  fs.writeFileSync(getProfilesPath(), JSON.stringify(data, null, 2) + '\n');
}

// ── Public API ──────────────────────────────────────────────────────

/**
 * List all profiles.
 * Returns an array of { name, config } objects.
 */
export function listProfiles(): Array<{ name: string; config: ProfileConfig; active: boolean }> {
  const data = loadProfilesData();
  const active = data.activeProfile ?? 'default';

  // Ensure all default profiles exist
  for (const [name, config] of Object.entries(DEFAULT_PROFILES)) {
    if (!data.profiles[name]) {
      data.profiles[name] = { ...config };
    }
  }

  return Object.entries(data.profiles).map(([name, config]) => ({
    name,
    config,
    active: name === active,
  }));
}

/**
 * Get a specific profile by name.
 * Falls back to default profile if name not found.
 */
export function getProfile(name: string): ProfileConfig & { name: string } {
  const data = loadProfilesData();

  // Ensure all default profiles exist
  for (const [defName, defConfig] of Object.entries(DEFAULT_PROFILES)) {
    if (!data.profiles[defName]) {
      data.profiles[defName] = { ...defConfig };
    }
  }

  const config = data.profiles[name] ?? data.profiles['default'] ?? DEFAULT_PROFILES.default;
  return { name, ...config };
}

/**
 * Save or update a profile.
 * Merges provided config with existing profile config.
 */
export function setProfile(name: string, config: Partial<ProfileConfig>): ProfileConfig {
  const data = loadProfilesData();

  // Initialize from default if this is a new profile
  if (!data.profiles[name]) {
    data.profiles[name] = {
      ...DEFAULT_PROFILES.default,
      ...config,
    };
  } else {
    data.profiles[name] = {
      ...data.profiles[name],
      ...config,
    };
  }

  saveProfilesData(data);
  return data.profiles[name];
}

/**
 * Set the active profile by name.
 * Returns the active profile config.
 */
export function useProfile(name: string): ProfileConfig & { name: string } {
  const data = loadProfilesData();

  // Ensure the target profile exists
  if (!data.profiles[name]) {
    // Create it from defaults if it's a known name, otherwise error
    if (DEFAULT_PROFILES[name]) {
      data.profiles[name] = { ...DEFAULT_PROFILES[name] };
    } else {
      throw new Error(`Profile "${name}" does not exist. Create it first with setProfile().`);
    }
  }

  data.activeProfile = name;
  saveProfilesData(data);

  return { name, ...data.profiles[name] };
}

/**
 * Get the currently active profile.
 * Falls back to 'default' if no active profile is set.
 */
export function getCurrentProfile(): ProfileConfig & { name: string } {
  const data = loadProfilesData();
  const activeName = data.activeProfile ?? 'default';

  // Ensure all default profiles exist
  for (const [defName, defConfig] of Object.entries(DEFAULT_PROFILES)) {
    if (!data.profiles[defName]) {
      data.profiles[defName] = { ...defConfig };
    }
  }

  const config = data.profiles[activeName] ?? data.profiles['default'] ?? DEFAULT_PROFILES.default;
  return { name: activeName, ...config };
}

/**
 * Delete a profile by name.
 * Cannot delete 'default', 'dev', 'staging', or 'production' built-in profiles.
 * If the deleted profile was active, switches to 'default'.
 */
export function deleteProfile(name: string): void {
  if (['default', 'dev', 'staging', 'production'].includes(name)) {
    throw new Error(`Cannot delete built-in profile "${name}".`);
  }

  const data = loadProfilesData();
  if (!data.profiles[name]) {
    throw new Error(`Profile "${name}" does not exist.`);
  }

  delete data.profiles[name];

  if (data.activeProfile === name) {
    data.activeProfile = 'default';
  }

  saveProfilesData(data);
}

/**
 * Merge the active profile config on top of a base config object.
 * This is intended to be used by the CLI and SDK initialization to
 * apply profile overrides.
 *
 * Profile values take precedence over base config values, but explicit
 * CLI args / env vars (already in the base config) should take precedence.
 * This function applies profile only for values not already set.
 */
export function applyProfileOverrides(
  baseConfig: Record<string, unknown>,
): Record<string, unknown> & { _profileName: string } {
  const profile = getCurrentProfile();
  const merged: Record<string, unknown> = { ...profile };

  // Remove profile name from the config object
  delete (merged as Record<string, unknown>).name;

  // Base config (env vars, CLI flags) takes precedence over profile
  for (const [key, value] of Object.entries(baseConfig)) {
    if (value !== undefined && value !== null && value !== '') {
      merged[key] = value;
    }
  }

  return { ...merged, _profileName: profile.name };
}
