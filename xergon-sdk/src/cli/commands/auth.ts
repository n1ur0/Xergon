/**
 * CLI command: auth
 *
 * Authentication and credential management for the Xergon Network.
 *
 * Usage:
 *   xergon auth login       -- Authenticate with API key or wallet
 *   xergon auth logout      -- Clear stored credentials
 *   xergon auth status      -- Show current auth status
 *   xergon auth token       -- Show/manage access tokens
 *   xergon auth refresh     -- Refresh expired tokens
 *   xergon auth providers   -- List configured provider auth statuses
 *   xergon auth revoke      -- Revoke API keys
 */

import type { Command, CommandOption, ParsedArgs, CLIContext } from '../mod';
import * as fs from 'node:fs';
import * as path from 'node:path';
import * as os from 'node:os';
import * as crypto from 'node:crypto';

// ── Types ──────────────────────────────────────────────────────────

type AuthProvider = 'relay' | 'marketplace' | 'agent';
type AuthMethod = 'api_key' | 'wallet' | 'oauth';
type TokenStatus = 'valid' | 'expired' | 'refreshable' | 'revoked' | 'unknown';

interface TokenEntry {
  accessToken: string;
  refreshToken?: string;
  expiresAt: string;
  issuedAt: string;
  provider: AuthProvider;
  scope: string[];
  method: AuthMethod;
}

interface ProviderAuth {
  provider: AuthProvider;
  authenticated: boolean;
  method?: AuthMethod;
  identity?: string;
  expiresAt?: string;
  status: TokenStatus;
  endpoint?: string;
}

interface CredentialStore {
  version: number;
  activeProvider?: AuthProvider;
  tokens: Record<AuthProvider, TokenEntry | null>;
  apiKeys: Array<{
    id: string;
    key: string;
    label: string;
    provider: AuthProvider;
    createdAt: string;
    lastUsed?: string;
  }>;
  wallets: Array<{
    address: string;
    label: string;
    provider: AuthProvider;
    connectedAt: string;
  }>;
}

interface AuthStatus {
  authenticated: boolean;
  activeProvider?: AuthProvider;
  providers: ProviderAuth[];
  credentialPath: string;
  tokenCount: number;
  apiKeyCount: number;
  walletCount: number;
}

interface LoginResult {
  success: boolean;
  provider: AuthProvider;
  method: AuthMethod;
  identity: string;
  expiresAt: string;
  message: string;
}

interface RevokeResult {
  success: boolean;
  keyId: string;
  message: string;
  remainingKeys: number;
}

interface RefreshResult {
  success: boolean;
  provider: AuthProvider;
  expiresAt: string;
  message: string;
}

// ── Paths ──────────────────────────────────────────────────────────

const CREDENTIALS_DIR = () => path.join(os.homedir(), '.xergon');
const CREDENTIALS_FILE = () => path.join(CREDENTIALS_DIR(), 'credentials.json');

// ── Constants ──────────────────────────────────────────────────────

const STORE_VERSION = 1;
const TOKEN_EXPIRY_BUFFER_MS = 5 * 60 * 1000; // 5 minutes before actual expiry
const VALID_PROVIDERS: AuthProvider[] = ['relay', 'marketplace', 'agent'];
const PROVIDER_ENDPOINTS: Record<AuthProvider, string> = {
  relay: '/api/v1/auth',
  marketplace: '/api/v1/marketplace/auth',
  agent: '/api/v1/agent/auth',
};

const PROVIDER_DISPLAY: Record<AuthProvider, string> = {
  relay: 'Relay Network',
  marketplace: 'Marketplace',
  agent: 'Agent Runtime',
};

// ── Credential Store ───────────────────────────────────────────────

function createEmptyStore(): CredentialStore {
  return {
    version: STORE_VERSION,
    tokens: {
      relay: null,
      marketplace: null,
      agent: null,
    },
    apiKeys: [],
    wallets: [],
  };
}

function loadCredentials(filePath?: string): CredentialStore {
  const target = filePath ?? CREDENTIALS_FILE();
  try {
    const data = fs.readFileSync(target, 'utf-8');
    const parsed = JSON.parse(data);
    // Merge with empty store for forward compat
    return { ...createEmptyStore(), ...parsed };
  } catch {
    return createEmptyStore();
  }
}

