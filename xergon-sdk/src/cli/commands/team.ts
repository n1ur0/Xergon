/**
 * CLI command: team
 *
 * Manage Xergon SDK teams for collaboration.
 *
 * Usage:
 *   xergon team create <name>                    -- create a team
 *   xergon team list                             -- list your teams
 *   xergon team info <id>                        -- team details
 *   xergon team invite <address> --role member   -- invite member
 *   xergon team accept <invite-id>               -- accept invite
 *   xergon team remove <user>                    -- remove member
 *   xergon team set-role <user> <role>           -- change role
 *   xergon team activity [team-id]               -- recent activity
 *   xergon team usage [team-id]                  -- usage summary
 *   xergon team delete <id>                      -- delete team
 */

import type { Command, ParsedArgs, CLIContext } from '../mod';

async function teamAction(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const sub = args.positional[0];

  if (!sub) {
    ctx.output.writeError('Usage: xergon team <create|list|info|invite|accept|remove|set-role|activity|usage|delete> [args]');
    process.exit(1);
    return;
  }

  switch (sub) {
    case 'create':
      await handleCreate(args, ctx);
      break;
    case 'list':
      await handleList(args, ctx);
      break;
    case 'info':
      await handleInfo(args, ctx);
      break;
    case 'invite':
      await handleInvite(args, ctx);
      break;
    case 'accept':
      await handleAccept(args, ctx);
      break;
    case 'remove':
      await handleRemove(args, ctx);
      break;
    case 'set-role':
      await handleSetRole(args, ctx);
      break;
    case 'activity':
      await handleActivity(args, ctx);
      break;
    case 'usage':
      await handleUsage(args, ctx);
      break;
    case 'delete':
      await handleDelete(args, ctx);
      break;
    default:
      ctx.output.writeError(`Unknown subcommand: ${sub}`);
      ctx.output.write('Usage: xergon team <create|list|info|invite|accept|remove|set-role|activity|usage|delete> [args]');
      process.exit(1);
  }
}

// ── create ─────────────────────────────────────────────────────────

async function handleCreate(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const name = args.positional[1];
  if (!name) {
    ctx.output.writeError('Team name required. Use: xergon team create <name>');
    process.exit(1);
    return;
  }

  const description = args.options.description as string | undefined;

  try {
    const { TeamClient } = await import('../../team');
    const client = new TeamClient();
    const team = await client.createTeam({ name, description });

    ctx.output.success(`Team "${team.name}" created`);
    ctx.output.write(`  ID:          ${team.id}`);
    ctx.output.write(`  Members:     ${team.members.length}`);
    ctx.output.write(`  Created:     ${team.createdAt}`);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to create team: ${message}`);
    process.exit(1);
  }
}

// ── list ───────────────────────────────────────────────────────────

async function handleList(_args: ParsedArgs, ctx: CLIContext): Promise<void> {
  try {
    const { TeamClient } = await import('../../team');
    const client = new TeamClient();
    const teams = await client.listTeams();

    if (teams.length === 0) {
      ctx.output.info('No teams found. Create one with: xergon team create <name>');
      return;
    }

    const tableData = teams.map(t => ({
      ID: t.id.length > 12 ? t.id.slice(0, 12) + '...' : t.id,
      Name: t.name,
      Members: String(t.members.length),
      Created: t.createdAt.split('T')[0],
    }));
    ctx.output.write(ctx.output.formatTable(tableData, `Your Teams (${teams.length})`));
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to list teams: ${message}`);
    process.exit(1);
  }
}

// ── info ───────────────────────────────────────────────────────────

