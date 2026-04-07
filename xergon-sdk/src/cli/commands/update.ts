/**
 * CLI command: update
 *
 * Self-update CLI for the Xergon SDK binary.
 *
 * Usage:
 *   xergon update              -- check for updates (default)
 *   xergon update check        -- check for updates without applying
 *   xergon update apply        -- download and install latest version
 *   xergon update rollback     -- revert to previous version from backup
 *   xergon update channel      -- show or switch update channel
 *   xergon update --yes        -- non-interactive (skip confirmation)
 *   xergon update --json       -- machine-readable output
 */

import type { Command, CommandOption, ParsedArgs, CLIContext } from '../mod';
import * as fs from 'node:fs';
import * as path from 'node:path';
import * as os from 'node:os';
import * as crypto from 'node:crypto';
import { createWriteStream, chmodSync } from 'node:fs';
import { pipeline } from 'node:stream/promises';
import { Readable } from 'node:stream';

// ── Types ──────────────────────────────────────────────────────────

interface UpdateCheckResult {
  currentVersion: string;
  latestVersion: string;
  updateAvailable: boolean;
  channel: string;
  releaseUrl: string;
  changelog: string;
  publishedAt: string;
}

interface UpdateOptions {
  force: boolean;
  version?: string;
  channel: string;
  dryRun: boolean;
}

interface UpdateResult {
  success: boolean;
  previousVersion: string;
  newVersion: string;
  backupPath?: string;
}

interface PlatformInfo {
  os: string;
  arch: string;
  platform: string;
}

interface SemverParts {
  major: number;
  minor: number;
  patch: number;
  prerelease: string;
  prereleaseNum: number;
}

// ── Constants ──────────────────────────────────────────────────────

const CURRENT_VERSION = '0.1.0';
const GITHUB_API = 'https://api.github.com/repos/xergon-network/xergon-sdk';
const GITHUB_RELEASES = `${GITHUB_API}/releases`;
const CONFIG_DIR = () => path.join(os.homedir(), '.xergon');
const BACKUP_DIR = () => path.join(CONFIG_DIR(), 'backups');
const CHANNEL_FILE = () => path.join(CONFIG_DIR(), 'update-channel');

const CHANNEL_MAP: Record<string, string> = {
  stable: 'latest',
  beta: 'prerelease',
  nightly: 'nightly',
};

const VALID_CHANNELS = ['stable', 'beta', 'nightly'];

// ── Options ────────────────────────────────────────────────────────

const updateOptions: CommandOption[] = [
  {
    name: 'check',
    short: '',
    long: '--check',
    description: 'Check if update is available without updating',
    required: false,
    type: 'boolean',
  },
  {
    name: 'yes',
    short: '-y',
    long: '--yes',
    description: 'Skip confirmation prompt (non-interactive)',
    required: false,
    type: 'boolean',
  },
  {
    name: 'force',
    short: '-f',
    long: '--force',
    description: 'Update without confirmation prompt',
    required: false,
    type: 'boolean',
  },
  {
    name: 'version',
    short: '',
    long: '--version',
    description: 'Update to specific version (e.g. 0.2.0)',
    required: false,
    type: 'string',
  },
  {
    name: 'channel',
    short: '',
    long: '--channel',
    description: 'Update channel: stable, beta, nightly',
    required: false,
    default: 'stable',
    type: 'string',
  },
  {
    name: 'dryRun',
    short: '',
    long: '--dry-run',
    description: 'Show what would happen without actually updating',
    required: false,
    type: 'boolean',
  },
  {
    name: 'json',
    short: '',
    long: '--json',
    description: 'Output as JSON',
    required: false,
    type: 'boolean',
  },
];

// ── Semver Parsing & Comparison ───────────────────────────────────

/**
 * Parse a semver string into its component parts.
 */