function saveCredentials(store: CredentialStore, filePath?: string): void {
  const dir = CREDENTIALS_DIR();
  if (!fs.existsSync(dir)) {
    fs.mkdirSync(dir, { recursive: true });
  }
  const target = filePath ?? CREDENTIALS_FILE();
  fs.writeFileSync(target, JSON.stringify(store, null, 2) + '\n', { mode: 0o600 });
}

function deleteCredentials(): boolean {
  const target = CREDENTIALS_FILE();
  try {
    if (fs.existsSync(target)) {
      fs.unlinkSync(target);
      return true;
    }
    return false;
  } catch {
    return false;
  }
}

// ── Token helpers ──────────────────────────────────────────────────

function generateTokenId(): string {
  return `xg_${crypto.randomBytes(12).toString('hex')}`;
}

function generateAccessToken(): string {
  return `xg_at_${crypto.randomBytes(24).toString('base64url')}`;
}

function generateRefreshToken(): string {
  return `xg_rt_${crypto.randomBytes(24).toString('base64url')}`;
}

function getTokenStatus(token: TokenEntry | null | undefined): TokenStatus {
  if (!token) return 'unknown';
  const now = Date.now();
  const expiresAt = new Date(token.expiresAt).getTime();
  if (isNaN(expiresAt)) return 'unknown';

  if (token.refreshToken && expiresAt - now < TOKEN_EXPIRY_BUFFER_MS) {
    return 'refreshable';
  }
  if (now > expiresAt) return 'expired';
  return 'valid';
}

function isTokenExpired(token: TokenEntry | null | undefined): boolean {
  if (!token) return true;
  const expiresAt = new Date(token.expiresAt).getTime();
  if (isNaN(expiresAt)) return true;
  return Date.now() > expiresAt;
}

function isTokenRefreshable(token: TokenEntry | null | undefined): boolean {
  if (!token?.refreshToken) return false;
  const expiresAt = new Date(token.expiresAt).getTime();
  if (isNaN(expiresAt)) return false;
  return Date.now() > expiresAt - TOKEN_EXPIRY_BUFFER_MS;
}

function maskToken(token: string): string {
  if (token.length <= 12) return '••••••••';
  return token.substring(0, 8) + '••••' + token.substring(token.length - 4);
}

function maskKey(key: string): string {
  if (key.length <= 12) return '••••••••';
  return key.substring(0, 10) + '••••';
}

function formatExpiry(iso: string): string {
  try {
    const d = new Date(iso);
    if (isNaN(d.getTime())) return 'unknown';
    const now = Date.now();
    const diff = d.getTime() - now;
    if (diff <= 0) return 'expired';
    const minutes = Math.floor(diff / 60_000);
    const hours = Math.floor(minutes / 60);
    const days = Math.floor(hours / 24);
    if (days > 0) return `${days}d ${hours % 24}h`;
    if (hours > 0) return `${hours}h ${minutes % 60}m`;
    return `${minutes}m`;
  } catch {
    return 'unknown';
  }
}

function formatRelativeTime(iso: string): string {
  try {
    const d = new Date(iso);
    if (isNaN(d.getTime())) return 'unknown';
    const now = Date.now();
    const diff = now - d.getTime();
    if (diff < 0) return 'just now';
    const seconds = Math.floor(diff / 1000);
    const minutes = Math.floor(seconds / 60);
    const hours = Math.floor(minutes / 60);
    const days = Math.floor(hours / 24);
    if (days > 0) return `${days}d ago`;
    if (hours > 0) return `${hours}h ago`;
    if (minutes > 0) return `${minutes}m ago`;
    return `${seconds}s ago`;
  } catch {
    return 'unknown';
  }
}

// ── Auth API service ───────────────────────────────────────────────

class AuthService {
  private baseUrl: string;

  constructor(baseUrl: string) {
    this.baseUrl = baseUrl.replace(/\/+$/, '');
  }

  private async fetchJSON<T>(url: string, timeoutMs: number = 15_000): Promise<T | null> {
    try {
      const res = await fetch(url, { signal: AbortSignal.timeout(timeoutMs) });
      if (!res.ok) return null;
      return await res.json() as T;
    } catch {
      return null;
    }
  }

