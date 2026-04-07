/**
 * CLI command: governance
 *
 * Manage governance proposals on the Xergon Network.
 * List, create, vote on, execute, check status of proposals,
 * delegate voting power, view tallies, treasury, stats, and more.
 *
 * Usage:
 *   xergon governance list [--status active|passed|failed]
 *   xergon governance create
 *   xergon governance vote --proposal-id <id> --vote for|against|abstain
 *   xergon governance execute --proposal-id <id>
 *   xergon governance status --proposal-id <id>
 *   xergon governance delegate --to <address> --weight <amount>
 *   xergon governance revoke --from <address>
 *   xergon governance tally --proposal-id <id>
 *   xergon governance treasury [--history]
 *   xergon governance stats
 *   xergon governance delegations --address <addr>
 *   xergon governance receipts --proposal-id <id>
 */

import type { Command, CommandOption, ParsedArgs, CLIContext } from '../mod';

// ── Types ──────────────────────────────────────────────────────────

interface Proposal {
  id: string;
  title: string;
  description: string;
  category: string;
  proposer: string;
  status: 'active' | 'passed' | 'failed' | 'executed' | 'pending';
  votesFor: number;
  votesAgainst: number;
  votesAbstain: number;
  quorum: number;
  totalVoters: number;
  createdAt: string;
  expiresAt: string;
  parameters?: Record<string, unknown>;
}

interface CreateProposalInput {
  title: string;
  description: string;
  category: string;
  parameters?: Record<string, unknown>;
}

interface VoteInput {
  proposalId: string;
  vote: 'for' | 'against' | 'abstain';
}

interface ProposalStatus {
  id: string;
  title: string;
  status: string;
  votesFor: number;
  votesAgainst: number;
  votesAbstain: number;
  totalVotes: number;
  quorum: number;
  quorumMet: boolean;
  passes: boolean;
  turnout: string;
  timeRemaining?: string;
}

interface DelegateInput {
  to: string;
  weight: number;
}

interface DelegateResult {
  delegator: string;
  delegate: string;
  weight: number;
  timestamp: string;
  txId?: string;
}

interface TallyResult {
  proposalId: string;
  votesFor: number;
  votesAgainst: number;
  votesAbstain: number;
  totalVotes: number;
  quorum: number;
  quorumMet: boolean;
  approvalPercent: number;
  passes: boolean;
}

interface TreasuryStatus {
  balance: string;
  available: string;
  locked: string;
  lastOperation?: string;
  lastOperationTime?: string;
}

interface TreasuryOperation {
  id: string;
  type: string;
  amount: string;
  description: string;
  timestamp: string;
  status: string;
}

interface TreasuryHistory {
  balance: string;
  operations: TreasuryOperation[];
  totalDeposits: string;
  totalWithdrawals: string;
}

interface GovStats {
  totalProposals: number;
  activeProposals: number;
  passedProposals: number;
  failedProposals: number;
  executedProposals: number;
  passRate: string;
  participationRate: string;
  totalDelegations: number;
  totalVoters: number;
  avgTurnout: string;
}

interface Delegation {
  delegator: string;
  delegate: string;
  weight: number;
  createdAt: string;
  status: string;
}

interface ExecutionReceipt {
  txId: string;
  proposalId: string;
  result: string;
  timestamp: string;
  executor: string;
  gasUsed?: number;
}

// ── Helpers ────────────────────────────────────────────────────────

function isJsonOutput(args: ParsedArgs): boolean {
  return args.options.json === true;
}

function isTableFormat(args: ParsedArgs): boolean {
  return args.options.format === 'table';
}

/**
 * Read a line from stdin (non-interactive fallback returns empty string).
 */
async function readLine(prompt: string): Promise<string> {
  if (!process.stdin.isTTY) return '';
  process.stdout.write(prompt);
  return new Promise<string>((resolve) => {
    const rl = require('node:readline').createInterface({ input: process.stdin, output: process.stdout });
    rl.question('', (answer: string) => {
      rl.close();
      resolve(answer.trim());
    });
  });
}

// ── Subcommand: list ───────────────────────────────────────────────