export function parseSemver(version: string): SemverParts | null {
  const cleaned = version.replace(/^v/, '').trim();
  const match = cleaned.match(/^(\d+)\.(\d+)\.(\d+)(?:-(alpha|beta|rc|nightly)\.?(\d+))?$/i);
  if (!match) return null;

  return {
    major: parseInt(match[1], 10),
    minor: parseInt(match[2], 10),
    patch: parseInt(match[3], 10),
    prerelease: match[4]?.toLowerCase() ?? '',
    prereleaseNum: match[5] ? parseInt(match[5], 10) : 0,
  };
}

/**
 * Compare two semver versions.
 * Returns -1 if a < b, 0 if a === b, 1 if a > b.
 */
export function compareSemver(a: string, b: string): number {
  const pa = parseSemver(a);
  const pb = parseSemver(b);

  if (!pa && !pb) return 0;
  if (!pa) return -1;
  if (!pb) return 1;

  if (pa.major !== pb.major) return pa.major < pb.major ? -1 : 1;
  if (pa.minor !== pb.minor) return pa.minor < pb.minor ? -1 : 1;
  if (pa.patch !== pb.patch) return pa.patch < pb.patch ? -1 : 1;

  const PRERELEASE_ORDER: Record<string, number> = { alpha: 0, nightly: 1, beta: 2, rc: 3 };
  if (pa.prerelease === '' && pb.prerelease !== '') return 1;
  if (pa.prerelease !== '' && pb.prerelease === '') return -1;

  const orderA = PRERELEASE_ORDER[pa.prerelease] ?? -1;
  const orderB = PRERELEASE_ORDER[pb.prerelease] ?? -1;
  if (orderA !== orderB) return orderA < orderB ? -1 : 1;

  return pa.prereleaseNum < pb.prereleaseNum ? -1 : pa.prereleaseNum > pb.prereleaseNum ? 1 : 0;
}

// ── Platform Detection ────────────────────────────────────────────

/**
 * Detect the current platform for binary download.
 */
export function detectPlatform(): PlatformInfo {
  const rawOs = os.platform();
  const rawArch = os.arch() as string;

  let osName: string;
  switch (rawOs) {
    case 'darwin': osName = 'darwin'; break;
    case 'linux': osName = 'linux'; break;
    case 'win32': osName = 'windows'; break;
    default: osName = rawOs;
  }

  let archName: string;
  switch (rawArch) {
    case 'arm64': archName = 'arm64'; break;
    case 'x64': archName = 'amd64'; break;
    case 'aarch64': archName = 'arm64'; break;
    default: archName = rawArch;
  }

  return {
    os: osName,
    arch: archName,
    platform: `${osName}-${archName}`,
  };
}

/**
 * Get the expected binary filename for a given platform.
 */
export function getBinaryName(platform: PlatformInfo): string {
  if (platform.os === 'windows') {
    return `xergon-${platform.platform}.exe`;
  }
  return `xergon-${platform.platform}`;
}

// ── GitHub Releases API ───────────────────────────────────────────

interface GitHubRelease {
  tag_name: string;
  name: string;
  body: string;
  published_at: string;
  html_url: string;
  prerelease: boolean;
  assets: GitHubAsset[];
}

interface GitHubAsset {
  name: string;
  browser_download_url: string;
}

/**
 * Fetch release info from GitHub.
 */
async function fetchRelease(channel: string, targetVersion?: string): Promise<GitHubRelease | null> {
  try {
    let url: string;

    if (targetVersion) {
      url = `${GITHUB_RELEASES}/tags/v${targetVersion.replace(/^v/, '')}`;
    } else if (channel === 'nightly') {
      url = `${GITHUB_RELEASES}`;
      const res = await fetch(url, {
        headers: { 'Accept': 'application/vnd.github+json', 'User-Agent': 'xergon-cli' },
        signal: AbortSignal.timeout(15_000),
      });
      if (!res.ok) return null;
      const releases: GitHubRelease[] = await res.json();
      return releases.find(r => r.tag_name.includes('nightly')) ?? releases[0] ?? null;
    } else if (channel === 'beta') {
      url = `${GITHUB_RELEASES}`;
      const res = await fetch(url, {
        headers: { 'Accept': 'application/vnd.github+json', 'User-Agent': 'xergon-cli' },
        signal: AbortSignal.timeout(15_000),
      });
      if (!res.ok) return null;
      const releases: GitHubRelease[] = await res.json();
      return releases.find(r => r.prerelease) ?? releases[0] ?? null;
    } else {
      url = `${GITHUB_RELEASES}/latest`;
    }

    const res = await fetch(url, {
      headers: { 'Accept': 'application/vnd.github+json', 'User-Agent': 'xergon-cli' },
      signal: AbortSignal.timeout(15_000),
    });

    if (!res.ok) return null;
    return await res.json() as GitHubRelease;
  } catch {
    return null;
  }
}