async function handleInfo(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const teamId = args.positional[1];
  if (!teamId) {
    ctx.output.writeError('Team ID required. Use: xergon team info <id>');
    process.exit(1);
    return;
  }

  try {
    const { TeamClient } = await import('../../team');
    const client = new TeamClient();
    const team = await client.getTeam(teamId);

    ctx.output.write(`  ${ctx.output.colorize(team.name, 'bold')}`);
    ctx.output.write(`  ${team.description || 'No description'}`);
    ctx.output.write('');
    ctx.output.write(`  ID:          ${team.id}`);
    ctx.output.write(`  Created:     ${team.createdAt}`);
    ctx.output.write(`  Members:     ${team.members.length}`);
    ctx.output.write('');

    if (team.members.length > 0) {
      ctx.output.write(ctx.output.colorize('  Members:', 'bold'));
      for (const member of team.members) {
        const roleColor = member.role === 'owner' ? 'yellow' : member.role === 'admin' ? 'cyan' : 'dim';
        ctx.output.write(`    ${ctx.output.colorize(member.role, roleColor)}  ${member.address.slice(0, 16)}...`);
      }
    }

    ctx.output.write('');
    ctx.output.write(ctx.output.colorize('  Settings:', 'bold'));
    ctx.output.write(`    Default Model:    ${team.settings.defaultModel || 'none'}`);
    ctx.output.write(`    Max Budget:       ${team.settings.maxBudget}`);
    ctx.output.write(`    Allowed Models:   ${team.settings.allowedModels.join(', ') || 'all'}`);
    ctx.output.write(`    Notifications:    ${team.settings.notificationLevel}`);
    ctx.output.write(`    Approval Required: ${team.settings.approvalRequired ? 'yes' : 'no'}`);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to get team info: ${message}`);
    process.exit(1);
  }
}

// ── invite ─────────────────────────────────────────────────────────

async function handleInvite(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const address = args.positional[1];
  const role = (args.options.role as string || 'member') as 'admin' | 'member' | 'viewer';

  if (!address) {
    ctx.output.writeError('Address required. Use: xergon team invite <address> --role member');
    process.exit(1);
    return;
  }

  const teamId = args.options.team as string;
  if (!teamId) {
    ctx.output.writeError('Team ID required. Use: xergon team invite <address> --team <id>');
    process.exit(1);
    return;
  }

  try {
    const { TeamClient } = await import('../../team');
    const client = new TeamClient();
    const invite = await client.inviteMember(teamId, address, role);

    ctx.output.success(`Invite sent to ${address}`);
    ctx.output.write(`  Invite ID:   ${invite.id}`);
    ctx.output.write(`  Role:        ${invite.role}`);
    ctx.output.write(`  Expires:     ${invite.expiresAt}`);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to send invite: ${message}`);
    process.exit(1);
  }
}

// ── accept ─────────────────────────────────────────────────────────

async function handleAccept(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const inviteId = args.positional[1];
  if (!inviteId) {
    ctx.output.writeError('Invite ID required. Use: xergon team accept <invite-id>');
    process.exit(1);
    return;
  }

  try {
    const { TeamClient } = await import('../../team');
    const client = new TeamClient();
    const result = await client.acceptInvite(inviteId);

    ctx.output.success(`Joined team "${result.team.name}"`);
    ctx.output.write(`  Role: ${result.member.role}`);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to accept invite: ${message}`);
    process.exit(1);
  }
}

// ── remove ─────────────────────────────────────────────────────────

async function handleRemove(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const userId = args.positional[1];
  if (!userId) {
    ctx.output.writeError('User ID required. Use: xergon team remove <user-id>');
    process.exit(1);
    return;
  }

  const teamId = args.options.team as string;
  if (!teamId) {
    ctx.output.writeError('Team ID required. Use: xergon team remove <user-id> --team <id>');
    process.exit(1);
    return;
  }

  try {
    const { TeamClient } = await import('../../team');
    const client = new TeamClient();
    await client.removeMember(teamId, userId);
    ctx.output.success(`Member ${userId} removed from team`);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to remove member: ${message}`);
    process.exit(1);
  }
}

// ── set-role ───────────────────────────────────────────────────────