async function handleList(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const statusFilter = args.options.status ? String(args.options.status) : undefined;

  try {
    let proposals: Proposal[];

    if (ctx.client?.governance?.list) {
      proposals = await ctx.client.governance.list({ status: statusFilter });
    } else {
      // Fallback mock for when no governance client is available
      proposals = [];
    }

    // Apply local status filter if not done server-side
    if (statusFilter && proposals.length > 0) {
      proposals = proposals.filter(p => p.status === statusFilter);
    }

    if (proposals.length === 0) {
      ctx.output.info('No governance proposals found.');
      return;
    }

    if (isJsonOutput(args)) {
      ctx.output.write(JSON.stringify(proposals, null, 2));
      return;
    }

    if (isTableFormat(args)) {
      const tableData = proposals.map(p => ({
        ID: p.id.substring(0, 12) + '...',
        Title: p.title.length > 30 ? p.title.substring(0, 30) + '...' : p.title,
        Status: p.status,
        Category: p.category,
        'For/Against': `${p.votesFor}/${p.votesAgainst}`,
        Quorum: `${((p.votesFor + p.votesAgainst + p.votesAbstain) / p.totalVoters * 100).toFixed(1)}%`,
        Expires: p.expiresAt ? new Date(p.expiresAt).toISOString().slice(0, 10) : '-',
      }));
      ctx.output.write(ctx.output.formatTable(tableData, `Governance Proposals (${proposals.length})`));
      return;
    }

    // Text output
    ctx.output.write(ctx.output.colorize(`Governance Proposals (${proposals.length})`, 'bold'));
    ctx.output.write('');
    for (const p of proposals) {
      const statusColor = p.status === 'passed' ? 'green'
        : p.status === 'active' ? 'cyan'
        : p.status === 'failed' ? 'red'
        : p.status === 'executed' ? 'green'
        : 'dim';
      ctx.output.write(`  ${ctx.output.colorize(p.id.substring(0, 16) + '...', 'cyan')}  ${ctx.output.colorize(p.status.toUpperCase(), statusColor)}`);
      ctx.output.write(`    ${p.title}`);
      ctx.output.write(`    Category: ${p.category}  |  Proposer: ${p.proposer.substring(0, 16)}...`);
      const totalVotes = p.votesFor + p.votesAgainst + p.votesAbstain;
      ctx.output.write(`    Votes: ${p.votesFor} for, ${p.votesAgainst} against, ${p.votesAbstain} abstain (${totalVotes} total)`);
      ctx.output.write('');
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to list proposals: ${message}`);
    process.exit(1);
  }
}

// ── Subcommand: create ─────────────────────────────────────────────

async function handleCreate(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const title = args.options.title ? String(args.options.title) : await readLine('Title: ');
  const description = args.options.description ? String(args.options.description) : await readLine('Description: ');
  const category = args.options.category ? String(args.options.category) : await readLine('Category (e.g., parameter, upgrade, budget): ');

  if (!title) {
    ctx.output.writeError('Proposal title is required. Use --title or enter interactively.');
    process.exit(1);
    return;
  }

  // Parse optional parameters from --params JSON
  let parameters: Record<string, unknown> | undefined;
  if (args.options.params) {
    try {
      parameters = JSON.parse(String(args.options.params));
    } catch {
      ctx.output.writeError('--params must be valid JSON');
      process.exit(1);
      return;
    }
  }

  const input: CreateProposalInput = {
    title,
    description: description || '',
    category: category || 'general',
    parameters,
  };

  ctx.output.info('Creating governance proposal...');

  try {
    let proposal: Proposal;

    if (ctx.client?.governance?.create) {
      proposal = await ctx.client.governance.create(input);
    } else {
      throw new Error('Governance client not available. Ensure you are connected to the Xergon network.');
    }

    if (isJsonOutput(args)) {
      ctx.output.write(JSON.stringify(proposal, null, 2));
      return;
    }

    ctx.output.success('Proposal created successfully');
    ctx.output.write('');
    ctx.output.write(ctx.output.formatText({
      'Proposal ID': proposal.id,
      Title: proposal.title,
      Description: proposal.description,
      Category: proposal.category,
      Status: proposal.status,
      Proposer: proposal.proposer,
      'Created At': proposal.createdAt,
      'Expires At': proposal.expiresAt,
    }, 'New Proposal'));
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to create proposal: ${message}`);
    process.exit(1);
  }
}

// ── Subcommand: vote ───────────────────────────────────────────────