/**
 * Check for available updates.
 */
export async function checkForUpdates(channel: string): Promise<UpdateCheckResult> {
  const release = await fetchRelease(channel);

  const latestVersion = release?.tag_name?.replace(/^v/, '') ?? 'unknown';
  const updateAvailable = release !== null && compareSemver(CURRENT_VERSION, latestVersion) < 0;

  return {
    currentVersion: CURRENT_VERSION,
    latestVersion,
    updateAvailable,
    channel,
    releaseUrl: release?.html_url ?? '',
    changelog: release?.body ?? 'No changelog available.',
    publishedAt: release?.published_at ?? '',
  };
}

// ── Channel Management ────────────────────────────────────────────

/**
 * Read the saved channel from disk.
 */
export function readSavedChannel(): string {
  try {
    const data = fs.readFileSync(CHANNEL_FILE(), 'utf-8').trim();
    if (VALID_CHANNELS.includes(data)) return data;
  } catch {
    // No saved channel
  }
  return 'stable';
}

/**
 * Save the channel to disk.
 */
export function saveChannel(channel: string): void {
  const dir = CONFIG_DIR();
  if (!fs.existsSync(dir)) {
    fs.mkdirSync(dir, { recursive: true });
  }
  fs.writeFileSync(CHANNEL_FILE(), channel + '\n');
}

// ── Checksum Verification ─────────────────────────────────────────

/**
 * Compute SHA256 hash of a file.
 */
export async function computeFileSha256(filePath: string): Promise<string> {
  return new Promise((resolve, reject) => {
    const hash = crypto.createHash('sha256');
    const stream = fs.createReadStream(filePath);
    stream.on('data', (chunk: Buffer) => hash.update(chunk));
    stream.on('end', () => resolve(hash.digest('hex')));
    stream.on('error', reject);
  });
}

/**
 * Verify the SHA256 checksum of a downloaded file.
 */
export async function verifyChecksum(filePath: string, expectedHash: string): Promise<boolean> {
  const actual = await computeFileSha256(filePath);
  return actual.toLowerCase() === expectedHash.toLowerCase();
}

// ── Binary Download ───────────────────────────────────────────────

/**
 * Find the correct asset for the current platform from a release.
 */
function findPlatformAsset(release: GitHubRelease, platform: PlatformInfo): GitHubAsset | null {
  const binaryName = getBinaryName(platform);

  const asset = release.assets.find(a =>
    a.name === binaryName ||
    a.name.includes(platform.platform)
  );

  return asset ?? null;
}

/**
 * Find the checksum asset for the current platform.
 */
function findChecksumAsset(release: GitHubRelease, platform: PlatformInfo): GitHubAsset | null {
  const binaryName = getBinaryName(platform);
  const checksumName = `${binaryName}.sha256`;

  return release.assets.find(a => a.name === checksumName) ?? null;
}

/**
 * Download a file from a URL to a local path.
 */
async function downloadFile(url: string, destPath: string): Promise<void> {
  const response = await fetch(url, { signal: AbortSignal.timeout(120_000) });
  if (!response.ok) {
    throw new Error(`Download failed: HTTP ${response.status}`);
  }

  if (!response.body) {
    throw new Error('No response body for download');
  }

  const nodeStream = Readable.fromWeb(response.body as any);
  await pipeline(nodeStream, createWriteStream(destPath));
}

/**
 * Fetch checksum text from GitHub and extract the hash.
 */