async function handleSetRole(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const userId = args.positional[1];
  const role = args.positional[2] as 'admin' | 'member' | 'viewer';

  if (!userId || !role) {
    ctx.output.writeError('Usage: xergon team set-role <user-id> <admin|member|viewer>');
    process.exit(1);
    return;
  }

  const validRoles = ['admin', 'member', 'viewer'];
  if (!validRoles.includes(role)) {
    ctx.output.writeError(`Invalid role "${role}". Must be one of: ${validRoles.join(', ')}`);
    process.exit(1);
    return;
  }

  const teamId = args.options.team as string;
  if (!teamId) {
    ctx.output.writeError('Team ID required. Use: xergon team set-role <user-id> <role> --team <id>');
    process.exit(1);
    return;
  }

  try {
    const { TeamClient } = await import('../../team');
    const client = new TeamClient();
    await client.updateRole(teamId, userId, role);
    ctx.output.success(`Role updated: ${userId} is now ${role}`);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to update role: ${message}`);
    process.exit(1);
  }
}

// ── activity ───────────────────────────────────────────────────────

async function handleActivity(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const teamId = args.positional[1];

  if (!teamId) {
    ctx.output.writeError('Team ID required. Use: xergon team activity <team-id>');
    process.exit(1);
    return;
  }

  const limit = (args.options.limit as number) || 20;

  try {
    const { TeamClient } = await import('../../team');
    const client = new TeamClient();
    const activities = await client.getTeamActivity(teamId, limit);

    if (activities.length === 0) {
      ctx.output.info('No recent activity.');
      return;
    }

    const tableData = activities.map(a => ({
      Time: a.timestamp.split('T')[0] + ' ' + (a.timestamp.split('T')[1] || '').slice(0, 8),
      User: a.userId.length > 12 ? a.userId.slice(0, 12) + '...' : a.userId,
      Action: a.action,
      Details: a.details.length > 40 ? a.details.slice(0, 40) + '...' : a.details,
    }));
    ctx.output.write(ctx.output.formatTable(tableData, `Recent Activity (${activities.length})`));
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to get activity: ${message}`);
    process.exit(1);
  }
}

// ── usage ──────────────────────────────────────────────────────────

async function handleUsage(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const teamId = args.positional[1];

  if (!teamId) {
    ctx.output.writeError('Team ID required. Use: xergon team usage <team-id>');
    process.exit(1);
    return;
  }

  try {
    const { TeamClient } = await import('../../team');
    const client = new TeamClient();
    const usage = await client.getTeamUsage(teamId);

    const budgetPercent = usage.budgetLimit > 0
      ? ((usage.budgetUsed / usage.budgetLimit) * 100).toFixed(1)
      : 'N/A';

    ctx.output.write(ctx.output.colorize(`  Usage for team ${teamId}`, 'bold'));
    ctx.output.write('');
    ctx.output.write(`  Period:         ${usage.period}`);
    ctx.output.write(`  Total Requests: ${usage.totalRequests}`);
    ctx.output.write(`  Total Tokens:   ${usage.totalTokens.toLocaleString()}`);
    ctx.output.write(`  Total Cost:     ${usage.totalCost}`);
    ctx.output.write(`  Budget:         ${usage.budgetUsed} / ${usage.budgetLimit} (${budgetPercent}%)`);
    ctx.output.write('');

    if (usage.topModels.length > 0) {
      ctx.output.write(ctx.output.colorize('  Top Models:', 'bold'));
      for (const m of usage.topModels) {
        ctx.output.write(`    ${m.model}: ${m.requests} requests, ${m.tokens.toLocaleString()} tokens`);
      }
    }

    if (usage.topMembers.length > 0) {
      ctx.output.write('');
      ctx.output.write(ctx.output.colorize('  Top Members:', 'bold'));
      for (const m of usage.topMembers) {
        const addr = m.userId.length > 16 ? m.userId.slice(0, 16) + '...' : m.userId;
        ctx.output.write(`    ${addr}: ${m.requests} requests, ${m.tokens.toLocaleString()} tokens`);
      }
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to get usage: ${message}`);
    process.exit(1);
  }
}

// ── delete ─────────────────────────────────────────────────────────

async function handleDelete(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const teamId = args.positional[1];
  if (!teamId) {
    ctx.output.writeError('Team ID required. Use: xergon team delete <id>');
    process.exit(1);
    return;
  }

  if (!args.options.force) {
    ctx.output.writeError('This will permanently delete the team and all its data.');
    ctx.output.write('Use --force to confirm deletion.');
    process.exit(1);
    return;
  }

  try {
    const { TeamClient } = await import('../../team');
    const client = new TeamClient();
    await client.deleteTeam(teamId);
    ctx.output.success(`Team ${teamId} deleted`);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to delete team: ${message}`);
    process.exit(1);
  }
}

export const teamCommand: Command = {
  name: 'team',
  description: 'Manage teams for collaboration',
  aliases: ['teams'],
  options: [],
  action: teamAction,
};