async function handleVote(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const proposalId = args.options.proposal_id ? String(args.options.proposal_id) : undefined;
  const voteStr = args.options.vote ? String(args.options.vote) : undefined;

  if (!proposalId) {
    ctx.output.writeError('Usage: xergon governance vote --proposal-id <id> --vote for|against|abstain');
    process.exit(1);
    return;
  }
  if (!voteStr || !['for', 'against', 'abstain'].includes(voteStr.toLowerCase())) {
    ctx.output.writeError('Vote must be one of: for, against, abstain');
    process.exit(1);
    return;
  }

  const vote = voteStr.toLowerCase() as 'for' | 'against' | 'abstain';
  const input: VoteInput = { proposalId, vote };

  ctx.output.info(`Voting "${vote}" on proposal ${proposalId.substring(0, 16)}...`);

  try {
    if (ctx.client?.governance?.vote) {
      await ctx.client.governance.vote(input);
    } else {
      throw new Error('Governance client not available.');
    }

    if (isJsonOutput(args)) {
      ctx.output.write(JSON.stringify({ proposalId, vote, status: 'submitted' }, null, 2));
      return;
    }

    ctx.output.success(`Vote "${vote}" submitted on proposal ${proposalId.substring(0, 16)}...`);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to submit vote: ${message}`);
    process.exit(1);
  }
}

// ── Subcommand: execute ────────────────────────────────────────────

async function handleExecute(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const proposalId = args.options.proposal_id ? String(args.options.proposal_id) : undefined;

  if (!proposalId) {
    ctx.output.writeError('Usage: xergon governance execute --proposal-id <id>');
    process.exit(1);
    return;
  }

  ctx.output.info(`Executing proposal ${proposalId.substring(0, 16)}...`);

  try {
    let result: { txId?: string; status: string; message?: string };

    if (ctx.client?.governance?.execute) {
      result = await ctx.client.governance.execute(proposalId);
    } else {
      throw new Error('Governance client not available.');
    }

    if (isJsonOutput(args)) {
      ctx.output.write(JSON.stringify(result, null, 2));
      return;
    }

    ctx.output.success(`Proposal ${proposalId.substring(0, 16)}... executed successfully`);
    if (result.txId) {
      ctx.output.write(`  TX ID: ${ctx.output.colorize(result.txId, 'cyan')}`);
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to execute proposal: ${message}`);
    process.exit(1);
  }
}

// ── Subcommand: status ─────────────────────────────────────────────

async function handleStatus(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const proposalId = args.options.proposal_id ? String(args.options.proposal_id) : undefined;

  if (!proposalId) {
    ctx.output.writeError('Usage: xergon governance status --proposal-id <id>');
    process.exit(1);
    return;
  }

  try {
    let status: ProposalStatus;

    if (ctx.client?.governance?.status) {
      status = await ctx.client.governance.status(proposalId);
    } else {
      throw new Error('Governance client not available.');
    }

    if (isJsonOutput(args)) {
      ctx.output.write(JSON.stringify(status, null, 2));
      return;
    }

    // Text output
    ctx.output.write(ctx.output.colorize('Proposal Status', 'bold'));
    ctx.output.write(ctx.output.colorize('─'.repeat(40), 'dim'));
    ctx.output.write(ctx.output.formatText({
      'Proposal ID': status.id,
      Title: status.title,
      Status: status.status,
      'Votes For': String(status.votesFor),
      'Votes Against': String(status.votesAgainst),
      'Votes Abstain': String(status.votesAbstain),
      'Total Votes': String(status.totalVotes),
      Quorum: `${status.turnout} (required: ${status.quorum})`,
      'Quorum Met': String(status.quorumMet),
      Passes: String(status.passes),
    }));

    if (status.timeRemaining) {
      ctx.output.write(`  Time Remaining: ${status.timeRemaining}`);
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to get proposal status: ${message}`);
    process.exit(1);
  }
}

// ── Subcommand: delegate ───────────────────────────────────────────

async function handleDelegate(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const to = args.options.to ? String(args.options.to) : undefined;
  const weight = args.options.weight ? Number(args.options.weight) : undefined;

  if (!to) {
    ctx.output.writeError('Usage: xergon governance delegate --to <address> --weight <amount>');
    process.exit(1);
    return;
  }
  if (!weight || weight <= 0) {
    ctx.output.writeError('Weight must be a positive number. Use --weight <amount>');
    process.exit(1);
    return;
  }

  const input: DelegateInput = { to, weight };

  ctx.output.info(`Delegating ${weight} voting power to ${to.substring(0, 16)}...`);

  try {
    let result: DelegateResult;

    if (ctx.client?.governance?.delegate) {
      result = await ctx.client.governance.delegate(input);
    } else {
      throw new Error('Governance client not available. Ensure you are connected to the Xergon network.');
    }

    if (isJsonOutput(args)) {
      ctx.output.write(JSON.stringify(result, null, 2));
      return;
    }

    ctx.output.success('Delegation created successfully');
    ctx.output.write('');
    ctx.output.write(ctx.output.formatText({
      Delegator: result.delegator,
      Delegate: result.delegate,
      Weight: String(result.weight),
      'Timestamp': result.timestamp,
      ...(result.txId ? { 'TX ID': result.txId } : {}),
    }, 'Delegation Details'));
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to delegate voting power: ${message}`);
    process.exit(1);
  }
}