async function fetchRemoteChecksum(checksumUrl: string): Promise<string> {
  const res = await fetch(checksumUrl, { signal: AbortSignal.timeout(15_000) });
  if (!res.ok) throw new Error(`Failed to fetch checksum: HTTP ${res.status}`);
  const text = await res.text();
  return text.trim().split(/\s+/)[0];
}

// ── Backup & Rollback ─────────────────────────────────────────────

/**
 * Create a backup of the current binary.
 */
export async function backupCurrentBinary(currentPath: string): Promise<string> {
  const backupDir = BACKUP_DIR();
  if (!fs.existsSync(backupDir)) {
    fs.mkdirSync(backupDir, { recursive: true });
  }

  const timestamp = new Date().toISOString().replace(/[:.]/g, '-');
  const backupName = `xergon-backup-${timestamp}`;
  const backupPath = path.join(backupDir, backupName);

  fs.copyFileSync(currentPath, backupPath);

  try { chmodSync(backupPath, 0o755); } catch { /* non-unix */ }

  return backupPath;
}

/**
 * Restore a backup binary (rollback).
 */
export async function rollbackToBackup(backupPath: string, targetPath: string): Promise<void> {
  if (!fs.existsSync(backupPath)) {
    throw new Error(`Backup not found at ${backupPath}`);
  }
  fs.copyFileSync(backupPath, targetPath);
  try { chmodSync(targetPath, 0o755); } catch { /* non-unix */ }
}

/**
 * Get list of available backups sorted by date (newest first).
 */
export function listBackups(): Array<{ name: string; path: string; size: number; mtime: string }> {
  const backupDir = BACKUP_DIR();
  if (!fs.existsSync(backupDir)) return [];

  return fs.readdirSync(backupDir)
    .filter(f => f.startsWith('xergon-backup-'))
    .map(f => {
      const p = path.join(backupDir, f);
      const stat = fs.statSync(p);
      return {
        name: f,
        path: p,
        size: stat.size,
        mtime: stat.mtime.toISOString(),
      };
    })
    .sort((a, b) => b.mtime.localeCompare(a.mtime));
}

/**
 * Clean up old backups (keep only the 5 most recent).
 */
export function cleanOldBackups(): number {
  const backupDir = BACKUP_DIR();
  if (!fs.existsSync(backupDir)) return 0;

  const files = fs.readdirSync(backupDir)
    .filter(f => f.startsWith('xergon-backup-'))
    .map(f => ({ name: f, path: path.join(backupDir, f), mtime: fs.statSync(path.join(backupDir, f)).mtimeMs }))
    .sort((a, b) => b.mtime - a.mtime);

  let removed = 0;
  for (let i = 5; i < files.length; i++) {
    fs.unlinkSync(files[i].path);
    removed++;
  }
  return removed;
}

// ── Self-Update Verification ──────────────────────────────────────

/**
 * Run a quick health check on the updated binary.
 */
async function verifyUpdatedBinary(binaryPath: string): Promise<boolean> {
  try {
    const { execSync } = await import('node:child_process');
    const output = execSync(`"${binaryPath}" --version`, {
      timeout: 10_000,
      encoding: 'utf-8',
      stdio: ['pipe', 'pipe', 'pipe'],
    }).trim();

    return output.includes('xergon') && output.includes('.');
  } catch {
    return false;
  }
}

// ── Update Execution ──────────────────────────────────────────────

/**
 * Execute the update process.
 */