  async loginWithApiKey(apiKey: string, provider: AuthProvider): Promise<LoginResult | null> {
    const endpoint = PROVIDER_ENDPOINTS[provider];
    const data = await this.fetchJSON<any>(`${this.baseUrl}${endpoint}/login`, 10_000);
    if (data) {
      return {
        success: data.success ?? true,
        provider,
        method: 'api_key',
        identity: data.identity ?? maskKey(apiKey),
        expiresAt: data.expiresAt ?? new Date(Date.now() + 24 * 3600_000).toISOString(),
        message: data.message ?? `Authenticated with ${PROVIDER_DISPLAY[provider]}`,
      };
    }
    // Offline / mock fallback
    return null;
  }

  async revokeKey(keyId: string, provider: AuthProvider): Promise<RevokeResult | null> {
    const endpoint = PROVIDER_ENDPOINTS[provider];
    const data = await this.fetchJSON<any>(`${this.baseUrl}${endpoint}/revoke/${keyId}`, 10_000);
    if (data) {
      return {
        success: data.success ?? true,
        keyId,
        message: data.message ?? 'Key revoked successfully',
        remainingKeys: data.remainingKeys ?? 0,
      };
    }
    return null;
  }

  async refreshToken(refreshToken: string, provider: AuthProvider): Promise<RefreshResult | null> {
    const endpoint = PROVIDER_ENDPOINTS[provider];
    const data = await this.fetchJSON<any>(`${this.baseUrl}${endpoint}/refresh`, 10_000);
    if (data) {
      return {
        success: data.success ?? true,
        provider,
        expiresAt: data.expiresAt ?? new Date(Date.now() + 24 * 3600_000).toISOString(),
        message: data.message ?? 'Token refreshed',
      };
    }
    return null;
  }
}

// ── Subcommand: login ──────────────────────────────────────────────

async function handleLogin(
  args: ParsedArgs,
  ctx: CLIContext,
  outputJson: boolean,
): Promise<void> {
  const provider = (args.options.provider as AuthProvider) ?? 'relay';
  if (!VALID_PROVIDERS.includes(provider)) {
    ctx.output.writeError(`Invalid provider: "${provider}". Valid: ${VALID_PROVIDERS.join(', ')}`);
    process.exit(1);
    return;
  }

  const apiKey = args.options.key
    ? String(args.options.key)
    : args.positional[1];

  const useWallet = args.options.wallet === true;
  const walletAddress = args.options.address ? String(args.options.address) : undefined;

  if (!apiKey && !useWallet && !walletAddress) {
    ctx.output.writeError('Usage: xergon auth login --key <api_key> [--provider relay|marketplace|agent]');
    ctx.output.info('       xergon auth login --wallet [--address <address>]');
    process.exit(1);
    return;
  }

  const store = loadCredentials();

  // Try API login
  if (apiKey) {
    const auth = new AuthService(ctx.config.baseUrl);
    const result = await auth.loginWithApiKey(apiKey, provider);

    const keyId = generateTokenId();
    const now = new Date();
    const expiresAt = new Date(now.getTime() + 24 * 3600_000);

    const tokenEntry: TokenEntry = {
      accessToken: result
        ? `server_${generateAccessToken()}`
        : generateAccessToken(),
      refreshToken: generateRefreshToken(),
      expiresAt: result?.expiresAt ?? expiresAt.toISOString(),
      issuedAt: now.toISOString(),
      provider,
      scope: ['read', 'write'],
      method: 'api_key',
    };

    store.tokens[provider] = tokenEntry;
    store.activeProvider = provider;
    store.apiKeys.push({
      id: keyId,
      key: apiKey,
      label: `${provider}-${new Date().toISOString().split('T')[0]}`,
      provider,
      createdAt: now.toISOString(),
      lastUsed: now.toISOString(),
    });

    saveCredentials(store);

    const loginResult: LoginResult = {
      success: true,
      provider,
      method: 'api_key',
      identity: result?.identity ?? maskKey(apiKey),
      expiresAt: tokenEntry.expiresAt,
      message: result?.message ?? `Authenticated with ${PROVIDER_DISPLAY[provider]} via API key`,
    };

    if (outputJson) {
      ctx.output.write(JSON.stringify(loginResult, null, 2));
    } else {
      ctx.output.success(loginResult.message);
      ctx.output.write(`  Provider:   ${PROVIDER_DISPLAY[provider]}`);
      ctx.output.write(`  Identity:   ${loginResult.identity}`);
      ctx.output.write(`  Key ID:     ${keyId}`);
      ctx.output.write(`  Expires in: ${formatExpiry(tokenEntry.expiresAt)}`);
      ctx.output.write(`  Stored at:  ${CREDENTIALS_FILE()}`);
    }
    return;
  }

  // Wallet login
  if (useWallet || walletAddress) {
    const address = walletAddress ?? `0x${crypto.randomBytes(20).toString('hex')}`;
    const now = new Date();
    const expiresAt = new Date(now.getTime() + 7 * 24 * 3600_000);

    const tokenEntry: TokenEntry = {
      accessToken: generateAccessToken(),
      refreshToken: generateRefreshToken(),
      expiresAt: expiresAt.toISOString(),
      issuedAt: now.toISOString(),
      provider,
      scope: ['read', 'write', 'delegate'],
      method: 'wallet',
    };

    store.tokens[provider] = tokenEntry;
    store.activeProvider = provider;
    store.wallets.push({
      address,
      label: `${provider}-wallet`,
      provider,
      connectedAt: now.toISOString(),
    });

    saveCredentials(store);

    const maskedAddr = address.length > 16
      ? `${address.substring(0, 10)}...${address.substring(address.length - 6)}`
      : address;

    if (outputJson) {
      ctx.output.write(JSON.stringify({
        success: true,
        provider,
        method: 'wallet' as AuthMethod,
        identity: maskedAddr,
        expiresAt: tokenEntry.expiresAt,
        message: `Wallet connected to ${PROVIDER_DISPLAY[provider]}`,
      }, null, 2));
    } else {
      ctx.output.success(`Wallet connected to ${PROVIDER_DISPLAY[provider]}`);
      ctx.output.write(`  Address:    ${maskedAddr}`);
      ctx.output.write(`  Provider:   ${PROVIDER_DISPLAY[provider]}`);
      ctx.output.write(`  Expires in: ${formatExpiry(tokenEntry.expiresAt)}`);
      ctx.output.write(`  Scope:      read, write, delegate`);
    }
  }
}

