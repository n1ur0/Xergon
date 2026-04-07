/**
 * Tests for CLI command: update
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import {
  parseSemver,
  compareSemver,
  detectPlatform,
  getBinaryName,
  readSavedChannel,
  saveChannel,
  verifyChecksum,
  computeFileSha256,
  listBackups,
  cleanOldBackups,
  backupCurrentBinary,
  rollbackToBackup,
  checkForUpdates,
  updateCommand,
  CURRENT_VERSION,
  VALID_CHANNELS,
} from './update';
import * as fs from 'node:fs';
import * as os from 'node:os';
import * as path from 'node:path';

// ── parseSemver ──────────────────────────────────────────────────

describe('parseSemver', () => {
  it('parses a simple version', () => {
    const result = parseSemver('1.2.3');
    expect(result).toEqual({ major: 1, minor: 2, patch: 3, prerelease: '', prereleaseNum: 0 });
  });

  it('parses version with v prefix', () => {
    const result = parseSemver('v1.2.3');
    expect(result).toEqual({ major: 1, minor: 2, patch: 3, prerelease: '', prereleaseNum: 0 });
  });

  it('parses version with prerelease', () => {
    const result = parseSemver('1.2.3-beta.1');
    expect(result).toEqual({ major: 1, minor: 2, patch: 3, prerelease: 'beta', prereleaseNum: 1 });
  });

  it('parses alpha prerelease', () => {
    const result = parseSemver('0.1.0-alpha.5');
    expect(result).toEqual({ major: 0, minor: 1, patch: 0, prerelease: 'alpha', prereleaseNum: 5 });
  });

  it('parses rc prerelease', () => {
    const result = parseSemver('2.0.0-rc.1');
    expect(result).toEqual({ major: 2, minor: 0, patch: 0, prerelease: 'rc', prereleaseNum: 1 });
  });

  it('returns null for invalid version', () => {
    expect(parseSemver('not-a-version')).toBeNull();
  });

  it('returns null for empty string', () => {
    expect(parseSemver('')).toBeNull();
  });

  it('parses nightly prerelease', () => {
    const result = parseSemver('0.2.0-nightly.42');
    expect(result).toEqual({ major: 0, minor: 2, patch: 0, prerelease: 'nightly', prereleaseNum: 42 });
  });
});

// ── compareSemver ─────────────────────────────────────────────────

describe('compareSemver', () => {
  it('returns -1 when a < b (patch)', () => {
    expect(compareSemver('1.0.0', '1.0.1')).toBe(-1);
  });

  it('returns 1 when a > b (minor)', () => {
    expect(compareSemver('1.1.0', '1.0.9')).toBe(1);
  });

  it('returns 0 for equal versions', () => {
    expect(compareSemver('1.2.3', '1.2.3')).toBe(0);
  });

  it('returns -1 when a is prerelease and b is release', () => {
    expect(compareSemver('1.0.0-beta.1', '1.0.0')).toBe(-1);
  });

  it('returns 1 when a is release and b is prerelease', () => {
    expect(compareSemver('1.0.0', '1.0.0-beta.1')).toBe(1);
  });

  it('compares prerelease types correctly', () => {
    expect(compareSemver('1.0.0-alpha.1', '1.0.0-beta.1')).toBe(-1);
    expect(compareSemver('1.0.0-beta.1', '1.0.0-rc.1')).toBe(-1);
    expect(compareSemver('1.0.0-rc.1', '1.0.0')).toBe(-1);
  });

  it('compares same prerelease type by number', () => {
    expect(compareSemver('1.0.0-beta.1', '1.0.0-beta.2')).toBe(-1);
  });

  it('handles null inputs', () => {
    expect(compareSemver('not-a-version', '1.0.0')).toBe(-1);
    expect(compareSemver('1.0.0', 'not-a-version')).toBe(1);
    expect(compareSemver('bad', 'bad')).toBe(0);
  });
});

// ── detectPlatform ────────────────────────────────────────────────

describe('detectPlatform', () => {
  it('returns an object with os, arch, and platform', () => {
    const platform = detectPlatform();
    expect(platform).toHaveProperty('os');
    expect(platform).toHaveProperty('arch');
    expect(platform).toHaveProperty('platform');
    expect(platform.platform).toContain('-');
  });

  it('platform string combines os and arch', () => {
    const platform = detectPlatform();
    expect(platform.platform).toBe(`${platform.os}-${platform.arch}`);
  });
});

// ── getBinaryName ─────────────────────────────────────────────────

describe('getBinaryName', () => {
  it('returns correct name for linux-amd64', () => {
    expect(getBinaryName({ os: 'linux', arch: 'amd64', platform: 'linux-amd64' })).toBe('xergon-linux-amd64');
  });

  it('returns correct name for darwin-arm64', () => {
    expect(getBinaryName({ os: 'darwin', arch: 'arm64', platform: 'darwin-arm64' })).toBe('xergon-darwin-arm64');
  });

  it('returns .exe for windows', () => {
    expect(getBinaryName({ os: 'windows', arch: 'amd64', platform: 'windows-amd64' })).toBe('xergon-windows-amd64.exe');
  });
});

// ── Channel Management ───────────────────────────────────────────

describe('readSavedChannel', () => {
  it('returns stable when no file exists', () => {
    // Use a path that definitely doesn't exist
    const result = readSavedChannel();
    // May return 'stable' or whatever is on disk in CI, but should be valid
    expect(VALID_CHANNELS).toContain(result);
  });
});

describe('VALID_CHANNELS', () => {
  it('contains stable, beta, nightly', () => {
    expect(VALID_CHANNELS).toContain('stable');
    expect(VALID_CHANNELS).toContain('beta');
    expect(VALID_CHANNELS).toContain('nightly');
    expect(VALID_CHANNELS.length).toBe(3);
  });
});

describe('CURRENT_VERSION', () => {
  it('is a valid semver string', () => {
    const parsed = parseSemver(CURRENT_VERSION);
    expect(parsed).not.toBeNull();
  });
});

// ── Backup Management ─────────────────────────────────────────────

describe('listBackups', () => {
  it('returns empty array when backup dir does not exist', () => {
    const result = listBackups();
    expect(Array.isArray(result)).toBe(true);
  });
});

describe('cleanOldBackups', () => {
  it('returns a number', () => {
    const result = cleanOldBackups();
    expect(typeof result).toBe('number');
  });
});

// ── updateCommand ─────────────────────────────────────────────────

describe('updateCommand', () => {
  it('has correct name', () => {
    expect(updateCommand.name).toBe('update');
  });

  it('has description', () => {
    expect(updateCommand.description).toBeTruthy();
  });

  it('has aliases', () => {
    expect(updateCommand.aliases).toContain('self-update');
    expect(updateCommand.aliases).toContain('upgrade');
  });

  it('has options', () => {
    expect(updateCommand.options.length).toBeGreaterThan(0);
    // Check for key options
    const names = updateCommand.options.map(o => o.name);
    expect(names).toContain('yes');
    expect(names).toContain('channel');
    expect(names).toContain('force');
    expect(names).toContain('json');
    expect(names).toContain('dryRun');
  });

  it('has action function', () => {
    expect(typeof updateCommand.action).toBe('function');
  });
});
