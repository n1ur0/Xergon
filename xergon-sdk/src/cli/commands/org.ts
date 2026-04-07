/**
 * CLI command: org
 *
 * Organization management for the Xergon Network.
 *
 * Usage:
 *   xergon org create <name>           -- Create organization
 *   xergon org info                   -- Show organization info
 *   xergon org members                -- List members
 *   xergon org invite <email>         -- Invite member
 *   xergon org remove <email>         -- Remove member
 *   xergon org keys                   -- Manage API keys with scopes
 *   xergon org keys create            -- Create scoped API key
 *   xergon org keys revoke <id>       -- Revoke API key
 *   xergon org settings               -- View/update org settings
 */

import type { Command, CommandOption, ParsedArgs, CLIContext } from '../mod';
import * as crypto from 'node:crypto';

// ── Types ──────────────────────────────────────────────────────────

type OrgRole = 'Admin' | 'Member' | 'Viewer';
type KeyScope = 'read' | 'write' | 'admin' | 'inference' | 'deploy';
type KeyStatus = 'active' | 'revoked' | 'expired';
type MemberStatus = 'active' | 'pending' | 'suspended';

interface OrgInfo {
  id: string;
  name: string;
  slug: string;
  plan: 'free' | 'pro' | 'enterprise';
  memberCount: number;
  keyCount: number;
  createdAt: string;
  updatedAt: string;
  owner: string;
  settings: OrgSettings;
}

interface OrgSettings {
  defaultRole: OrgRole;
  allowSelfInvite: boolean;
  require2FA: boolean;
  webhookUrl: string | null;
  billingEmail: string;
  maxKeys: number;
  maxMembers: number;
}

interface OrgMember {
  id: string;
  email: string;
  name: string;
  role: OrgRole;
  status: MemberStatus;
  joinedAt: string;
  lastActive: string;
  invitedBy: string;
}

interface OrgApiKey {
  id: string;
  name: string;
  key: string;
  scopes: KeyScope[];
  status: KeyStatus;
  createdAt: string;
  expiresAt: string | null;
  lastUsed: string | null;
  createdBy: string;
  requestCount: number;
}

interface InviteResult {
  success: boolean;
  email: string;
  role: OrgRole;
  inviteUrl: string;
  expiresAt: string;
  message: string;
}

interface RemoveResult {
  success: boolean;
  email: string;
  message: string;
}

interface CreateKeyResult {
  success: boolean;
  id: string;
  name: string;
  key: string;
  scopes: KeyScope[];
  expiresAt: string | null;
  message: string;
}

interface RevokeKeyResult {
  success: boolean;
  id: string;
  message: string;
}

interface CreateOrgResult {
  success: boolean;
  id: string;
  name: string;
  slug: string;
  owner: string;
  message: string;
}

// ── Constants ──────────────────────────────────────────────────────

const VALID_ROLES: OrgRole[] = ['Admin', 'Member', 'Viewer'];
const VALID_SCOPES: KeyScope[] = ['read', 'write', 'admin', 'inference', 'deploy'];
const ROLE_PERMISSIONS: Record<OrgRole, string[]> = {
  Admin: ['manage_members', 'manage_keys', 'manage_settings', 'billing', 'delete_org'],
  Member: ['create_keys', 'view_members', 'use_inference', 'deploy_models'],
  Viewer: ['view_members', 'view_settings', 'view_usage'],
};
const ROLE_DISPLAY: Record<OrgRole, string> = {
  Admin: 'Admin (full access)',
  Member: 'Member (standard access)',
  Viewer: 'Viewer (read-only)',
};
const SCOPE_DESCRIPTIONS: Record<KeyScope, string> = {
  read: 'Read organization data and resources',
  write: 'Write and modify organization resources',
  admin: 'Full administrative access',
  inference: 'Run model inference requests',
  deploy: 'Deploy models to the network',
};

// ── Helpers ────────────────────────────────────────────────────────

function generateOrgId(): string {
  return `org_${crypto.randomBytes(12).toString('hex')}`;
}