// ── Subcommand: revoke ─────────────────────────────────────────────

async function handleRevoke(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const from = args.options.from ? String(args.options.from) : undefined;

  if (!from) {
    ctx.output.writeError('Usage: xergon governance revoke --from <address>');
    process.exit(1);
    return;
  }

  ctx.output.info(`Revoking delegation from ${from.substring(0, 16)}...`);

  try {
    if (ctx.client?.governance?.revokeDelegation) {
      await ctx.client.governance.revokeDelegation(from);
    } else {
      throw new Error('Governance client not available. Ensure you are connected to the Xergon network.');
    }

    if (isJsonOutput(args)) {
      ctx.output.write(JSON.stringify({ delegator: from, status: 'revoked' }, null, 2));
      return;
    }

    ctx.output.success(`Delegation from ${from.substring(0, 16)}... revoked successfully`);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to revoke delegation: ${message}`);
    process.exit(1);
  }
}

// ── Subcommand: tally ──────────────────────────────────────────────

async function handleTally(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const proposalId = args.options.proposal_id ? String(args.options.proposal_id) : undefined;

  if (!proposalId) {
    ctx.output.writeError('Usage: xergon governance tally --proposal-id <id>');
    process.exit(1);
    return;
  }

  try {
    let tally: TallyResult;

    if (ctx.client?.governance?.tally) {
      tally = await ctx.client.governance.tally(proposalId);
    } else {
      throw new Error('Governance client not available.');
    }

    if (isJsonOutput(args)) {
      ctx.output.write(JSON.stringify(tally, null, 2));
      return;
    }

    // Text output
    const passFailColor = tally.passes ? 'green' : 'red';
    const quorumColor = tally.quorumMet ? 'green' : 'yellow';

    ctx.output.write(ctx.output.colorize(`Vote Tally: ${proposalId.substring(0, 16)}...`, 'bold'));
    ctx.output.write(ctx.output.colorize('─'.repeat(40), 'dim'));
    ctx.output.write(ctx.output.formatText({
      'Proposal ID': tally.proposalId,
      'Votes For': String(tally.votesFor),
      'Votes Against': String(tally.votesAgainst),
      'Votes Abstain': String(tally.votesAbstain),
      'Total Votes': String(tally.totalVotes),
      'Approval': `${tally.approvalPercent.toFixed(1)}%`,
      'Quorum': `${tally.totalVotes}/${tally.quorum} (${tally.quorumMet ? 'met' : 'not met'})`,
    }));
    ctx.output.write('');
    ctx.output.write(`  Verdict: ${ctx.output.colorize(tally.passes ? 'PASS' : 'FAIL', passFailColor)}`);
    ctx.output.write(`  Quorum:  ${ctx.output.colorize(tally.quorumMet ? 'MET' : 'NOT MET', quorumColor)}`);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to get vote tally: ${message}`);
    process.exit(1);
  }
}

// ── Subcommand: treasury ───────────────────────────────────────────