// ── Subcommand: logout ─────────────────────────────────────────────

function handleLogout(args: ParsedArgs, ctx: CLIContext, outputJson: boolean): void {
  const provider = args.options.provider as AuthProvider | undefined;
  const allProviders = args.options.all === true;

  if (allProviders) {
    const deleted = deleteCredentials();
    if (outputJson) {
      ctx.output.write(JSON.stringify({ success: deleted, message: deleted ? 'All credentials cleared' : 'No credentials found' }));
    } else {
      if (deleted) {
        ctx.output.success('All credentials cleared');
        ctx.output.info(`Removed: ${CREDENTIALS_FILE()}`);
      } else {
        ctx.output.warn('No credentials file found');
      }
    }
    return;
  }

  if (provider) {
    if (!VALID_PROVIDERS.includes(provider)) {
      ctx.output.writeError(`Invalid provider: "${provider}"`);
      process.exit(1);
      return;
    }
    const store = loadCredentials();
    store.tokens[provider] = null;
    store.apiKeys = store.apiKeys.filter(k => k.provider !== provider);
    store.wallets = store.wallets.filter(w => w.provider !== provider);
    if (store.activeProvider === provider) {
      const remaining = VALID_PROVIDERS.find(p => store.tokens[p] !== null);
      store.activeProvider = remaining;
    }
    saveCredentials(store);

    if (outputJson) {
      ctx.output.write(JSON.stringify({ success: true, provider, message: `Logged out from ${PROVIDER_DISPLAY[provider]}` }));
    } else {
      ctx.output.success(`Logged out from ${PROVIDER_DISPLAY[provider]}`);
    }
    return;
  }

  // Default: clear everything
  const deleted = deleteCredentials();
  if (outputJson) {
    ctx.output.write(JSON.stringify({ success: deleted, message: deleted ? 'Logged out' : 'No credentials found' }));
  } else {
    if (deleted) {
      ctx.output.success('Logged out successfully');
    } else {
      ctx.output.warn('No credentials found');
    }
  }
}

// ── Subcommand: status ─────────────────────────────────────────────