function generateKeyId(): string {
  return `xgk_${crypto.randomBytes(16).toString('hex')}`;
}

function generateApiKey(): string {
  return `xg_live_${crypto.randomBytes(32).toString('base64url')}`;
}

function generateInviteToken(): string {
  return crypto.randomBytes(24).toString('base64url');
}

function generateSlug(name: string): string {
  return name
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-|-$/g, '')
    .slice(0, 48);
}

function validateEmail(email: string): boolean {
  return /^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(email);
}

function validateRole(role: string): role is OrgRole {
  return (VALID_ROLES as string[]).includes(role);
}

function validateScope(scope: string): scope is KeyScope {
  return (VALID_SCOPES as string[]).includes(scope);
}

function maskApiKey(key: string): string {
  if (key.length <= 16) return '••••••••••••••••';
  return key.substring(0, 12) + '••••' + key.substring(key.length - 4);
}

function formatRoleBadge(role: OrgRole): string {
  const badges: Record<OrgRole, string> = {
    Admin: '[ADMIN]',
    Member: '[MEMBER]',
    Viewer: '[VIEWER]',
  };
  return badges[role];
}

function formatScopeList(scopes: KeyScope[]): string {
  return scopes.map(s => s.toUpperCase()).join(', ');
}

function formatExpiry(iso: string | null): string {
  if (!iso) return 'never';
  try {
    const d = new Date(iso);
    if (isNaN(d.getTime())) return 'never';
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
    return 'never';
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

function parseScopes(scopeStr: string): KeyScope[] {
  return scopeStr
    .split(',')
    .map(s => s.trim().toLowerCase())
    .filter((s): s is KeyScope => validateScope(s));
}

function isJsonOutput(args: ParsedArgs): boolean {
  return args.options.json === true || args.options.j === true;
}

// ── Mock data (for local/offline mode) ─────────────────────────────

function getMockOrg(): OrgInfo {
  return {
    id: 'org_a1b2c3d4e5f6',
    name: 'Acme Corp',
    slug: 'acme-corp',
    plan: 'pro',
    memberCount: 5,
    keyCount: 3,
    createdAt: '2025-01-15T10:30:00Z',
    updatedAt: '2025-06-01T14:20:00Z',
    owner: 'admin@acme.com',
    settings: {
      defaultRole: 'Member',
      allowSelfInvite: false,
      require2FA: true,
      webhookUrl: 'https://hooks.acme.com/xergon',
      billingEmail: 'billing@acme.com',
      maxKeys: 50,
      maxMembers: 100,
    },
  };
}

function getMockMembers(): OrgMember[] {
  return [
    {
      id: 'mem_001',
      email: 'admin@acme.com',
      name: 'Alice Admin',
      role: 'Admin',
      status: 'active',
      joinedAt: '2025-01-15T10:30:00Z',
      lastActive: '2025-06-01T14:20:00Z',
      invitedBy: 'self',
    },
    {
      id: 'mem_002',
      email: 'bob@acme.com',
      name: 'Bob Builder',
      role: 'Member',
      status: 'active',
      joinedAt: '2025-02-10T09:00:00Z',
      lastActive: '2025-05-30T16:45:00Z',
      invitedBy: 'admin@acme.com',
    },
    {
      id: 'mem_003',
      email: 'carol@acme.com',
      name: 'Carol Dev',
      role: 'Member',
      status: 'active',
      joinedAt: '2025-03-01T11:15:00Z',
      lastActive: '2025-05-28T10:00:00Z',
      invitedBy: 'admin@acme.com',
    },
    {
      id: 'mem_004',
      email: 'dave@external.com',
      name: 'Dave Viewer',
      role: 'Viewer',
      status: 'pending',
      joinedAt: '2025-05-25T08:00:00Z',
      lastActive: 'never',
      invitedBy: 'admin@acme.com',
    },
    {
      id: 'mem_005',
      email: 'eve@contractor.com',
      name: 'Eve Contractor',
      role: 'Member',
      status: 'suspended',
      joinedAt: '2025-04-01T12:00:00Z',
      lastActive: '2025-04-15T09:30:00Z',
      invitedBy: 'bob@acme.com',
    },
  ];
}

function getMockKeys(): OrgApiKey[] {
  return [
    {
      id: 'xgk_abcdef123456',
      name: 'Production API Key',
      key: 'xg_live_abc123def456ghi789jkl012mno345pqr678stu901vwx234yz567',
      scopes: ['read', 'write', 'inference'],
      status: 'active',
      createdAt: '2025-01-20T10:00:00Z',
      expiresAt: null,
      lastUsed: '2025-06-01T14:15:00Z',
      createdBy: 'admin@acme.com',
      requestCount: 15420,
    },
    {
      id: 'xgk_fedcba654321',
      name: 'Deploy Key',
      key: 'xg_live_xyz987wvu654tsr321qpo098nml765kjih432gfe101dcb234a567',
      scopes: ['deploy', 'read'],
      status: 'active',
      createdAt: '2025-02-15T08:00:00Z',
      expiresAt: '2026-02-15T08:00:00Z',
      lastUsed: '2025-05-30T11:00:00Z',
      createdBy: 'bob@acme.com',
      requestCount: 328,
    },
    {
      id: 'xgk_111111222222',
      name: 'Old Test Key',
      key: 'xg_live_000000000000000000000000000000000000000000000000000000',
      scopes: ['read'],
      status: 'revoked',
      createdAt: '2025-01-10T09:00:00Z',
      expiresAt: '2025-04-10T09:00:00Z',
      lastUsed: '2025-03-01T12:00:00Z',
      createdBy: 'admin@acme.com',
      requestCount: 42,
    },
  ];
}

// ── Options ────────────────────────────────────────────────────────

const orgOptions: CommandOption[] = [
  {
    name: 'role',
    short: '',
    long: '--role',
    description: 'Member role (Admin, Member, Viewer)',
    required: false,
    default: 'Member',
    type: 'string',
  },
  {
    name: 'scopes',
    short: '',
    long: '--scopes',
    description: 'API key scopes (comma-separated: read,write,admin,inference,deploy)',
    required: false,
    default: 'read',
    type: 'string',
  },
  {
    name: 'name',
    short: '',
    long: '--name',
    description: 'API key name',
    required: false,
    type: 'string',
  },
  {
    name: 'expires',
    short: '',
    long: '--expires',
    description: 'Key expiration (e.g., 30d, 90d, 1y)',
    required: false,
    type: 'string',
  },
  {
    name: 'force',
    short: '',
    long: '--force',
    description: 'Skip confirmation prompt',
    required: false,
    type: 'boolean',
  },
  {
    name: 'setting',
    short: '',
    long: '--setting',
    description: 'Setting key to update (e.g., defaultRole, require2FA)',
    required: false,
    type: 'string',
  },
  {
    name: 'value',
    short: '',
    long: '--value',
    description: 'Setting value',
    required: false,
    type: 'string',
  },
];

// ── Subcommand handlers ────────────────────────────────────────────

async function handleCreate(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const name = args.positional[1];
  const outputJson = isJsonOutput(args);

  if (!name) {
    ctx.output.writeError('Organization name required. Use: xergon org create <name>');
    process.exit(1);
    return;
  }

  if (name.length < 2) {
    ctx.output.writeError('Organization name must be at least 2 characters.');
    process.exit(1);
    return;
  }

  if (name.length > 64) {
    ctx.output.writeError('Organization name must be at most 64 characters.');
    process.exit(1);
    return;
  }

  const slug = generateSlug(name);
  const orgId = generateOrgId();

  try {
    if (ctx.client) {
      const resp = await ctx.client.post('/api/v1/orgs', { name, slug });
      const org = resp as any;

      if (outputJson) {
        ctx.output.write(JSON.stringify(org, null, 2));
        return;
      }

      ctx.output.success('Organization created');
      ctx.output.write(`  ID:    ${org.id}`);
      ctx.output.write(`  Name:  ${org.name}`);
      ctx.output.write(`  Slug:  ${org.slug}`);
      ctx.output.write(`  Owner: ${org.owner}`);
    } else {
      const result: CreateOrgResult = {
        success: true,
        id: orgId,
        name,
        slug,
        owner: 'you',
        message: `Organization "${name}" created successfully.`,
      };

      if (outputJson) {
        ctx.output.write(JSON.stringify(result, null, 2));
        return;
      }

      ctx.output.success('Organization created');
      ctx.output.write(`  ID:    ${result.id}`);
      ctx.output.write(`  Name:  ${result.name}`);
      ctx.output.write(`  Slug:  ${result.slug}`);
      ctx.output.write(`  Owner: ${result.owner}`);
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to create organization: ${message}`);
    process.exit(1);
  }
}

async function handleInfo(_args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const outputJson = isJsonOutput(_args);

  try {
    let org: OrgInfo;
    if (ctx.client) {
      const resp = await ctx.client.get('/api/v1/org');
      org = resp as any;
    } else {
      org = getMockOrg();
    }

    if (outputJson) {
      ctx.output.write(JSON.stringify(org, null, 2));
      return;
    }

    ctx.output.write(`${ctx.output.colorize('Organization', 'bold')}: ${org.name}`);
    ctx.output.write(`  ID:           ${org.id}`);
    ctx.output.write(`  Slug:         ${org.slug}`);
    ctx.output.write(`  Plan:         ${ctx.output.colorize(org.plan.toUpperCase(), 'cyan')}`);
    ctx.output.write(`  Owner:        ${org.owner}`);
    ctx.output.write(`  Members:      ${org.memberCount}`);
    ctx.output.write(`  API Keys:     ${org.keyCount}`);
    ctx.output.write(`  Created:      ${org.createdAt}`);
    ctx.output.write(`  Updated:      ${formatRelativeTime(org.updatedAt)}`);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to get organization info: ${message}`);
    process.exit(1);
  }
}

async function handleMembers(_args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const outputJson = isJsonOutput(_args);

  try {
    let members: OrgMember[];
    if (ctx.client) {
      const resp = await ctx.client.get('/api/v1/org/members');
      members = (resp as any).members ?? resp;
    } else {
      members = getMockMembers();
    }

    if (outputJson) {
      ctx.output.write(JSON.stringify(members, null, 2));
      return;
    }

    if (members.length === 0) {
      ctx.output.info('No members found.');
      return;
    }

    const tableData = members.map(m => ({
      Email: m.email,
      Name: m.name,
      Role: formatRoleBadge(m.role),
      Status: m.status === 'active'
        ? ctx.output.colorize('active', 'green')
        : m.status === 'pending'
          ? ctx.output.colorize('pending', 'yellow')
          : ctx.output.colorize('suspended', 'red'),
      Joined: m.joinedAt.split('T')[0],
      'Last Active': m.lastActive === 'never' ? 'never' : formatRelativeTime(m.lastActive),
    }));

    ctx.output.write(ctx.output.formatTable(tableData, `Members (${members.length})`));
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to list members: ${message}`);
    process.exit(1);
  }
}

async function handleInvite(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const email = args.positional[1];
  const roleStr = (args.options.role as string) || 'Member';
  const outputJson = isJsonOutput(args);

  if (!email) {
    ctx.output.writeError('Email required. Use: xergon org invite <email> [--role Role]');
    process.exit(1);
    return;
  }

  if (!validateEmail(email)) {
    ctx.output.writeError(`Invalid email address: ${email}`);
    process.exit(1);
    return;
  }

  if (!validateRole(roleStr)) {
    ctx.output.writeError(`Invalid role: ${roleStr}. Must be one of: ${VALID_ROLES.join(', ')}`);
    process.exit(1);
    return;
  }

  const role = roleStr as OrgRole;
  const token = generateInviteToken();
  const expiresAt = new Date(Date.now() + 7 * 86400_000).toISOString();

  try {
    if (ctx.client) {
      const resp = await ctx.client.post('/api/v1/org/members/invite', { email, role });
      const result = resp as any;

      if (outputJson) {
        ctx.output.write(JSON.stringify(result, null, 2));
        return;
      }

      ctx.output.success(`Invitation sent to ${email}`);
      ctx.output.write(`  Role:      ${formatRoleBadge(role)}`);
      ctx.output.write(`  Expires:   ${formatExpiry(expiresAt)}`);
    } else {
      const result: InviteResult = {
        success: true,
        email,
        role,
        inviteUrl: `https://relay.xergon.gg/invite/${token}`,
        expiresAt,
        message: `Invitation sent to ${email} with role ${role}.`,
      };

      if (outputJson) {
        ctx.output.write(JSON.stringify(result, null, 2));
        return;
      }

      ctx.output.success(`Invitation sent to ${email}`);
      ctx.output.write(`  Role:      ${formatRoleBadge(role)}`);
      ctx.output.write(`  Expires:   ${formatExpiry(expiresAt)}`);
      ctx.output.write(`  URL:       ${result.inviteUrl}`);
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to invite member: ${message}`);
    process.exit(1);
  }
}

async function handleRemove(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const email = args.positional[1];
  const force = args.options.force as boolean | undefined;
  const outputJson = isJsonOutput(args);

  if (!email) {
    ctx.output.writeError('Email required. Use: xergon org remove <email>');
    process.exit(1);
    return;
  }

  if (!validateEmail(email)) {
    ctx.output.writeError(`Invalid email address: ${email}`);
    process.exit(1);
    return;
  }

  if (!force) {
    ctx.output.writeError(`This will remove ${email} from the organization.`);
    ctx.output.write('Use --force to confirm removal.');
    process.exit(1);
    return;
  }

  try {
    if (ctx.client) {
      await ctx.client.delete(`/api/v1/org/members/${encodeURIComponent(email)}`);
    }

    const result: RemoveResult = {
      success: true,
      email,
      message: `${email} has been removed from the organization.`,
    };

    if (outputJson) {
      ctx.output.write(JSON.stringify(result, null, 2));
      return;
    }

    ctx.output.success(`Removed ${email} from the organization.`);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to remove member: ${message}`);
    process.exit(1);
  }
}