async function executeUpdate(options: UpdateOptions, ctx: CLIContext): Promise<UpdateResult> {
  const platform = detectPlatform();
  const output = ctx.output;

  output.write(output.colorize('Checking for updates...', 'cyan') + '\n');

  // Step 1: Fetch release info
  const release = await fetchRelease(options.channel, options.version);
  if (!release) {
    throw new Error(`No release found${options.version ? ` for version ${options.version}` : ` on ${options.channel} channel`}. Check your network connection.`);
  }

  const newVersion = release.tag_name.replace(/^v/, '');
  output.write(`  Current version: ${output.colorize(CURRENT_VERSION, 'dim')}\n`);
  output.write(`  Target version:  ${output.colorize(newVersion, 'green')}\n`);
  output.write(`  Channel:         ${options.channel}\n`);
  output.write(`  Platform:        ${platform.platform}\n`);
  output.write(`  Published:       ${release.published_at}\n`);

  // Step 2: Check if already up to date
  if (!options.version && compareSemver(CURRENT_VERSION, newVersion) >= 0) {
    output.write(output.colorize('\n  Already up to date!', 'green') + '\n');
    return { success: true, previousVersion: CURRENT_VERSION, newVersion: CURRENT_VERSION };
  }

  // Step 3: Find the right asset
  const asset = findPlatformAsset(release, platform);
  if (!asset) {
    throw new Error(`No binary found for platform ${platform.platform} in release ${newVersion}. Available assets: ${release.assets.map(a => a.name).join(', ')}`);
  }

  output.write(`  Binary:          ${asset.name}\n`);

  // Step 4: Dry run
  if (options.dryRun) {
    output.write(output.colorize('\n  [DRY RUN] Would download and install:', 'yellow') + '\n');
    output.write(`    Source: ${asset.browser_download_url}\n`);
    output.write(`    Version: ${newVersion}\n`);
    output.write(`    Channel: ${options.channel}\n`);
    return { success: true, previousVersion: CURRENT_VERSION, newVersion, backupPath: undefined };
  }

  // Step 5: Confirm (unless --force or --yes)
  if (!options.force) {
    output.write(output.colorize('\n  Proceed with update? [y/N] ', 'bold'));
    const answer = await new Promise<string>((resolve) => {
      process.stdin.setRawMode?.(false);
      process.stdin.resume();
      process.stdin.once('data', (data: Buffer) => {
        resolve(data.toString().trim().toLowerCase());
      });
    });
    process.stdin.pause();

    if (answer !== 'y' && answer !== 'yes') {
      output.write(output.colorize('Update cancelled.', 'yellow') + '\n');
      return { success: false, previousVersion: CURRENT_VERSION, newVersion };
    }
  }

  // Step 6: Backup current binary
  const currentBinary = process.argv[0] ?? process.execPath;
  output.write(output.colorize('\n  Backing up current binary...', 'cyan') + '\n');
  let backupPath: string | undefined;
  try {
    backupPath = await backupCurrentBinary(currentBinary);
    output.write(`    Backup saved to: ${output.colorize(backupPath, 'dim')}\n`);
  } catch (err) {
    output.write(output.colorize(`    Warning: Backup failed: ${err instanceof Error ? err.message : String(err)}`, 'yellow') + '\n');
  }

  // Step 7: Download new binary
  const tmpDir = os.tmpdir();
  const tmpPath = path.join(tmpDir, `xergon-update-${newVersion}-${Date.now()}`);

  output.write(output.colorize('  Downloading new binary...', 'cyan') + '\n');
  try {
    await downloadFile(asset.browser_download_url, tmpPath);
  } catch (err) {
    output.write(output.colorize(`    Download failed: ${err instanceof Error ? err.message : String(err)}`, 'red') + '\n');
    if (backupPath) {
      output.write(output.colorize('  Rolling back to backup...', 'yellow') + '\n');
      await rollbackToBackup(backupPath, currentBinary);
      output.write(output.colorize('  Rollback complete.', 'green') + '\n');
    }
    throw new Error(`Download failed: ${err instanceof Error ? err.message : String(err)}`);
  }

  // Step 8: Verify checksum
  const checksumAsset = findChecksumAsset(release, platform);
  if (checksumAsset) {
    output.write(output.colorize('  Verifying checksum...', 'cyan') + '\n');
    try {
      const expectedHash = await fetchRemoteChecksum(checksumAsset.browser_download_url);
      const valid = await verifyChecksum(tmpPath, expectedHash);
      if (!valid) {
        fs.unlinkSync(tmpPath);
        if (backupPath) {
          output.write(output.colorize('  Checksum mismatch! Rolling back...', 'red') + '\n');
          await rollbackToBackup(backupPath, currentBinary);
        }
        throw new Error('Checksum verification failed. The downloaded binary may be corrupted or tampered with.');
      }
      output.write(`    Checksum verified: ${output.colorize(expectedHash.substring(0, 16) + '...', 'green')}\n`);
    } catch (err) {
      if (err instanceof Error && err.message.includes('Checksum verification')) throw err;
      output.write(output.colorize('    Warning: Could not verify checksum, proceeding anyway.', 'yellow') + '\n');
    }
  } else {
    output.write(output.colorize('    No checksum asset found, skipping verification.', 'yellow') + '\n');
  }

  // Step 9: Replace binary
  output.write(output.colorize('  Installing new binary...', 'cyan') + '\n');
  try {
    fs.copyFileSync(tmpPath, currentBinary);
    try { chmodSync(currentBinary, 0o755); } catch { /* non-unix */ }
    try { fs.unlinkSync(tmpPath); } catch { /* ignore */ }
  } catch (err) {
    output.write(output.colorize(`    Install failed: ${err instanceof Error ? err.message : String(err)}`, 'red') + '\n');
    if (backupPath) {
      output.write(output.colorize('  Rolling back to backup...', 'yellow') + '\n');
      await rollbackToBackup(backupPath, currentBinary);
      output.write(output.colorize('  Rollback complete.', 'green') + '\n');
    }
    throw new Error(`Install failed: ${err instanceof Error ? err.message : String(err)}`);
  }

  // Step 10: Verify updated binary
  output.write(output.colorize('  Verifying update...', 'cyan') + '\n');
  const verified = await verifyUpdatedBinary(currentBinary);
  if (!verified) {
    output.write(output.colorize('    Warning: Updated binary verification failed. It may still work correctly.', 'yellow') + '\n');
  } else {
    output.write(output.colorize('    Updated binary verified successfully!', 'green') + '\n');
  }

  // Step 11: Clean old backups
  const removed = cleanOldBackups();
  if (removed > 0) {
    output.write(`    Cleaned up ${removed} old backup(s).\n`);
  }

  // Step 12: Show changelog
  if (release.body) {
    output.write(output.colorize('\n  Changelog:', 'bold') + '\n');
    output.write(output.colorize('  ' + '\u2500'.repeat(50), 'dim') + '\n');
    const lines = release.body.split('\n').slice(0, 20);
    for (const line of lines) {
      output.write(`  ${line}\n`);
    }
    if (release.body.split('\n').length > 20) {
      output.write(`  ${output.colorize('... (truncated)', 'dim')}\n`);
    }
    output.write(output.colorize('  ' + '\u2500'.repeat(50), 'dim') + '\n');
  }

  output.write(output.colorize(`\n  Successfully updated to v${newVersion}!`, 'green') + '\n');

  return {
    success: true,
    previousVersion: CURRENT_VERSION,
    newVersion,
    backupPath,
  };
}