async function handleTreasury(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const showHistory = args.options.history === true;

  try {
    if (showHistory) {
      let history: TreasuryHistory;

      if (ctx.client?.governance?.treasuryHistory) {
        history = await ctx.client.governance.treasuryHistory();
      } else {
        throw new Error('Governance client not available.');
      }

      if (isJsonOutput(args)) {
        ctx.output.write(JSON.stringify(history, null, 2));
        return;
      }

      ctx.output.write(ctx.output.colorize('Treasury History', 'bold'));
      ctx.output.write(ctx.output.colorize('─'.repeat(40), 'dim'));
      ctx.output.write(ctx.output.formatText({
        'Current Balance': history.balance,
        'Total Deposits': history.totalDeposits,
        'Total Withdrawals': history.totalWithdrawals,
      }));
      ctx.output.write('');

      if (history.operations.length === 0) {
        ctx.output.info('No treasury operations found.');
        return;
      }

      if (isTableFormat(args)) {
        const tableData = history.operations.map(op => ({
          ID: op.id.substring(0, 12) + '...',
          Type: op.type,
          Amount: op.amount,
          Description: op.description.length > 30 ? op.description.substring(0, 30) + '...' : op.description,
          Status: op.status,
          Date: new Date(op.timestamp).toISOString().slice(0, 10),
        }));
        ctx.output.write(ctx.output.formatTable(tableData, `Operations (${history.operations.length})`));
        return;
      }

      ctx.output.write(ctx.output.colorize(`Recent Operations (${history.operations.length})`, 'bold'));
      ctx.output.write('');
      for (const op of history.operations) {
        const statusColor = op.status === 'completed' ? 'green'
          : op.status === 'pending' ? 'yellow'
          : 'red';
        ctx.output.write(`  ${ctx.output.colorize(op.id.substring(0, 16) + '...', 'cyan')}  ${ctx.output.colorize(op.status.toUpperCase(), statusColor)}`);
        ctx.output.write(`    ${op.type}: ${op.amount}  --  ${op.description}`);
        ctx.output.write(`    ${new Date(op.timestamp).toISOString()}`);
        ctx.output.write('');
      }
    } else {
      let status: TreasuryStatus;

      if (ctx.client?.governance?.treasuryStatus) {
        status = await ctx.client.governance.treasuryStatus();
      } else {
        throw new Error('Governance client not available.');
      }

      if (isJsonOutput(args)) {
        ctx.output.write(JSON.stringify(status, null, 2));
        return;
      }

      ctx.output.write(ctx.output.colorize('Treasury Status', 'bold'));
      ctx.output.write(ctx.output.colorize('─'.repeat(40), 'dim'));
      ctx.output.write(ctx.output.formatText({
        'Total Balance': status.balance,
        'Available': status.available,
        'Locked': status.locked,
        ...(status.lastOperation ? { 'Last Operation': status.lastOperation } : {}),
        ...(status.lastOperationTime ? { 'Last Operation Time': status.lastOperationTime } : {}),
      }));
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to get treasury info: ${message}`);
    process.exit(1);
  }
}

// ── Subcommand: stats ──────────────────────────────────────────────

async function handleStats(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  try {
    let stats: GovStats;

    if (ctx.client?.governance?.stats) {
      stats = await ctx.client.governance.stats();
    } else {
      throw new Error('Governance client not available.');
    }

    if (isJsonOutput(args)) {
      ctx.output.write(JSON.stringify(stats, null, 2));
      return;
    }

    ctx.output.write(ctx.output.colorize('Governance Statistics', 'bold'));
    ctx.output.write(ctx.output.colorize('─'.repeat(40), 'dim'));
    ctx.output.write(ctx.output.formatText({
      'Total Proposals': String(stats.totalProposals),
      'Active Proposals': String(stats.activeProposals),
      'Passed Proposals': String(stats.passedProposals),
      'Failed Proposals': String(stats.failedProposals),
      'Executed Proposals': String(stats.executedProposals),
      'Pass Rate': stats.passRate,
      'Participation Rate': stats.participationRate,
      'Average Turnout': stats.avgTurnout,
      'Total Voters': String(stats.totalVoters),
      'Total Delegations': String(stats.totalDelegations),
    }));
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to get governance stats: ${message}`);
    process.exit(1);
  }
}

// ── Subcommand: delegations ────────────────────────────────────────