async function handleKeys(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const sub = args.positional[1];

  if (!sub) {
    await handleKeysList(args, ctx);
    return;
  }

  switch (sub) {
    case 'list':
      await handleKeysList(args, ctx);
      break;
    case 'create':
      await handleKeysCreate(args, ctx);
      break;
    case 'revoke':
      await handleKeysRevoke(args, ctx);
      break;
    default:
      ctx.output.writeError(`Unknown keys subcommand: ${sub}`);
      ctx.output.write('Usage: xergon org keys [list|create|revoke] [args]');
      process.exit(1);
  }
}

async function handleKeysList(_args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const outputJson = isJsonOutput(_args);

  try {
    let keys: OrgApiKey[];
    if (ctx.client) {
      const resp = await ctx.client.get('/api/v1/org/keys');
      keys = (resp as any).keys ?? resp;
    } else {
      keys = getMockKeys();
    }

    if (outputJson) {
      ctx.output.write(JSON.stringify(keys, null, 2));
      return;
    }

    if (keys.length === 0) {
      ctx.output.info('No API keys found. Create one with: xergon org keys create');
      return;
    }

    const tableData = keys.map(k => ({
      ID: k.id.length > 16 ? k.id.slice(0, 16) + '...' : k.id,
      Name: k.name,
      Key: maskApiKey(k.key),
      Scopes: formatScopeList(k.scopes),
      Status: k.status === 'active'
        ? ctx.output.colorize('active', 'green')
        : k.status === 'revoked'
          ? ctx.output.colorize('revoked', 'red')
          : ctx.output.colorize('expired', 'yellow'),
      Expires: formatExpiry(k.expiresAt),
      Requests: k.requestCount.toLocaleString(),
    }));

    ctx.output.write(ctx.output.formatTable(tableData, `API Keys (${keys.length})`));
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to list API keys: ${message}`);
    process.exit(1);
  }
}

async function handleKeysCreate(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const name = (args.options.name as string) || 'Unnamed Key';
  const scopesStr = (args.options.scopes as string) || 'read';
  const expiresStr = (args.options.expires as string) || undefined;
  const outputJson = isJsonOutput(args);

  const scopes = parseScopes(scopesStr);
  if (scopes.length === 0) {
    ctx.output.writeError(`No valid scopes provided. Available: ${VALID_SCOPES.join(', ')}`);
    process.exit(1);
    return;
  }

  let expiresAt: string | null = null;
  if (expiresStr) {
    const match = expiresStr.match(/^(\d+)(d|y)$/);
    if (!match) {
      ctx.output.writeError('Invalid expiration format. Use e.g., 30d, 90d, 1y');
      process.exit(1);
      return;
    }
    const amount = parseInt(match[1], 10);
    const unit = match[2];
    const ms = unit === 'd' ? amount * 86400_000 : amount * 365 * 86400_000;
    expiresAt = new Date(Date.now() + ms).toISOString();
  }

  const keyId = generateKeyId();
  const apiKey = generateApiKey();

  try {
    if (ctx.client) {
      const resp = await ctx.client.post('/api/v1/org/keys', {
        name,
        scopes,
        expiresAt,
      });
      const result = resp as any;

      if (outputJson) {
        ctx.output.write(JSON.stringify(result, null, 2));
        return;
      }

      ctx.output.success('API key created');
      ctx.output.write(`  ID:      ${result.id}`);
      ctx.output.write(`  Name:    ${result.name}`);
      ctx.output.write(`  Key:     ${result.key}`);
      ctx.output.write(`  Scopes:  ${formatScopeList(result.scopes)}`);
      ctx.output.write(`  Expires: ${formatExpiry(result.expiresAt)}`);
      ctx.output.info('Keep this key safe -- it cannot be shown again.');
    } else {
      const result: CreateKeyResult = {
        success: true,
        id: keyId,
        name,
        key: apiKey,
        scopes,
        expiresAt,
        message: `API key "${name}" created successfully.`,
      };

      if (outputJson) {
        ctx.output.write(JSON.stringify(result, null, 2));
        return;
      }

      ctx.output.success('API key created');
      ctx.output.write(`  ID:      ${result.id}`);
      ctx.output.write(`  Name:    ${result.name}`);
      ctx.output.write(`  Key:     ${result.key}`);
      ctx.output.write(`  Scopes:  ${formatScopeList(result.scopes)}`);
      ctx.output.write(`  Expires: ${formatExpiry(result.expiresAt)}`);
      ctx.output.info('Keep this key safe -- it cannot be shown again.');
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to create API key: ${message}`);
    process.exit(1);
  }
}

