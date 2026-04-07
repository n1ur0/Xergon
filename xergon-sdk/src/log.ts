/**
 * Enhanced Logging System
 *
 * Provides structured logging with configurable levels, in-memory
 * history, and export capabilities for the Xergon SDK.
 *
 * Config file: ~/.xergon/log-config.json
 */

import * as fs from 'node:fs';
import * as os from 'node:os';
import * as path from 'node:path';

// ── Types ─────────────────────────────────────────────────────────

export enum LogLevel {
  Debug = 0,
  Info = 1,
  Warn = 2,
  Error = 3,
  None = 4,
}

export interface LogEntry {
  timestamp: string;
  level: LogLevel;
  message: string;
  context?: string;
  data?: unknown;
}

export interface LogConfig {
  level: LogLevel;
  maxHistory: number;
  persistToFile: boolean;
}

// ── Storage ───────────────────────────────────────────────────────

const LOG_DIR = path.join(os.homedir(), '.xergon');
const LOG_CONFIG_FILE = path.join(LOG_DIR, 'log-config.json');

const LEVEL_NAMES: Record<LogLevel, string> = {
  [LogLevel.Debug]: 'DEBUG',
  [LogLevel.Info]: 'INFO',
  [LogLevel.Warn]: 'WARN',
  [LogLevel.Error]: 'ERROR',
  [LogLevel.None]: 'NONE',
};

// ── State ─────────────────────────────────────────────────────────

let currentLevel: LogLevel = LogLevel.Info;
let history: LogEntry[] = [];
let maxHistorySize = 1000;
let persistToFile = false;

/**
 * Load log configuration from disk.
 */
function loadConfig(): LogConfig {
  try {
    const data = fs.readFileSync(LOG_CONFIG_FILE, 'utf-8');
    return JSON.parse(data) as LogConfig;
  } catch {
    return {
      level: LogLevel.Info,
      maxHistory: 1000,
      persistToFile: false,
    };
  }
}

/**
 * Save log configuration to disk.
 */
function saveConfig(config: LogConfig): void {
  try {
    fs.mkdirSync(LOG_DIR, { recursive: true });
    fs.writeFileSync(LOG_CONFIG_FILE, JSON.stringify(config, null, 2), 'utf-8');
  } catch {
    // Silently ignore config save failures
  }
}

/**
 * Initialize logging from config file.
 */
function initFromConfig(): void {
  const config = loadConfig();
  currentLevel = config.level ?? LogLevel.Info;
  maxHistorySize = config.maxHistory ?? 1000;
  persistToFile = config.persistToFile ?? false;
}

// Initialize on import
initFromConfig();

// ── Internal helpers ──────────────────────────────────────────────

function addEntry(entry: LogEntry): void {
  history.push(entry);

  // Trim to max size
  if (history.length > maxHistorySize) {
    history = history.slice(-maxHistorySize);
  }
}

function formatEntry(entry: LogEntry): string {
  const parts = [entry.timestamp, LEVEL_NAMES[entry.level]];
  if (entry.context) parts.push(`[${entry.context}]`);
  parts.push(entry.message);
  return parts.join(' ');
}

function persistEntry(entry: LogEntry): void {
  if (!persistToFile) return;
  try {
    const logFile = path.join(LOG_DIR, 'xergon.log');
    fs.mkdirSync(LOG_DIR, { recursive: true });
    fs.appendFileSync(logFile, formatEntry(entry) + '\n', 'utf-8');
  } catch {
    // Silently ignore file write failures
  }
}

// ── Public API ────────────────────────────────────────────────────

/**
 * Set the global log level.
 */
export function setLevel(level: LogLevel): void {
  currentLevel = level;
  const config = loadConfig();
  config.level = level;
  saveConfig(config);
}

/**
 * Get the current log level.
 */
export function getLevel(): LogLevel {
  return currentLevel;
}

/**
 * Log a debug message.
 */
export function debug(message: string, context?: string, data?: unknown): void {
  if (currentLevel > LogLevel.Debug) return;
  const entry: LogEntry = {
    timestamp: new Date().toISOString(),
    level: LogLevel.Debug,
    message,
    context,
    data,
  };
  addEntry(entry);
  persistEntry(entry);
}

/**
 * Log an info message.
 */
export function info(message: string, context?: string, data?: unknown): void {
  if (currentLevel > LogLevel.Info) return;
  const entry: LogEntry = {
    timestamp: new Date().toISOString(),
    level: LogLevel.Info,
    message,
    context,
    data,
  };
  addEntry(entry);
  persistEntry(entry);
}

/**
 * Log a warning message.
 */
export function warn(message: string, context?: string, data?: unknown): void {
  if (currentLevel > LogLevel.Warn) return;
  const entry: LogEntry = {
    timestamp: new Date().toISOString(),
    level: LogLevel.Warn,
    message,
    context,
    data,
  };
  addEntry(entry);
  persistEntry(entry);
}

/**
 * Log an error message.
 */
export function error(message: string, context?: string, data?: unknown): void {
  if (currentLevel > LogLevel.Error) return;
  const entry: LogEntry = {
    timestamp: new Date().toISOString(),
    level: LogLevel.Error,
    message,
    context,
    data,
  };
  addEntry(entry);
  persistEntry(entry);
}

/**
 * Get the in-memory log history (last N entries).
 */
export function getHistory(limit?: number): LogEntry[] {
  if (limit !== undefined) {
    return history.slice(-limit);
  }
  return [...history];
}

/**
 * Export logs as JSON or plain text.
 */
export function exportLogs(format: 'json' | 'text' = 'text'): string {
  if (format === 'json') {
    return JSON.stringify(history, null, 2);
  }

  return history.map(formatEntry).join('\n');
}

/**
 * Clear the in-memory log history.
 */
export function clearHistory(): void {
  history = [];
}