async function handleDelegations(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const address = args.options.address ? String(args.options.address) : undefined;

  if (!address) {
    ctx.output.writeError('Usage: xergon governance delegations --address <addr>');
    process.exit(1);
    return;
  }

  try {
    let delegations: Delegation[];

    if (ctx.client?.governance?.delegations) {
      delegations = await ctx.client.governance.delegations(address);
    } else {
      throw new Error('Governance client not available.');
    }

    if (isJsonOutput(args)) {
      ctx.output.write(JSON.stringify(delegations, null, 2));
      return;
    }

    if (delegations.length === 0) {
      ctx.output.info(`No delegations found for ${address.substring(0, 16)}...`);
      return;
    }

    if (isTableFormat(args)) {
      const tableData = delegations.map(d => ({
        Delegator: d.delegator.substring(0, 16) + '...',
        Delegate: d.delegate.substring(0, 16) + '...',
        Weight: String(d.weight),
        Status: d.status,
        Created: new Date(d.createdAt).toISOString().slice(0, 10),
      }));
      ctx.output.write(ctx.output.formatTable(tableData, `Delegations for ${address.substring(0, 16)}... (${delegations.length})`));
      return;
    }

    // Text output
    ctx.output.write(ctx.output.colorize(`Delegations for ${address.substring(0, 16)}... (${delegations.length})`, 'bold'));
    ctx.output.write('');
    for (const d of delegations) {
      const statusColor = d.status === 'active' ? 'green'
        : d.status === 'revoked' ? 'red'
        : 'dim';
      ctx.output.write(`  ${ctx.output.colorize(d.delegate.substring(0, 16) + '...', 'cyan')}  ${ctx.output.colorize(d.status.toUpperCase(), statusColor)}`);
      ctx.output.write(`    Weight: ${d.weight}  |  Delegator: ${d.delegator.substring(0, 16)}...`);
      ctx.output.write(`    Created: ${new Date(d.createdAt).toISOString()}`);
      ctx.output.write('');
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to list delegations: ${message}`);
    process.exit(1);
  }
}

// ── Subcommand: receipts ───────────────────────────────────────────

async function handleReceipts(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const proposalId = args.options.proposal_id ? String(args.options.proposal_id) : undefined;

  if (!proposalId) {
    ctx.output.writeError('Usage: xergon governance receipts --proposal-id <id>');
    process.exit(1);
    return;
  }

  try {
    let receipts: ExecutionReceipt[];

    if (ctx.client?.governance?.receipts) {
      receipts = await ctx.client.governance.receipts(proposalId);
    } else {
      throw new Error('Governance client not available.');
    }

    if (isJsonOutput(args)) {
      ctx.output.write(JSON.stringify(receipts, null, 2));
      return;
    }

    if (receipts.length === 0) {
      ctx.output.info(`No execution receipts found for proposal ${proposalId.substring(0, 16)}...`);
      return;
    }

    if (isTableFormat(args)) {
      const tableData = receipts.map(r => ({
        'TX ID': r.txId.substring(0, 16) + '...',
        Result: r.result,
        Executor: r.executor.substring(0, 16) + '...',
        'Gas Used': r.gasUsed ? String(r.gasUsed) : '-',
        Timestamp: new Date(r.timestamp).toISOString().slice(0, 19).replace('T', ' '),
      }));
      ctx.output.write(ctx.output.formatTable(tableData, `Execution Receipts (${receipts.length})`));
      return;
    }

    // Text output
    ctx.output.write(ctx.output.colorize(`Execution Receipts: ${proposalId.substring(0, 16)}... (${receipts.length})`, 'bold'));
    ctx.output.write('');
    for (const r of receipts) {
      const resultColor = r.result === 'success' ? 'green'
        : r.result === 'failed' ? 'red'
        : 'yellow';
      ctx.output.write(`  ${ctx.output.colorize(r.txId.substring(0, 24), 'cyan')}  ${ctx.output.colorize(r.result.toUpperCase(), resultColor)}`);
      ctx.output.write(`    Executor: ${r.executor.substring(0, 16)}...`);
      if (r.gasUsed) {
        ctx.output.write(`    Gas Used: ${r.gasUsed}`);
      }
      ctx.output.write(`    Timestamp: ${new Date(r.timestamp).toISOString()}`);
      ctx.output.write('');
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to get execution receipts: ${message}`);
    process.exit(1);
  }
}

// ── Command action ─────────────────────────────────────────────────