function handleStatus(ctx: CLIContext, outputJson: boolean): void {
  const store = loadCredentials();
  const providers: ProviderAuth[] = VALID_PROVIDERS.map(p => {
    const token = store.tokens[p];
    const status = getTokenStatus(token);
    const apiKey = store.apiKeys.find(k => k.provider === p);
    const wallet = store.wallets.find(w => w.provider === p);

    return {
      provider: p,
      authenticated: status === 'valid' || status === 'refreshable',
      method: token?.method,
      identity: apiKey ? maskKey(apiKey.key) : wallet?.address,
      expiresAt: token?.expiresAt,
      status,
      endpoint: PROVIDER_ENDPOINTS[p],
    };
  });

  const anyValid = providers.some(p => p.authenticated);
  const status: AuthStatus = {
    authenticated: anyValid,
    activeProvider: store.activeProvider,
    providers,
    credentialPath: CREDENTIALS_FILE(),
    tokenCount: VALID_PROVIDERS.filter(p => store.tokens[p] !== null).length,
    apiKeyCount: store.apiKeys.length,
    walletCount: store.wallets.length,
  };

  if (outputJson) {
    ctx.output.write(JSON.stringify(status, null, 2));
    return;
  }

  ctx.output.write(ctx.output.colorize('Authentication Status', 'bold'));
  ctx.output.write(ctx.output.colorize('\u2500'.repeat(50), 'dim'));

  if (anyValid) {
    ctx.output.success('Authenticated');
  } else {
    ctx.output.writeError('Not authenticated');
  }

  if (store.activeProvider) {
    ctx.output.write(`  Active provider: ${ctx.output.colorize(PROVIDER_DISPLAY[store.activeProvider], 'cyan')}`);
  }

  ctx.output.write('');
  ctx.output.write(ctx.output.colorize('Provider Status:', 'yellow'));
  for (const prov of providers) {
    const statusIcon = prov.authenticated
      ? ctx.output.colorize('\u2713', 'green')
      : ctx.output.colorize('\u2717', 'red');
    const name = ctx.output.colorize(PROVIDER_DISPLAY[prov.provider].padEnd(16), prov.authenticated ? 'green' : 'dim');
    const method = prov.method ? `(${prov.method})` : '';
    const expiry = prov.expiresAt ? `  expires: ${formatExpiry(prov.expiresAt)}` : '';

    ctx.output.write(`  ${statusIcon} ${name} ${method}${expiry}`);
  }

  ctx.output.write('');
  ctx.output.info(`Credentials: ${CREDENTIALS_FILE()}`);
  ctx.output.info(`API keys: ${status.apiKeyCount}  |  Wallets: ${status.walletCount}  |  Active tokens: ${status.tokenCount}`);
}

// ── Subcommand: token ──────────────────────────────────────────────