async function handleKeysRevoke(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const keyId = args.positional[2];
  const force = args.options.force as boolean | undefined;
  const outputJson = isJsonOutput(args);

  if (!keyId) {
    ctx.output.writeError('Key ID required. Use: xergon org keys revoke <id>');
    process.exit(1);
    return;
  }

  if (!force) {
    ctx.output.writeError(`This will permanently revoke key ${keyId}.`);
    ctx.output.write('Use --force to confirm revocation.');
    process.exit(1);
    return;
  }

  try {
    if (ctx.client) {
      await ctx.client.delete(`/api/v1/org/keys/${keyId}`);
    }

    const result: RevokeKeyResult = {
      success: true,
      id: keyId,
      message: `API key ${keyId} has been revoked.`,
    };

    if (outputJson) {
      ctx.output.write(JSON.stringify(result, null, 2));
      return;
    }

    ctx.output.success(`API key ${keyId} revoked.`);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to revoke API key: ${message}`);
    process.exit(1);
  }
}

async function handleSettings(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const settingKey = args.options.setting as string | undefined;
  const settingValue = args.options.value as string | undefined;
  const outputJson = isJsonOutput(args);

  try {
    let settings: OrgSettings;
    if (ctx.client) {
      if (settingKey && settingValue !== undefined) {
        const resp = await ctx.client.patch('/api/v1/org/settings', {
          [settingKey]: settingValue,
        });
        settings = (resp as any).settings;
        ctx.output.success(`Setting "${settingKey}" updated.`);
      } else {
        const resp = await ctx.client.get('/api/v1/org/settings');
        settings = resp as any;
      }
    } else {
      settings = getMockOrg().settings;
    }

    if (outputJson) {
      ctx.output.write(JSON.stringify(settings, null, 2));
      return;
    }

    ctx.output.write(ctx.output.colorize('Organization Settings:', 'bold'));
    ctx.output.write('');
    ctx.output.write(`  Default Role:      ${settings.defaultRole}`);
    ctx.output.write(`  Allow Self-Invite: ${settings.allowSelfInvite ? 'yes' : 'no'}`);
    ctx.output.write(`  Require 2FA:       ${settings.require2FA ? 'yes' : 'no'}`);
    ctx.output.write(`  Webhook URL:       ${settings.webhookUrl || '(none)'}`);
    ctx.output.write(`  Billing Email:     ${settings.billingEmail}`);
    ctx.output.write(`  Max API Keys:      ${settings.maxKeys}`);
    ctx.output.write(`  Max Members:       ${settings.maxMembers}`);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to get/update settings: ${message}`);
    process.exit(1);
  }
}

