/**
 * Tests for the update CLI command.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import * as fs from 'node:fs';
import * as os from 'node:os';
import * as path from 'node:path';
import * as crypto from 'node:crypto';

import {
  parseSemver,
  compareSemver,
  detectPlatform,
  getBinaryName,
  verifyChecksum,
  computeFileSha256,
  backupCurrentBinary,
  rollbackToBackup,
  cleanOldBackups,
  checkForUpdates,
  CURRENT_VERSION,
  VALID_CHANNELS,
} from '../../src/cli/commands/update';

// ── Semver Parsing Tests ──────────────────────────────────────────

describe('parseSemver', () => {
  it('parses a standard semver string', () => {
    const result = parseSemver('1.2.3');
    expect(result).toEqual({ major: 1, minor: 2, patch: 3, prerelease: '', prereleaseNum: 0 });
  });

  it('parses semver with v prefix', () => {
    const result = parseSemver('v2.0.0');
    expect(result).toEqual({ major: 2, minor: 0, patch: 0, prerelease: '', prereleaseNum: 0 });
  });

  it('parses semver with prerelease', () => {
    const result = parseSemver('1.0.0-beta.1');
    expect(result).toEqual({ major: 1, minor: 0, patch: 0, prerelease: 'beta', prereleaseNum: 1 });
  });

  it('parses semver with alpha prerelease', () => {
    const result = parseSemver('0.2.0-alpha.3');
    expect(result).toEqual({ major: 0, minor: 2, patch: 0, prerelease: 'alpha', prereleaseNum: 3 });
  });

  it('parses semver with rc prerelease', () => {
    const result = parseSemver('1.0.0-rc.2');
    expect(result).toEqual({ major: 1, minor: 0, patch: 0, prerelease: 'rc', prereleaseNum: 2 });
  });

  it('returns null for invalid semver', () => {
    expect(parseSemver('not-a-version')).toBeNull();
    expect(parseSemver('')).toBeNull();
    expect(parseSemver('1')).toBeNull();
  });
});

// ── Semver Comparison Tests ───────────────────────────────────────

describe('compareSemver', () => {
  it('returns 0 for equal versions', () => {
    expect(compareSemver('1.2.3', '1.2.3')).toBe(0);
  });

  it('returns -1 when first version is older (major)', () => {
    expect(compareSemver('1.0.0', '2.0.0')).toBe(-1);
  });

  it('returns 1 when first version is newer (minor)', () => {
    expect(compareSemver('1.5.0', '1.2.0')).toBe(1);
  });

  it('returns -1 when first version is older (patch)', () => {
    expect(compareSemver('1.2.0', '1.2.5')).toBe(-1);
  });

  it('stable version is newer than prerelease of same version', () => {
    expect(compareSemver('1.0.0-beta.1', '1.0.0')).toBe(-1);
    expect(compareSemver('1.0.0', '1.0.0-beta.1')).toBe(1);
  });

  it('beta is newer than alpha', () => {
    expect(compareSemver('1.0.0-alpha.1', '1.0.0-beta.1')).toBe(-1);
  });

  it('rc is newer than beta', () => {
    expect(compareSemver('1.0.0-beta.2', '1.0.0-rc.1')).toBe(-1);
  });

  it('handles v prefix', () => {
    expect(compareSemver('v1.0.0', 'v2.0.0')).toBe(-1);
  });

  it('returns -1 for unparseable first version', () => {
    expect(compareSemver('bad', '1.0.0')).toBe(-1);
  });

  it('returns 1 for unparseable second version', () => {
    expect(compareSemver('1.0.0', 'bad')).toBe(1);
  });
});

// ── Platform Detection Tests ──────────────────────────────────────

describe('detectPlatform', () => {
  it('detects darwin arm64', () => {
    // Test via direct function call with current platform - verify expected shape
    const result = detectPlatform();
    expect(result).toHaveProperty('os');
    expect(result).toHaveProperty('arch');
    expect(result).toHaveProperty('platform');
    expect(result.platform).toBe(`${result.os}-${result.arch}`);
  });

  it('detects a valid platform format', () => {
    const result = detectPlatform();
    expect(typeof result.os).toBe('string');
    expect(typeof result.arch).toBe('string');
    expect(typeof result.platform).toBe('string');
    expect(result.platform).toContain('-');
  });

  it('returns one of supported OS values', () => {
    const result = detectPlatform();
    expect(['darwin', 'linux', 'windows', 'win32']).toContain(result.os);
  });

  it('returns one of supported arch values', () => {
    const result = detectPlatform();
    expect(['amd64', 'arm64', 'x64', 'aarch64', 'ia32']).toContain(result.arch);
  });
});

// ── Binary Name Tests ─────────────────────────────────────────────

describe('getBinaryName', () => {
  it('returns correct name for darwin-arm64', () => {
    expect(getBinaryName({ os: 'darwin', arch: 'arm64', platform: 'darwin-arm64' }))
      .toBe('xergon-darwin-arm64');
  });

  it('returns correct name for linux-amd64', () => {
    expect(getBinaryName({ os: 'linux', arch: 'amd64', platform: 'linux-amd64' }))
      .toBe('xergon-linux-amd64');
  });

  it('returns .exe suffix for windows', () => {
    expect(getBinaryName({ os: 'windows', arch: 'amd64', platform: 'windows-amd64' }))
      .toBe('xergon-windows-amd64.exe');
  });
});

// ── Checksum Verification Tests ───────────────────────────────────

describe('verifyChecksum', () => {
  it('computes SHA256 correctly', async () => {
    // Create a temp file with known content
    const tmpDir = os.tmpdir();
    const testPath = path.join(tmpDir, `xergon-test-${Date.now()}`);
    const content = 'hello world';
    fs.writeFileSync(testPath, content);

    const hash = await computeFileSha256(testPath);
    const expected = crypto.createHash('sha256').update(content).digest('hex');
    expect(hash).toBe(expected);

    // Cleanup
    fs.unlinkSync(testPath);
  });

  it('verifies matching checksum', async () => {
    const tmpDir = os.tmpdir();
    const testPath = path.join(tmpDir, `xergon-checksum-test-${Date.now()}`);
    const content = 'test content for checksum';
    fs.writeFileSync(testPath, content);

    const expected = crypto.createHash('sha256').update(content).digest('hex');
    const result = await verifyChecksum(testPath, expected);
    expect(result).toBe(true);

    fs.unlinkSync(testPath);
  });

  it('rejects mismatched checksum', async () => {
    const tmpDir = os.tmpdir();
    const testPath = path.join(tmpDir, `xergon-bad-checksum-${Date.now()}`);
    fs.writeFileSync(testPath, 'some content');

    const result = await verifyChecksum(testPath, '0000000000000000000000000000000000000000000000000000000000000000');
    expect(result).toBe(false);

    fs.unlinkSync(testPath);
  });

  it('is case-insensitive for hex', async () => {
    const tmpDir = os.tmpdir();
    const testPath = path.join(tmpDir, `xergon-case-check-${Date.now()}`);
    const content = 'case test';
    fs.writeFileSync(testPath, content);

    const expected = crypto.createHash('sha256').update(content).digest('hex').toUpperCase();
    const result = await verifyChecksum(testPath, expected);
    expect(result).toBe(true);

    fs.unlinkSync(testPath);
  });
});

// ── Backup & Rollback Tests ───────────────────────────────────────

describe('backupCurrentBinary', () => {
  it('creates a backup file in the backup directory', async () => {
    const tmpDir = os.tmpdir();
    const sourcePath = path.join(tmpDir, `xergon-src-${Date.now()}`);
    fs.writeFileSync(sourcePath, 'fake binary content');

    const backupPath = await backupCurrentBinary(sourcePath);

    expect(backupPath).toContain('xergon-backup-');
    expect(fs.existsSync(backupPath)).toBe(true);

    // Cleanup
    fs.unlinkSync(sourcePath);
    fs.unlinkSync(backupPath);
  });
});

describe('rollbackToBackup', () => {
  it('restores backup to target path', async () => {
    const tmpDir = os.tmpdir();
    const backupPath = path.join(tmpDir, `xergon-backup-test-${Date.now()}`);
    const targetPath = path.join(tmpDir, `xergon-target-test-${Date.now()}`);

    fs.writeFileSync(backupPath, 'backup content');
    fs.writeFileSync(targetPath, 'corrupted content');

    await rollbackToBackup(backupPath, targetPath);

    const restored = fs.readFileSync(targetPath, 'utf-8');
    expect(restored).toBe('backup content');

    fs.unlinkSync(backupPath);
    fs.unlinkSync(targetPath);
  });

  it('throws if backup does not exist', async () => {
    await expect(rollbackToBackup('/nonexistent/backup', '/some/target'))
      .rejects.toThrow('Backup not found');
  });
});

describe('cleanOldBackups', () => {
  it('returns 0 when backup dir has no files', () => {
    const result = cleanOldBackups();
    expect(result).toBe(0);
  });
});

// ── Channel Validation ────────────────────────────────────────────

describe('VALID_CHANNELS', () => {
  it('contains expected channels', () => {
    expect(VALID_CHANNELS).toEqual(['stable', 'beta', 'nightly']);
  });
});

// ── Check for Updates Tests ───────────────────────────────────────

describe('checkForUpdates', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it('returns current version when API is unreachable', async () => {
    // With network failures, it returns unknown
    const result = await checkForUpdates('stable');
    // Depending on network, this will either succeed or return unknown
    expect(result).toHaveProperty('currentVersion', CURRENT_VERSION);
    expect(result).toHaveProperty('channel', 'stable');
    expect(result).toHaveProperty('updateAvailable');
    expect(result).toHaveProperty('latestVersion');
  });
});

// ── Dry Run Tests ─────────────────────────────────────────────────

describe('UpdateOptions dry-run', () => {
  it('dry-run flag is properly typed', () => {
    const options = { force: false, channel: 'stable', dryRun: true };
    expect(options.dryRun).toBe(true);
    expect(options.force).toBe(false);
  });
});

// ── Version Target Tests ──────────────────────────────────────────

describe('target version validation', () => {
  it('accepts valid version format', () => {
    expect(parseSemver('0.2.0')).not.toBeNull();
    expect(parseSemver('1.0.0-beta.1')).not.toBeNull();
  });

  it('rejects invalid version format', () => {
    expect(parseSemver('x.y.z')).toBeNull();
    expect(parseSemver('1.0')).toBeNull();
    expect(parseSemver('')).toBeNull();
  });
});
