/**
 * Xergon SDK -- Team Collaboration
 *
 * Provides team management capabilities including creating teams,
 * inviting members, managing roles, tracking activity, and usage.
 */

// ── Types ───────────────────────────────────────────────────────────

export type TeamRole = 'owner' | 'admin' | 'member' | 'viewer';
export type NotificationLevel = 'all' | 'important' | 'none';

export interface TeamMember {
  userId: string;
  address: string;
  role: TeamRole;
  joinedAt: string;
}

export interface TeamSettings {
  defaultModel: string;
  maxBudget: number;
  allowedModels: string[];
  notificationLevel: NotificationLevel;
  approvalRequired: boolean;
}

export interface Team {
  id: string;
  name: string;
  description: string;
  members: TeamMember[];
  createdAt: string;
  settings: TeamSettings;
}

export interface TeamInvite {
  id: string;
  teamId: string;
  role: Exclude<TeamRole, 'owner'>;
  invitedBy: string;
  expiresAt: string;
  status: 'pending' | 'accepted' | 'expired';
}

export interface TeamActivity {
  id: string;
  teamId: string;
  userId: string;
  action: string;
  details: string;
  timestamp: string;
}

export interface TeamUsage {
  teamId: string;
  period: string;
  totalRequests: number;
  totalTokens: number;
  totalCost: number;
  budgetUsed: number;
  budgetLimit: number;
  topModels: Array<{ model: string; requests: number; tokens: number }>;
  topMembers: Array<{ userId: string; requests: number; tokens: number }>;
}

export interface CreateTeamParams {
  name: string;
  description?: string;
  settings?: Partial<TeamSettings>;
}

export interface UpdateTeamParams {
  name?: string;
  description?: string;
  settings?: Partial<TeamSettings>;
}

// ── Team Client ────────────────────────────────────────────────────

export class TeamClient {
  private baseUrl: string;

  constructor(options?: { baseUrl?: string }) {
    this.baseUrl = options?.baseUrl || 'https://relay.xergon.gg';
  }

  /**
   * Create a new team.
   */
  async createTeam(params: CreateTeamParams): Promise<Team> {
    const url = `${this.baseUrl}/v1/teams`;
    const response = await fetch(url, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(params),
    });

    if (!response.ok) {
      const data = await response.json().catch(() => ({}));
      throw new Error((data as any).message || `Failed to create team: ${response.status}`);
    }