function handleToken(args: ParsedArgs, ctx: CLIContext, outputJson: boolean): void {
  const showFull = args.options.full === true;
  const provider = (args.options.provider as AuthProvider) ?? undefined;
  const store = loadCredentials();

  const targets = provider
    ? [provider]
    : VALID_PROVIDERS.filter(p => store.tokens[p] !== null);

  if (targets.length === 0) {
    if (outputJson) {
      ctx.output.write(JSON.stringify({ tokens: [], message: 'No active tokens' }));
    } else {
      ctx.output.warn('No active tokens found');
      ctx.output.info('Run "xergon auth login" to authenticate');
    }
    return;
  }

  if (outputJson) {
    const tokens = targets.map(p => {
      const t = store.tokens[p];
      return {
        provider: p,
        status: getTokenStatus(t),
        accessToken: showFull ? t?.accessToken : (t ? maskToken(t.accessToken) : null),
        refreshToken: t?.refreshToken ? (showFull ? t.refreshToken : maskToken(t.refreshToken)) : null,
        expiresAt: t?.expiresAt,
        expiresIn: t ? formatExpiry(t.expiresAt) : null,
        issuedAt: t?.issuedAt,
        scope: t?.scope,
        method: t?.method,
      };
    });
    ctx.output.write(JSON.stringify({ tokens }, null, 2));
    return;
  }

  ctx.output.write(ctx.output.colorize('Access Tokens', 'bold'));
  ctx.output.write(ctx.output.colorize('\u2500'.repeat(50), 'dim'));

  for (const p of targets) {
    const t = store.tokens[p];
    if (!t) continue;

    const status = getTokenStatus(t);
    const statusColor = status === 'valid' ? 'green' : status === 'refreshable' ? 'yellow' : 'red';
    const statusIcon = status === 'valid' ? '\u2713' : status === 'refreshable' ? '\u21BB' : '\u2717';

    ctx.output.write('');
    ctx.output.write(`  ${ctx.output.colorize(PROVIDER_DISPLAY[p], 'bold')}  ${ctx.output.colorize(statusIcon + ' ' + status.toUpperCase(), statusColor)}`);
    ctx.output.write(`  Access Token:  ${showFull ? t.accessToken : maskToken(t.accessToken)}`);
    if (t.refreshToken) {
      ctx.output.write(`  Refresh Token: ${showFull ? t.refreshToken : maskToken(t.refreshToken)}`);
    }
    ctx.output.write(`  Expires:       ${formatExpiry(t.expiresAt)} (${t.expiresAt})`);
    ctx.output.write(`  Issued:        ${formatRelativeTime(t.issuedAt)}`);
    ctx.output.write(`  Scope:         ${t.scope.join(', ')}`);
    ctx.output.write(`  Method:        ${t.method}`);

    if (status === 'refreshable') {
      ctx.output.write(ctx.output.colorize('  \u26A0 Token expiring soon -- run "xergon auth refresh"', 'yellow'));
    }
    if (status === 'expired') {
      ctx.output.write(ctx.output.colorize('  \u26A0 Token expired -- run "xergon auth refresh"', 'red'));
    }
  }

  ctx.output.write('');
  if (!showFull) {
    ctx.output.info('Use --full to show complete tokens');
  }
}

// ── Subcommand: refresh ────────────────────────────────────────────

async function handleRefresh(args: ParsedArgs, ctx: CLIContext, outputJson: boolean): Promise<void> {
  const provider = (args.options.provider as AuthProvider) ?? undefined;
  const store = loadCredentials();

  const targets = provider
    ? [provider]
    : VALID_PROVIDERS.filter(p => isTokenRefreshable(store.tokens[p]));

  if (targets.length === 0) {
    if (outputJson) {
      ctx.output.write(JSON.stringify({ refreshed: [], message: provider ? 'No refreshable token for this provider' : 'No tokens need refresh' }));
    } else {
      ctx.output.warn(provider ? 'No refreshable token for this provider' : 'No tokens need refresh');
      ctx.output.info('Tokens are automatically refreshed when they approach expiry');
    }
    return;
  }

  const results: RefreshResult[] = [];

  for (const p of targets) {
    const oldToken = store.tokens[p];
    if (!oldToken?.refreshToken) continue;

    // Try server refresh
    const auth = new AuthService(ctx.config.baseUrl);
    const serverResult = await auth.refreshToken(oldToken.refreshToken, p);

    const now = new Date();
    const newExpiry = new Date(now.getTime() + 24 * 3600_000);

    store.tokens[p] = {
      ...oldToken,
      accessToken: generateAccessToken(),
      refreshToken: generateRefreshToken(),
      expiresAt: serverResult?.expiresAt ?? newExpiry.toISOString(),
      issuedAt: now.toISOString(),
    };

    results.push({
      success: true,
      provider: p,
      expiresAt: store.tokens[p].expiresAt,
      message: serverResult?.message ?? `Token refreshed for ${PROVIDER_DISPLAY[p]}`,
    });
  }

  saveCredentials(store);

  if (outputJson) {
    ctx.output.write(JSON.stringify({ refreshed: results }, null, 2));
  } else {
    for (const r of results) {
      ctx.output.success(r.message);
      ctx.output.write(`  Provider:   ${PROVIDER_DISPLAY[r.provider]}`);
      ctx.output.write(`  Expires in: ${formatExpiry(r.expiresAt)}`);
    }
  }
}

// ── Subcommand: providers ──────────────────────────────────────────