// ── Output Rendering ──────────────────────────────────────────────

function renderCheckResult(result: UpdateCheckResult, output: any): string {
  const lines: string[] = [];

  lines.push(output.colorize('Xergon SDK Update Check', 'bold'));
  lines.push(output.colorize('\u2500'.repeat(50), 'dim'));
  lines.push('');

  lines.push(`  Current Version:  ${output.colorize(result.currentVersion, result.updateAvailable ? 'yellow' : 'green')}`);
  lines.push(`  Latest Version:   ${output.colorize(result.latestVersion, 'green')}`);
  lines.push(`  Channel:          ${result.channel}`);
  lines.push(`  Update Available: ${result.updateAvailable ? output.colorize('YES', 'green') : output.colorize('No', 'dim')}`);

  if (result.publishedAt) {
    lines.push(`  Published:        ${new Date(result.publishedAt).toLocaleDateString()}`);
  }

  if (result.releaseUrl) {
    lines.push(`  Release:          ${result.releaseUrl}`);
  }

  if (result.updateAvailable && result.changelog) {
    lines.push('');
    lines.push(output.colorize('  Changelog:', 'bold'));
    lines.push(output.colorize('  ' + '\u2500'.repeat(46), 'dim'));
    const changelogLines = result.changelog.split('\n').slice(0, 15);
    for (const line of changelogLines) {
      lines.push(`  ${line}`);
    }
    if (result.changelog.split('\n').length > 15) {
      lines.push(`  ${output.colorize('... (see full release notes online)', 'dim')}`);
    }
  }

  if (result.updateAvailable) {
    lines.push('');
    lines.push(`  Run ${output.colorize('xergon update apply', 'cyan')} to install the latest version.`);
  }

  lines.push('');
  return lines.join('\n');
}