    return await response.json() as Team;
  }

  /**
   * Get team details by ID.
   */
  async getTeam(id: string): Promise<Team> {
    const url = `${this.baseUrl}/v1/teams/${encodeURIComponent(id)}`;
    const response = await fetch(url);

    if (!response.ok) {
      if (response.status === 404) throw new Error(`Team "${id}" not found`);
      throw new Error(`Failed to get team: ${response.status}`);
    }

    return await response.json() as Team;
  }

  /**
   * List all teams the authenticated user belongs to.
   */
  async listTeams(): Promise<Team[]> {
    const url = `${this.baseUrl}/v1/teams`;
    const response = await fetch(url);

    if (!response.ok) {
      throw new Error(`Failed to list teams: ${response.status}`);
    }

    const data = await response.json() as { teams?: Team[] } | Team[];
    return Array.isArray(data) ? data : (data.teams || []);
  }

  /**
   * Update team settings or metadata.
   */
  async updateTeam(id: string, updates: UpdateTeamParams): Promise<Team> {
    const url = `${this.baseUrl}/v1/teams/${encodeURIComponent(id)}`;
    const response = await fetch(url, {
      method: 'PATCH',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(updates),
    });

    if (!response.ok) {
      const data = await response.json().catch(() => ({}));
      throw new Error((data as any).message || `Failed to update team: ${response.status}`);
    }

    return await response.json() as Team;
  }

  /**
   * Delete a team (owner only).
   */
  async deleteTeam(id: string): Promise<void> {
    const url = `${this.baseUrl}/v1/teams/${encodeURIComponent(id)}`;
    const response = await fetch(url, { method: 'DELETE' });

    if (!response.ok) {
      const data = await response.json().catch(() => ({}));
      throw new Error((data as any).message || `Failed to delete team: ${response.status}`);
    }
  }

  /**
   * Invite a member to a team.
   */
  async inviteMember(teamId: string, address: string, role: Exclude<TeamRole, 'owner'> = 'member'): Promise<TeamInvite> {
    const url = `${this.baseUrl}/v1/teams/${encodeURIComponent(teamId)}/invites`;
    const response = await fetch(url, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ address, role }),
    });

    if (!response.ok) {
      const data = await response.json().catch(() => ({}));
      throw new Error((data as any).message || `Failed to invite member: ${response.status}`);
    }

    return await response.json() as TeamInvite;
  }

  /**
   * Accept a team invitation.
   */
  async acceptInvite(inviteId: string): Promise<{ team: Team; member: TeamMember }> {
    const url = `${this.baseUrl}/v1/teams/invites/${encodeURIComponent(inviteId)}/accept`;
    const response = await fetch(url, { method: 'POST' });

    if (!response.ok) {
      const data = await response.json().catch(() => ({}));
      throw new Error((data as any).message || `Failed to accept invite: ${response.status}`);
    }

    return await response.json() as { team: Team; member: TeamMember };
  }

  /**
   * Remove a member from a team.
   */
  async removeMember(teamId: string, userId: string): Promise<void> {
    const url = `${this.baseUrl}/v1/teams/${encodeURIComponent(teamId)}/members/${encodeURIComponent(userId)}`;
    const response = await fetch(url, { method: 'DELETE' });

    if (!response.ok) {
      const data = await response.json().catch(() => ({}));
      throw new Error((data as any).message || `Failed to remove member: ${response.status}`);
    }
  }

  /**
   * Update a member's role.
   */
  async updateRole(teamId: string, userId: string, role: Exclude<TeamRole, 'owner'>): Promise<TeamMember> {
    const url = `${this.baseUrl}/v1/teams/${encodeURIComponent(teamId)}/members/${encodeURIComponent(userId)}/role`;
    const response = await fetch(url, {
      method: 'PATCH',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ role }),
    });

    if (!response.ok) {
      const data = await response.json().catch(() => ({}));
      throw new Error((data as any).message || `Failed to update role: ${response.status}`);
    }

    return await response.json() as TeamMember;
  }

  /**
   * Get recent team activity.
   */
  async getTeamActivity(teamId: string, limit: number = 20): Promise<TeamActivity[]> {
    const params = new URLSearchParams({ limit: String(limit) });
    const url = `${this.baseUrl}/v1/teams/${encodeURIComponent(teamId)}/activity?${params}`;
    const response = await fetch(url);

    if (!response.ok) {
      throw new Error(`Failed to get activity: ${response.status}`);
    }

    const data = await response.json() as { activities?: TeamActivity[] } | TeamActivity[];
    return Array.isArray(data) ? data : (data.activities || []);
  }

  /**
   * Get team usage/billing summary.
   */
  async getTeamUsage(teamId: string): Promise<TeamUsage> {
    const url = `${this.baseUrl}/v1/teams/${encodeURIComponent(teamId)}/usage`;
    const response = await fetch(url);

    if (!response.ok) {
      throw new Error(`Failed to get usage: ${response.status}`);
    }

    return await response.json() as TeamUsage;
  }

  /**
   * Transfer team ownership to another member.
   */
  async transferOwnership(teamId: string, newOwnerId: string): Promise<Team> {
    const url = `${this.baseUrl}/v1/teams/${encodeURIComponent(teamId)}/ownership`;
    const response = await fetch(url, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ newOwnerId }),
    });

    if (!response.ok) {
      const data = await response.json().catch(() => ({}));
      throw new Error((data as any).message || `Failed to transfer ownership: ${response.status}`);
    }

    return await response.json() as Team;
  }
}

// ── Convenience Functions ──────────────────────────────────────────

let defaultClient: TeamClient | null = null;

function getClient(): TeamClient {
  if (!defaultClient) {
    defaultClient = new TeamClient();
  }
  return defaultClient;
}

export async function createTeam(params: CreateTeamParams): Promise<Team> {
  return getClient().createTeam(params);
}

export async function getTeam(id: string): Promise<Team> {
  return getClient().getTeam(id);
}

export async function listTeams(): Promise<Team[]> {
  return getClient().listTeams();
}

export async function updateTeam(id: string, updates: UpdateTeamParams): Promise<Team> {
  return getClient().updateTeam(id, updates);
}

export async function deleteTeam(id: string): Promise<void> {
  return getClient().deleteTeam(id);
}

export async function inviteMember(teamId: string, address: string, role?: Exclude<TeamRole, 'owner'>): Promise<TeamInvite> {
  return getClient().inviteMember(teamId, address, role);
}

export async function acceptInvite(inviteId: string): Promise<{ team: Team; member: TeamMember }> {
  return getClient().acceptInvite(inviteId);
}

export async function removeMember(teamId: string, userId: string): Promise<void> {
  return getClient().removeMember(teamId, userId);
}

export async function updateRole(teamId: string, userId: string, role: Exclude<TeamRole, 'owner'>): Promise<TeamMember> {
  return getClient().updateRole(teamId, userId, role);
}

export async function getTeamActivity(teamId: string, limit?: number): Promise<TeamActivity[]> {
  return getClient().getTeamActivity(teamId, limit);
}

export async function getTeamUsage(teamId: string): Promise<TeamUsage> {
  return getClient().getTeamUsage(teamId);
}

export async function transferOwnership(teamId: string, newOwnerId: string): Promise<Team> {
  return getClient().transferOwnership(teamId, newOwnerId);
}