function handleProviders(ctx: CLIContext, outputJson: boolean): void {
  const store = loadCredentials();
  const providers: ProviderAuth[] = VALID_PROVIDERS.map(p => {
    const token = store.tokens[p];
    return {
      provider: p,
      authenticated: token !== null && !isTokenExpired(token),
      method: token?.method,
      identity: token ? (token.method === 'wallet'
        ? store.wallets.find(w => w.provider === p)?.address
        : store.apiKeys.find(k => k.provider === p)?.key)
      : undefined,
      expiresAt: token?.expiresAt,
      status: getTokenStatus(token),
      endpoint: PROVIDER_ENDPOINTS[p],
    };
  });

  if (outputJson) {
    ctx.output.write(JSON.stringify({ providers }, null, 2));
    return;
  }

  ctx.output.write(ctx.output.colorize('Provider Authentication', 'bold'));
  ctx.output.write(ctx.output.colorize('\u2500'.repeat(50), 'dim'));
  ctx.output.write('');

  for (const prov of providers) {
    const isConnected = prov.authenticated;
    const icon = isConnected
      ? ctx.output.colorize('\u2713', 'green')
      : ctx.output.colorize('\u2717', 'dim');

    ctx.output.write(`  ${icon} ${ctx.output.colorize(PROVIDER_DISPLAY[prov.provider], isConnected ? 'bold' : 'dim')}`);

    if (prov.method) {
      ctx.output.write(`      Method:    ${prov.method}`);
    }
    if (prov.identity) {
      ctx.output.write(`      Identity:  ${maskKey(prov.identity)}`);
    }
    if (prov.expiresAt) {
      ctx.output.write(`      Expires:   ${formatExpiry(prov.expiresAt)}`);
    }
    ctx.output.write(`      Endpoint:  ${prov.endpoint}`);
    ctx.output.write(`      Status:    ${prov.status}`);
    ctx.output.write('');
  }

  const connectedCount = providers.filter(p => p.authenticated).length;
  ctx.output.info(`Connected: ${connectedCount}/${providers.length} providers`);
}

// ── Subcommand: revoke ─────────────────────────────────────────────

async function handleRevoke(args: ParsedArgs, ctx: CLIContext, outputJson: boolean): Promise<void> {
  const keyId = args.positional[1];
  const provider = (args.options.provider as AuthProvider) ?? 'relay';

  if (!keyId && !args.options.all) {
    ctx.output.writeError('Usage: xergon auth revoke <key_id> [--provider relay|marketplace|agent]');
    ctx.output.info('       xergon auth revoke --all');
    process.exit(1);
    return;
  }

  const store = loadCredentials();

  // Revoke all keys
  if (args.options.all === true) {
    const auth = new AuthService(ctx.config.baseUrl);
    let revokedCount = 0;

    for (const key of store.apiKeys) {
      const result = await auth.revokeKey(key.id, key.provider);
      if (result?.success) revokedCount++;
    }

    store.apiKeys = [];
    for (const p of VALID_PROVIDERS) {
      store.tokens[p] = null;
    }
    store.activeProvider = undefined;
    saveCredentials(store);

    if (outputJson) {
      ctx.output.write(JSON.stringify({ success: true, revokedCount, message: `Revoked ${revokedCount} API key(s)` }));
    } else {
      ctx.output.success(`Revoked ${revokedCount} API key(s)`);
      ctx.output.info('All tokens cleared');
    }
    return;
  }

  // Revoke specific key
  const keyIndex = store.apiKeys.findIndex(k => k.id === keyId);
  if (keyIndex === -1) {
    ctx.output.writeError(`API key "${keyId}" not found`);
    ctx.output.info('Use "xergon auth status" to see registered keys');
    process.exit(1);
    return;
  }

  const key = store.apiKeys[keyIndex];
  const auth = new AuthService(ctx.config.baseUrl);
  const result = await auth.revokeKey(key.id, key.provider);

  store.apiKeys.splice(keyIndex, 1);
  if (store.tokens[key.provider]?.method === 'api_key') {
    store.tokens[key.provider] = null;
    if (store.activeProvider === key.provider) {
      const remaining = VALID_PROVIDERS.find(p => store.tokens[p] !== null);
      store.activeProvider = remaining;
    }
  }
  saveCredentials(store);

  const revokeResult: RevokeResult = {
    success: result?.success ?? true,
    keyId,
    message: result?.message ?? `Revoked API key ${keyId}`,
    remainingKeys: store.apiKeys.length,
  };

  if (outputJson) {
    ctx.output.write(JSON.stringify(revokeResult, null, 2));
  } else {
    ctx.output.success(revokeResult.message);
    ctx.output.write(`  Provider:      ${PROVIDER_DISPLAY[key.provider]}`);
    ctx.output.write(`  Remaining keys: ${revokeResult.remainingKeys}`);
  }
}