function renderChannelStatus(channel: string, output: any): string {
  const lines: string[] = [];
  lines.push(output.colorize('Update Channel', 'bold'));
  lines.push(output.colorize('\u2500'.repeat(50), 'dim'));
  lines.push('');
  lines.push(`  Current channel: ${output.colorize(channel.toUpperCase(), 'green')}`);
  lines.push('');
  lines.push('  Available channels:');
  for (const ch of VALID_CHANNELS) {
    const active = ch === channel;
    const marker = active ? output.colorize('  > ', 'green') : '    ';
    const desc: Record<string, string> = {
      stable: 'Stable releases only',
      beta: 'Pre-release builds for testing',
      nightly: 'Nightly development builds',
    };
    lines.push(`${marker}${output.colorize(ch.padEnd(10), active ? 'green' : 'dim')} ${desc[ch]}`);
  }
  lines.push('');
  lines.push(`  Switch channel: ${output.colorize('xergon update channel <stable|beta|nightly>', 'cyan')}`);
  lines.push('');
  return lines.join('\n');
}

// ── Subcommand Handlers ───────────────────────────────────────────

async function handleCheck(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const outputJson = args.options.json === true;
  const channel = String(args.options.channel ?? readSavedChannel());
  const targetVersion = args.options.version as string | undefined;

  const checkResult = await checkForUpdates(channel);

  if (targetVersion) {
    checkResult.latestVersion = targetVersion;
    checkResult.updateAvailable = compareSemver(CURRENT_VERSION, targetVersion) < 0;
  }

  if (outputJson) {
    ctx.output.write(JSON.stringify(checkResult, null, 2));
  } else {
    ctx.output.write(renderCheckResult(checkResult, ctx.output));
  }
}

async function handleApply(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const outputJson = args.options.json === true;
  const force = args.options.force === true || args.options.yes === true;
  const dryRun = args.options.dryRun === true;
  const targetVersion = args.options.version as string | undefined;
  const channel = String(args.options.channel ?? readSavedChannel());

  if (targetVersion && !parseSemver(targetVersion)) {
    ctx.output.write(`Invalid version format "${targetVersion}". Expected X.Y.Z or X.Y.Z-prerelease.N\n`);
    process.exit(1);
  }

  const options: UpdateOptions = { force, version: targetVersion, channel, dryRun };

  try {
    const result = await executeUpdate(options, ctx);

    if (outputJson) {
      ctx.output.write(JSON.stringify(result, null, 2));
    }

    if (!result.success && !outputJson) {
      process.exit(1);
    }
  } catch (err) {
    if (outputJson) {
      ctx.output.write(JSON.stringify({
        success: false,
        error: err instanceof Error ? err.message : String(err),
      }, null, 2));
    } else {
      ctx.output.write(`\n${ctx.output.colorize('Error:', 'red')} ${err instanceof Error ? err.message : String(err)}\n`);
    }
    process.exit(1);
  }
}