async function governanceAction(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const sub = args.positional[0];

  if (!sub) {
    ctx.output.writeError('Usage: xergon governance <subcommand> [options]');
    ctx.output.write('');
    ctx.output.write('Subcommands:');
    ctx.output.write('  list        List governance proposals');
    ctx.output.write('  create      Create a new governance proposal');
    ctx.output.write('  vote        Vote on a proposal');
    ctx.output.write('  execute     Execute a passed proposal');
    ctx.output.write('  status      Show detailed status of a proposal');
    ctx.output.write('  delegate    Delegate voting power to another address');
    ctx.output.write('  revoke      Revoke a vote delegation');
    ctx.output.write('  tally       Show vote tally for a proposal');
    ctx.output.write('  treasury    Show treasury status');
    ctx.output.write('  stats       Show governance statistics');
    ctx.output.write('  delegations List delegations for an address');
    ctx.output.write('  receipts    Show execution receipts for a proposal');
    process.exit(1);
    return;
  }

  switch (sub) {
    case 'list':
      await handleList(args, ctx);
      break;
    case 'create':
      await handleCreate(args, ctx);
      break;
    case 'vote':
      await handleVote(args, ctx);
      break;
    case 'execute':
      await handleExecute(args, ctx);
      break;
    case 'status':
      await handleStatus(args, ctx);
      break;
    case 'delegate':
      await handleDelegate(args, ctx);
      break;
    case 'revoke':
      await handleRevoke(args, ctx);
      break;
    case 'tally':
      await handleTally(args, ctx);
      break;
    case 'treasury':
      await handleTreasury(args, ctx);
      break;
    case 'stats':
      await handleStats(args, ctx);
      break;
    case 'delegations':
      await handleDelegations(args, ctx);
      break;
    case 'receipts':
      await handleReceipts(args, ctx);
      break;
    default:
      ctx.output.writeError(`Unknown subcommand: ${sub}`);
      ctx.output.write('Valid subcommands: list, create, vote, execute, status, delegate, revoke, tally, treasury, stats, delegations, receipts');
      process.exit(1);
      break;
  }
}

// ── Options ────────────────────────────────────────────────────────

const governanceOptions: CommandOption[] = [
  {
    name: 'proposal_id',
    short: '',
    long: '--proposal-id',
    description: 'Proposal ID for vote, execute, status, tally, or receipts subcommands',
    required: false,
    type: 'string',
  },
  {
    name: 'vote',
    short: '',
    long: '--vote',
    description: 'Vote choice: for, against, or abstain',
    required: false,
    type: 'string',
  },
  {
    name: 'json',
    short: '',
    long: '--json',
    description: 'Output in JSON format',
    required: false,
    type: 'boolean',
  },
  {
    name: 'format',
    short: '',
    long: '--format',
    description: 'Output format: text, json, or table',
    required: false,
    type: 'string',
  },
  {
    name: 'status',
    short: '',
    long: '--status',
    description: 'Filter proposals by status: active, passed, failed, executed',
    required: false,
    type: 'string',
  },
  {
    name: 'title',
    short: '',
    long: '--title',
    description: 'Proposal title (for create subcommand)',
    required: false,
    type: 'string',
  },
  {
    name: 'description',
    short: '',
    long: '--description',
    description: 'Proposal description (for create subcommand)',
    required: false,
    type: 'string',
  },
  {
    name: 'category',
    short: '',
    long: '--category',
    description: 'Proposal category: parameter, upgrade, budget, general',
    required: false,
    type: 'string',
  },
  {
    name: 'params',
    short: '',
    long: '--params',
    description: 'Proposal parameters as JSON string (for create subcommand)',
    required: false,
    type: 'string',
  },
  {
    name: 'to',
    short: '',
    long: '--to',
    description: 'Target address to delegate voting power to',
    required: false,
    type: 'string',
  },
  {
    name: 'weight',
    short: '',
    long: '--weight',
    description: 'Amount of voting power to delegate',
    required: false,
    type: 'number',
  },
  {
    name: 'from',
    short: '',
    long: '--from',
    description: 'Delegator address to revoke delegation from',
    required: false,
    type: 'string',
  },
  {
    name: 'history',
    short: '',
    long: '--history',
    description: 'Show treasury operation history (for treasury subcommand)',
    required: false,
    type: 'boolean',
  },
  {
    name: 'address',
    short: '',
    long: '--address',
    description: 'Address to list delegations for (for delegations subcommand)',
    required: false,
    type: 'string',
  },
];

// ── Command export ─────────────────────────────────────────────────

export const governanceCommand: Command = {
  name: 'governance',
  description: 'Manage governance proposals on the Xergon Network',
  aliases: ['gov', 'proposals'],
  options: governanceOptions,
  action: governanceAction,
};