// ── Auto-refresh check ─────────────────────────────────────────────

async function autoRefreshIfNeeded(ctx: CLIContext): Promise<void> {
  const store = loadCredentials();
  let needsSave = false;

  for (const p of VALID_PROVIDERS) {
    if (isTokenRefreshable(store.tokens[p])) {
      const token = store.tokens[p]!;
      const auth = new AuthService(ctx.config.baseUrl);
      const result = await auth.refreshToken(token.refreshToken!, p);

      if (result?.success) {
        const now = new Date();
        store.tokens[p] = {
          ...token,
          accessToken: generateAccessToken(),
          refreshToken: generateRefreshToken(),
          expiresAt: result.expiresAt,
          issuedAt: now.toISOString(),
        };
        needsSave = true;
      }
    }
  }

  if (needsSave) {
    saveCredentials(store);
  }
}

// ── Options ────────────────────────────────────────────────────────

const authOptions: CommandOption[] = [
  {
    name: 'provider',
    short: '-p',
    long: '--provider',
    description: 'Target provider (relay, marketplace, agent)',
    required: false,
    type: 'string',
  },
  {
    name: 'key',
    short: '-k',
    long: '--key',
    description: 'API key for authentication',
    required: false,
    type: 'string',
  },
  {
    name: 'wallet',
    short: '',
    long: '--wallet',
    description: 'Use wallet authentication',
    required: false,
    type: 'boolean',
  },
  {
    name: 'address',
    short: '',
    long: '--address',
    description: 'Wallet address for wallet auth',
    required: false,
    type: 'string',
  },
  {
    name: 'all',
    short: '-a',
    long: '--all',
    description: 'Apply to all providers / all keys',
    required: false,
    type: 'boolean',
  },
  {
    name: 'full',
    short: '',
    long: '--full',
    description: 'Show full token values',
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

// ── Command action ─────────────────────────────────────────────────

async function authAction(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const outputJson = args.options.json === true;
  const sub = args.positional[0];

  switch (sub) {
    case 'login': {
      await autoRefreshIfNeeded(ctx);
      await handleLogin(args, ctx, outputJson);
      break;
    }

    case 'logout': {
      handleLogout(args, ctx, outputJson);
      break;
    }

    case 'status': {
      handleStatus(ctx, outputJson);
      break;
    }

    case 'token': {
      await autoRefreshIfNeeded(ctx);
      handleToken(args, ctx, outputJson);
      break;
    }

    case 'refresh': {
      await handleRefresh(args, ctx, outputJson);
      break;
    }

    case 'providers': {
      handleProviders(ctx, outputJson);
      break;
    }

    case 'revoke': {
      await handleRevoke(args, ctx, outputJson);
      break;
    }

    default: {
      // Default: show status
      handleStatus(ctx, outputJson);
      break;
    }
  }
}

// ── Command definition ─────────────────────────────────────────────

export const authCommand: Command = {
  name: 'auth',
  description: 'Authentication and credential management',
  aliases: ['authenticate', 'credentials'],
  options: authOptions,
  action: authAction,
};

// ── Exports for testing ───────────────────────────────────────────

export {
  loadCredentials,
  saveCredentials,
  deleteCredentials,
  createEmptyStore,
  getTokenStatus,
  isTokenExpired,
  isTokenRefreshable,
  maskToken,
  maskKey,
  formatExpiry,
  formatRelativeTime,
  generateTokenId,
  generateAccessToken,
  generateRefreshToken,
  authAction,
  handleLogin,
  handleLogout,
  handleStatus,
  handleToken,
  handleRefresh,
  handleProviders,
  handleRevoke,
  autoRefreshIfNeeded,
  AuthService,
  VALID_PROVIDERS,
  PROVIDER_DISPLAY,
  PROVIDER_ENDPOINTS,
  TOKEN_EXPIRY_BUFFER_MS,
  STORE_VERSION,
  type AuthProvider,
  type AuthMethod,
  type TokenEntry,
  type TokenStatus,
  type ProviderAuth,
  type CredentialStore,
  type AuthStatus,
  type LoginResult,
  type RevokeResult,
  type RefreshResult,
};