async function handleRollback(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const outputJson = args.options.json === true;
  const backups = listBackups();

  if (backups.length === 0) {
    ctx.output.write(ctx.output.colorize('No backups available for rollback.', 'yellow') + '\n');
    process.exit(1);
    return;
  }

  // Find the backup to restore
  let targetBackup: typeof backups[0] | undefined;
  const backupName = args.positional[1];

  if (backupName) {
    targetBackup = backups.find(b => b.name === backupName || b.path === backupName);
    if (!targetBackup) {
      ctx.output.write(ctx.output.colorize(`Backup "${backupName}" not found.`, 'red') + '\n');
      process.exit(1);
      return;
    }
  } else {
    targetBackup = backups[0]; // Most recent
  }

  if (!outputJson) {
    ctx.output.write(ctx.output.colorize('Available Backups:', 'bold') + '\n');
    for (const b of backups.slice(0, 5)) {
      const isTarget = b.name === targetBackup.name;
      const marker = isTarget ? ctx.output.colorize(' > ', 'green') : '   ';
      const sizeKB = (b.size / 1024).toFixed(1);
      ctx.output.write(`${marker}${b.name}  (${sizeKB} KB, ${b.mtime})\n`);
    }
    ctx.output.write('');
  }

  // Perform rollback
  const currentBinary = process.argv[0] ?? process.execPath;
  try {
    // Backup current before rollback
    const preRollbackBackup = await backupCurrentBinary(currentBinary);

    await rollbackToBackup(targetBackup.path, currentBinary);

    if (outputJson) {
      ctx.output.write(JSON.stringify({
        success: true,
        restoredFrom: targetBackup.path,
        preRollbackBackup,
      }, null, 2));
    } else {
      ctx.output.write(ctx.output.colorize('Rollback complete!', 'green') + '\n');
      ctx.output.write(`  Restored from: ${targetBackup.path}\n`);
      ctx.output.write(`  Pre-rollback backup: ${preRollbackBackup}\n`);
    }
  } catch (err) {
    if (outputJson) {
      ctx.output.write(JSON.stringify({
        success: false,
        error: err instanceof Error ? err.message : String(err),
      }, null, 2));
    } else {
      ctx.output.write(ctx.output.colorize(`Rollback failed: ${err instanceof Error ? err.message : String(err)}`, 'red') + '\n');
    }
    process.exit(1);
  }
}

async function handleChannel(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const outputJson = args.options.json === true;
  const newChannel = args.positional[1];

  const currentChannel = readSavedChannel();

  if (!newChannel) {
    // Show current channel
    if (outputJson) {
      ctx.output.write(JSON.stringify({ currentChannel, available: VALID_CHANNELS }, null, 2));
    } else {
      ctx.output.write(renderChannelStatus(currentChannel, ctx.output));
    }
    return;
  }

  // Validate and set new channel
  if (!VALID_CHANNELS.includes(newChannel)) {
    ctx.output.write(`Invalid channel "${newChannel}". Must be one of: ${VALID_CHANNELS.join(', ')}\n`);
    process.exit(1);
    return;
  }

  if (newChannel === currentChannel) {
    ctx.output.write(ctx.output.colorize(`Already on ${newChannel} channel.`, 'yellow') + '\n');
    return;
  }

  saveChannel(newChannel);

  if (outputJson) {
    ctx.output.write(JSON.stringify({
      previousChannel: currentChannel,
      currentChannel: newChannel,
      success: true,
    }, null, 2));
  } else {
    ctx.output.write(ctx.output.colorize(`Switched to ${newChannel.toUpperCase()} channel.`, 'green') + '\n');
  }
}

// ── Command Action ────────────────────────────────────────────────

async function updateAction(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const sub = args.positional[0];

  switch (sub) {
    case 'check':
      await handleCheck(args, ctx);
      break;
    case 'apply':
    case 'install':
    case 'upgrade':
      await handleApply(args, ctx);
      break;
    case 'rollback':
    case 'revert':
    case 'restore':
      await handleRollback(args, ctx);
      break;
    case 'channel':
    case 'switch':
      await handleChannel(args, ctx);
      break;
    default:
      // Default: check for updates
      await handleCheck(args, ctx);
      break;
  }
}

// ── Command Definition ────────────────────────────────────────────

export const updateCommand: Command = {
  name: 'update',
  description: 'Check for updates, apply, rollback, or switch update channel',
  aliases: ['self-update', 'upgrade'],
  options: updateOptions,
  action: updateAction,
};

// Export types and helpers for testing
export { type UpdateCheckResult, type UpdateOptions, type UpdateResult, type PlatformInfo, type SemverParts };
export { CURRENT_VERSION, VALID_CHANNELS, BACKUP_DIR };