// ── Command action ─────────────────────────────────────────────────

async function orgAction(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const sub = args.positional[0];

  if (!sub) {
    ctx.output.writeError('Usage: xergon org <create|info|members|invite|remove|keys|settings> [args]');
    process.exit(1);
    return;
  }

  switch (sub) {
    case 'create':
      await handleCreate(args, ctx);
      break;
    case 'info':
      await handleInfo(args, ctx);
      break;
    case 'members':
      await handleMembers(args, ctx);
      break;
    case 'invite':
      await handleInvite(args, ctx);
      break;
    case 'remove':
    case 'rm':
      await handleRemove(args, ctx);
      break;
    case 'keys':
      await handleKeys(args, ctx);
      break;
    case 'settings':
    case 'config':
      await handleSettings(args, ctx);
      break;
    default:
      ctx.output.writeError(`Unknown subcommand: ${sub}`);
      ctx.output.write('Usage: xergon org <create|info|members|invite|remove|keys|settings> [args]');
      process.exit(1);
  }
}

// ── Command definition ─────────────────────────────────────────────

export const orgCommand: Command = {
  name: 'org',
  description: 'Organization management',
  aliases: ['organization', 'orgs'],
  options: orgOptions,
  action: orgAction,
};

// ── Exports for testing ───────────────────────────────────────────

export {
  // Types
  type OrgRole,
  type KeyScope,
  type KeyStatus,
  type MemberStatus,
  type OrgInfo,
  type OrgSettings,
  type OrgMember,
  type OrgApiKey,
  type InviteResult,
  type RemoveResult,
  type CreateKeyResult,
  type RevokeKeyResult,
  type CreateOrgResult,
  // Constants
  VALID_ROLES,
  VALID_SCOPES,
  ROLE_PERMISSIONS,
  ROLE_DISPLAY,
  SCOPE_DESCRIPTIONS,
  // Helpers
  generateOrgId,
  generateKeyId,
  generateApiKey,
  generateInviteToken,
  generateSlug,
  validateEmail,
  validateRole,
  validateScope,
  maskApiKey,
  formatRoleBadge,
  formatScopeList,
  formatExpiry,
  formatRelativeTime,
  parseScopes,
  isJsonOutput,
  // Handlers
  handleCreate,
  handleInfo,
  handleMembers,
  handleInvite,
  handleRemove,
  handleKeys,
  handleKeysList,
  handleKeysCreate,
  handleKeysRevoke,
  handleSettings,
  orgAction,
};
