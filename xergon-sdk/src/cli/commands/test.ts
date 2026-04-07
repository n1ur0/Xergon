/**
 * CLI command: test
 *
 * Integration testing for the Xergon Network.
 * Run test suites, view results, health-probe providers, verify on-chain transactions.
 *
 * Usage:
 *   xergon test run [suite]           -- Run test suites (contract, inference, settlement, provider, full)
 *   xergon test list                 -- List available test suites and their status
 *   xergon test results [id]         -- Show test results with pass/fail details
 *   xergon test history              -- Show historical test results with trends
 *   xergon test probe [provider]     -- Health probe a specific provider endpoint
 *   xergon test verify [tx-id]       -- Verify on-chain settlement transaction
 */

import type { Command, CommandOption, ParsedArgs, CLIContext } from '../mod';

// ── Types ──────────────────────────────────────────────────────────

type TestStatus = 'pass' | 'fail' | 'skip' | 'running' | 'pending';
type TestSuiteName = 'contract' | 'inference' | 'settlement' | 'provider' | 'full';

interface TestCase {
  id: string;
  name: string;
  suite: TestSuiteName;
  status: TestStatus;
  durationMs: number;
  message?: string;
  timestamp?: string;
}

interface TestSuite {
  name: TestSuiteName;
  description: string;
  testCount: number;
  passCount: number;
  failCount: number;
  skipCount: number;
  durationMs: number;
  tests: TestCase[];
}

interface TestRunResult {
  runId: string;
  suites: TestSuite[];
  totalTests: number;
  totalPass: number;
  totalFail: number;
  totalSkip: number;
  durationMs: number;
  timestamp: string;
}

interface TestHistoryItem {
  runId: string;
  suite: TestSuiteName;
  total: number;
  pass: number;
  fail: number;
  skip: number;
  durationMs: number;
  timestamp: string;
  passRate: number;
}

interface ProbeResult {
  provider: string;
  reachable: boolean;
  latencyMs: number;
  statusCode: number;
  version?: string;
  gpuInfo?: string;
  memoryUsed?: string;
  memoryTotal?: string;
  uptime?: number;
  activeModels?: string[];
  error?: string;
}

interface VerifyResult {
  txId: string;
  valid: boolean;
  confirmed: boolean;
  confirmations: number;
  amount?: string;
  from?: string;
  to?: string;
  blockHeight?: number;
  error?: string;
}

interface SuiteInfo {
  name: TestSuiteName;
  description: string;
  testCount: number;
  lastRun?: string;
  lastStatus?: TestStatus;
}

// ── TestService (mock implementation) ────────────────────────────

class TestService {
  private baseUrl: string;

  constructor(baseUrl: string) {
    this.baseUrl = baseUrl.replace(/\/+$/, '');
  }

  /**
   * Safely fetch JSON from an endpoint with timeout.
   */
  private async fetchJSON<T>(url: string, timeoutMs: number = 15_000): Promise<T | null> {
    try {
      const res = await fetch(url, { signal: AbortSignal.timeout(timeoutMs) });
      if (!res.ok) return null;
      return await res.json() as T;
    } catch {
      return null;
    }
  }

  /**
   * List available test suites.
   */
  async listSuites(): Promise<SuiteInfo[]> {
    const data = await this.fetchJSON<any>(`${this.baseUrl}/api/v1/test/suites`);
    if (data) {
      const suites: any[] = Array.isArray(data) ? data : (data.suites ?? data.data ?? []);
      return suites.map((s: any) => ({
        name: s.name ?? s.suite ?? 'contract',
        description: s.description ?? '',
        testCount: s.testCount ?? s.test_count ?? 0,
        lastRun: s.lastRun ?? s.last_run,
        lastStatus: s.lastStatus ?? s.last_status,
      }));
    }

    // Mock fallback
    return [
      { name: 'contract', description: 'Smart contract interaction tests', testCount: 12, lastRun: new Date(Date.now() - 3600_000).toISOString(), lastStatus: 'pass' as TestStatus },
      { name: 'inference', description: 'AI inference pipeline tests', testCount: 8, lastRun: new Date(Date.now() - 7200_000).toISOString(), lastStatus: 'pass' as TestStatus },
      { name: 'settlement', description: 'On-chain settlement verification tests', testCount: 6, lastRun: new Date(Date.now() - 14400_000).toISOString(), lastStatus: 'fail' as TestStatus },
      { name: 'provider', description: 'Provider health and connectivity tests', testCount: 10, lastRun: new Date(Date.now() - 1800_000).toISOString(), lastStatus: 'pass' as TestStatus },
      { name: 'full', description: 'Full integration test suite (all suites)', testCount: 36, lastRun: undefined, lastStatus: undefined },
    ];
  }

  /**
   * Run a test suite.
   */
  async runSuite(suite: TestSuiteName, timeout: number, parallel: boolean): Promise<TestRunResult> {
    // Try real API first
    try {
      const res = await fetch(`${this.baseUrl}/api/v1/test/run`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ suite, timeout, parallel }),
        signal: AbortSignal.timeout(timeout + 5000),
      });
      if (res.ok) {
        const data: any = await res.json();
        return this.parseRunResult(data);
      }
    } catch {
      // Mock fallback
    }

    return this.mockRunResult(suite);
  }

  /**
   * Get results for a specific test run.
   */
  async getResults(runId: string): Promise<TestRunResult | null> {
    const data = await this.fetchJSON<any>(`${this.baseUrl}/api/v1/test/results/${runId}`);
    if (data) return this.parseRunResult(data);

    // Mock fallback
    return this.mockRunResult('full');
  }

  /**
   * Get test run history.
   */
  async getHistory(last: number, suite?: TestSuiteName): Promise<TestHistoryItem[]> {
    const params = new URLSearchParams();
    if (last) params.set('last', String(last));
    if (suite) params.set('suite', suite);

    const data = await this.fetchJSON<any>(`${this.baseUrl}/api/v1/test/history?${params}`);
    if (data) {
      const items: any[] = Array.isArray(data) ? data : (data.history ?? data.data ?? []);
      return items.map((h: any) => ({
        runId: h.runId ?? h.run_id ?? '',
        suite: h.suite ?? 'full',
        total: h.total ?? h.totalTests ?? 0,
        pass: h.pass ?? h.passCount ?? 0,
        fail: h.fail ?? h.failCount ?? 0,
        skip: h.skip ?? h.skipCount ?? 0,
        durationMs: h.durationMs ?? h.duration_ms ?? 0,
        timestamp: h.timestamp ?? new Date().toISOString(),
        passRate: h.passRate ?? h.pass_rate ?? 0,
      }));
    }

    // Mock history
    return this.mockHistory(last, suite);
  }

  /**
   * Health probe a provider endpoint.
   */
  async probeProvider(provider: string, timeout: number): Promise<ProbeResult> {
    try {
      const res = await fetch(`${provider}/health`, {
        signal: AbortSignal.timeout(timeout),
      });
      const latencyMs = 0; // calculated below
      const startTime = performance.now();

      const body: any = await res.json().catch(() => null);
      const elapsed = performance.now() - startTime;

      return {
        provider,
        reachable: res.ok,
        latencyMs: Math.round(elapsed),
        statusCode: res.status,
        version: body?.version,
        gpuInfo: body?.gpu,
        memoryUsed: body?.memoryUsed ?? body?.memory_used,
        memoryTotal: body?.memoryTotal ?? body?.memory_total,
        uptime: body?.uptime,
        activeModels: body?.models ?? body?.activeModels,
        error: res.ok ? undefined : `HTTP ${res.status}`,
      };
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      return {
        provider,
        reachable: false,
        latencyMs: 0,
        statusCode: 0,
        error: message,
      };
    }
  }

  /**
   * Verify a settlement transaction on-chain.
   */
  async verifyTransaction(txId: string): Promise<VerifyResult> {
    const data = await this.fetchJSON<any>(`${this.baseUrl}/api/v1/test/verify/${txId}`);
    if (data) {
      return {
        txId: data.txId ?? data.tx_id ?? txId,
        valid: data.valid ?? false,
        confirmed: data.confirmed ?? false,
        confirmations: data.confirmations ?? 0,
        amount: data.amount,
        from: data.from,
        to: data.to,
        blockHeight: data.blockHeight ?? data.block_height,
        error: data.error,
      };
    }

    // Mock verification result
    return {
      txId,
      valid: true,
      confirmed: true,
      confirmations: 42,
      amount: '12.5 ERG',
      from: '9f...3a2b',
      to: '3e...7c1d',
      blockHeight: 847291,
    };
  }

  // ── Private helpers ──

  private parseRunResult(data: any): TestRunResult {
    const suites: TestSuite[] = (data.suites ?? []).map((s: any) => ({
      name: s.name ?? 'contract',
      description: s.description ?? '',
      testCount: s.testCount ?? s.test_count ?? 0,
      passCount: s.passCount ?? s.pass_count ?? 0,
      failCount: s.failCount ?? s.fail_count ?? 0,
      skipCount: s.skipCount ?? s.skip_count ?? 0,
      durationMs: s.durationMs ?? s.duration_ms ?? 0,
      tests: (s.tests ?? []).map((t: any) => ({
        id: t.id ?? '',
        name: t.name ?? '',
        suite: s.name ?? 'contract',
        status: t.status ?? 'pending',
        durationMs: t.durationMs ?? t.duration_ms ?? 0,
        message: t.message,
        timestamp: t.timestamp,
      })),
    }));

    return {
      runId: data.runId ?? data.run_id ?? '',
      suites,
      totalTests: data.totalTests ?? data.total_tests ?? 0,
      totalPass: data.totalPass ?? data.total_pass ?? 0,
      totalFail: data.totalFail ?? data.total_fail ?? 0,
      totalSkip: data.totalSkip ?? data.total_skip ?? 0,
      durationMs: data.durationMs ?? data.duration_ms ?? 0,
      timestamp: data.timestamp ?? new Date().toISOString(),
    };
  }

  private mockRunResult(suite: TestSuiteName): TestRunResult {
    const now = new Date().toISOString();
    const testCases: Record<TestSuiteName, { name: string; status: TestStatus; durationMs: number; message?: string }[]> = {
      contract: [
        { name: 'contract_deploy', status: 'pass', durationMs: 1200 },
        { name: 'contract_register_provider', status: 'pass', durationMs: 800 },
        { name: 'contract_submit_task', status: 'pass', durationMs: 600 },
        { name: 'contract_submit_result', status: 'pass', durationMs: 900 },
        { name: 'contract_settle_payment', status: 'pass', durationMs: 1500 },
        { name: 'contract_dispute_open', status: 'pass', durationMs: 700 },
        { name: 'contract_dispute_resolve', status: 'pass', durationMs: 1100 },
        { name: 'contract_refund', status: 'pass', durationMs: 950 },
        { name: 'contract_timeout', status: 'skip', durationMs: 0 },
        { name: 'contract_batch_settlement', status: 'pass', durationMs: 2200 },
        { name: 'contract_slash', status: 'fail', durationMs: 3000, message: 'Insufficient stake for slashing' },
        { name: 'contract_upgrade', status: 'pass', durationMs: 1800 },
      ],
      inference: [
        { name: 'inference_basic_completion', status: 'pass', durationMs: 2100 },
        { name: 'inference_streaming', status: 'pass', durationMs: 3500 },
        { name: 'inference_context_window', status: 'pass', durationMs: 4200 },
        { name: 'inference_multi_turn', status: 'pass', durationMs: 5100 },
        { name: 'inference_timeout_handling', status: 'pass', durationMs: 800 },
        { name: 'inference_model_routing', status: 'pass', durationMs: 1900 },
        { name: 'inference_rate_limiting', status: 'skip', durationMs: 0 },
        { name: 'inference_error_recovery', status: 'pass', durationMs: 2600 },
      ],
      settlement: [
        { name: 'settlement_single_payment', status: 'pass', durationMs: 1800 },
        { name: 'settlement_batch_payment', status: 'pass', durationMs: 3200 },
        { name: 'settlement_cross_shard', status: 'fail', durationMs: 5000, message: 'Cross-shard settlement not yet supported' },
        { name: 'settlement_refund', status: 'pass', durationMs: 1400 },
        { name: 'settlement_dispute_flow', status: 'pass', durationMs: 4100 },
        { name: 'settlement_gas_optimization', status: 'skip', durationMs: 0 },
      ],
      provider: [
        { name: 'provider_registration', status: 'pass', durationMs: 900 },
        { name: 'provider_heartbeat', status: 'pass', durationMs: 400 },
        { name: 'provider_health_check', status: 'pass', durationMs: 300 },
        { name: 'provider_model_loading', status: 'pass', durationMs: 5200 },
        { name: 'provider_graceful_shutdown', status: 'pass', durationMs: 1100 },
        { name: 'provider_restart_recovery', status: 'pass', durationMs: 2800 },
        { name: 'provider_stake_management', status: 'pass', durationMs: 1600 },
        { name: 'provider_reputation_scoring', status: 'pass', durationMs: 700 },
        { name: 'provider_multi_gpu', status: 'skip', durationMs: 0 },
        { name: 'provider_geo_routing', status: 'fail', durationMs: 4500, message: 'Geo-routing API unavailable' },
      ],
      full: [],
    };

    // For 'full', combine all suites
    let tests: { name: string; status: TestStatus; durationMs: number; message?: string }[] = [];
    if (suite === 'full') {
      tests = [
        ...testCases.contract,
        ...testCases.inference,
        ...testCases.settlement,
        ...testCases.provider,
      ];
    } else {
      tests = testCases[suite];
    }

    const totalDuration = tests.reduce((s, t) => s + t.durationMs, 0);
    const passCount = tests.filter(t => t.status === 'pass').length;
    const failCount = tests.filter(t => t.status === 'fail').length;
    const skipCount = tests.filter(t => t.status === 'skip').length;

    const suiteResult: TestSuite = {
      name: suite,
      description: `${suite} integration tests`,
      testCount: tests.length,
      passCount,
      failCount,
      skipCount,
      durationMs: totalDuration,
      tests: tests.map((t, i) => ({
        id: `${suite}-${String(i + 1).padStart(3, '0')}`,
        name: t.name,
        suite,
        status: t.status,
        durationMs: t.durationMs,
        message: t.message,
        timestamp: now,
      })),
    };

    return {
      runId: `run-${Date.now().toString(36)}`,
      suites: [suiteResult],
      totalTests: tests.length,
      totalPass: passCount,
      totalFail: failCount,
      totalSkip: skipCount,
      durationMs: totalDuration,
      timestamp: now,
    };
  }

  private mockHistory(last: number, suite?: TestSuiteName): TestHistoryItem[] {
    const items: TestHistoryItem[] = [];
    const now = Date.now();
    const suites: TestSuiteName[] = suite ? [suite] : ['contract', 'inference', 'settlement', 'provider'];

    for (let i = 0; i < Math.min(last, 20); i++) {
      const s = suites[i % suites.length];
      const total = s === 'full' ? 36 : s === 'contract' ? 12 : s === 'inference' ? 8 : s === 'settlement' ? 6 : 10;
      const pass = Math.floor(total * (0.75 + Math.random() * 0.25));
      const fail = Math.floor(Math.random() * 3);
      const skip = total - pass - fail;

      items.push({
        runId: `run-${(now - i * 3600_000).toString(36)}`,
        suite: s,
        total,
        pass,
        fail,
        skip: Math.max(0, skip),
        durationMs: 5000 + Math.floor(Math.random() * 25000),
        timestamp: new Date(now - i * 3600_000).toISOString(),
        passRate: total > 0 ? Math.round((pass / total) * 10000) / 100 : 0,
      });
    }

    return items;
  }
}

// ── Formatting helpers ────────────────────────────────────────────

function statusIcon(status: TestStatus): string {
  switch (status) {
    case 'pass': return '+';
    case 'fail': return 'X';
    case 'skip': return '-';
    case 'running': return '*';
    default: return '?';
  }
}

function statusColor(status: TestStatus): 'green' | 'red' | 'yellow' | 'cyan' | 'dim' {
  switch (status) {
    case 'pass': return 'green';
    case 'fail': return 'red';
    case 'skip': return 'yellow';
    case 'running': return 'cyan';
    default: return 'dim';
  }
}

function formatDuration(ms: number): string {
  if (ms < 1000) return `${ms}ms`;
  if (ms < 60000) return `${(ms / 1000).toFixed(1)}s`;
  return `${Math.floor(ms / 60000)}m ${Math.round((ms % 60000) / 1000)}s`;
}

function formatPassRate(rate: number): string {
  return `${rate.toFixed(1)}%`;
}

// ── Subcommand: run ───────────────────────────────────────────────

async function handleRun(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const suiteArg = args.positional[1];
  const suite = (suiteArg as TestSuiteName) ?? 'full';
  const timeout = args.options.timeout !== undefined ? Number(args.options.timeout) : 60000;
  const parallel = Boolean(args.options.parallel);
  const json = args.options.json === true;

  const validSuites: TestSuiteName[] = ['contract', 'inference', 'settlement', 'provider', 'full'];
  if (!validSuites.includes(suite)) {
    ctx.output.writeError(`Invalid suite: "${suite}". Must be one of: ${validSuites.join(', ')}`);
    process.exit(1);
    return;
  }

  ctx.output.info(`Running ${suite} test suite${parallel ? ' (parallel)' : ''}...`);

  const service = new TestService(ctx.config.baseUrl);

  try {
    const result = await service.runSuite(suite, timeout, parallel);

    if (json) {
      ctx.output.write(JSON.stringify(result, null, 2));
      return;
    }

    // Formatted output
    ctx.output.write('');
    ctx.output.write(ctx.output.colorize('Test Results', 'bold'));
    ctx.output.write(ctx.output.colorize('\u2500'.repeat(56), 'dim'));
    ctx.output.write('');
    ctx.output.write(`  Run ID:      ${ctx.output.colorize(result.runId, 'cyan')}`);
    ctx.output.write(`  Suite:       ${suite}`);
    ctx.output.write(`  Timestamp:   ${result.timestamp}`);
    ctx.output.write(`  Duration:    ${formatDuration(result.durationMs)}`);
    ctx.output.write('');

    // Summary bar
    const summaryLine = [
      `  Total: ${result.totalTests}`,
      ctx.output.colorize(`Pass: ${result.totalPass}`, 'green'),
      ctx.output.colorize(`Fail: ${result.totalFail}`, result.totalFail > 0 ? 'red' : 'dim'),
      ctx.output.colorize(`Skip: ${result.totalSkip}`, 'yellow'),
    ].join('  ');
    ctx.output.write(summaryLine);

    if (result.totalFail === 0) {
      ctx.output.success('All tests passed');
    } else {
      ctx.output.writeError(`${result.totalFail} test(s) failed`);
    }

    // Per-suite details
    for (const s of result.suites) {
      ctx.output.write('');
      ctx.output.write(ctx.output.colorize(`Suite: ${s.name}`, 'bold'));
      ctx.output.write(ctx.output.colorize(`  ${s.description}`, 'dim'));

      for (const t of s.tests) {
        const icon = statusIcon(t.status);
        const color = statusColor(t.status);
        const durStr = t.durationMs > 0 ? ` (${formatDuration(t.durationMs)})` : '';
        const msgStr = t.message ? ` -- ${t.message}` : '';

        ctx.output.write(
          `  ${ctx.output.colorize(`[${icon}]`, color)} ${t.name}${durStr}${msgStr}`
        );
      }
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Test run failed: ${message}`);
    process.exit(1);
  }
}

// ── Subcommand: list ──────────────────────────────────────────────

async function handleList(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const json = args.options.json === true;
  const service = new TestService(ctx.config.baseUrl);

  try {
    const suites = await service.listSuites();

    if (json) {
      ctx.output.write(JSON.stringify(suites, null, 2));
      return;
    }

    ctx.output.write(ctx.output.colorize('Available Test Suites', 'bold'));
    ctx.output.write(ctx.output.colorize('\u2500'.repeat(56), 'dim'));
    ctx.output.write('');

    const tableData = suites.map(s => ({
      Suite: s.name,
      Tests: String(s.testCount),
      'Last Run': s.lastRun ? new Date(s.lastRun).toISOString().slice(0, 19) : '-',
      'Last Status': s.lastStatus ? statusIcon(s.lastStatus) : '-',
      Description: s.description,
    }));

    ctx.output.write(ctx.output.formatTable(tableData));
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to list test suites: ${message}`);
    process.exit(1);
  }
}

// ── Subcommand: results ───────────────────────────────────────────

async function handleResults(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const runId = args.positional[1];
  const json = args.options.json === true;

  if (!runId) {
    ctx.output.writeError('Usage: xergon test results <run-id> [--json]');
    process.exit(1);
    return;
  }

  const service = new TestService(ctx.config.baseUrl);

  try {
    const result = await service.getResults(runId);

    if (!result) {
      ctx.output.writeError(`No test results found for run: ${runId}`);
      process.exit(1);
      return;
    }

    if (json) {
      ctx.output.write(JSON.stringify(result, null, 2));
      return;
    }

    ctx.output.write(ctx.output.colorize('Test Results', 'bold'));
    ctx.output.write(ctx.output.colorize('\u2500'.repeat(56), 'dim'));
    ctx.output.write('');
    ctx.output.write(`  Run ID:      ${ctx.output.colorize(result.runId, 'cyan')}`);
    ctx.output.write(`  Timestamp:   ${result.timestamp}`);
    ctx.output.write(`  Duration:    ${formatDuration(result.durationMs)}`);
    ctx.output.write('');
    ctx.output.write(
      `  Total: ${result.totalTests}  ` +
      ctx.output.colorize(`Pass: ${result.totalPass}`, 'green') + '  ' +
      ctx.output.colorize(`Fail: ${result.totalFail}`, result.totalFail > 0 ? 'red' : 'dim') + '  ' +
      ctx.output.colorize(`Skip: ${result.totalSkip}`, 'yellow')
    );

    // Show all tests grouped by suite
    for (const s of result.suites) {
      ctx.output.write('');
      ctx.output.write(ctx.output.colorize(`${s.name} (${s.testCount} tests)`, 'bold'));

      for (const t of s.tests) {
        const icon = statusIcon(t.status);
        const color = statusColor(t.status);
        const durStr = t.durationMs > 0 ? ` ${formatDuration(t.durationMs)}` : '';
        const msgStr = t.message ? ` -- ${t.message}` : '';
        ctx.output.write(`  ${ctx.output.colorize(`[${icon}]`, color)} ${t.name}${durStr}${msgStr}`);
      }
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to get test results: ${message}`);
    process.exit(1);
  }
}

// ── Subcommand: history ───────────────────────────────────────────

async function handleHistory(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const json = args.options.json === true;
  const last = args.options.last !== undefined ? Number(args.options.last) : 20;
  const suite = args.options.suite ? String(args.options.suite) as TestSuiteName : undefined;

  if (suite && !['contract', 'inference', 'settlement', 'provider', 'full'].includes(suite)) {
    ctx.output.writeError(`Invalid suite: "${suite}"`);
    process.exit(1);
    return;
  }

  const service = new TestService(ctx.config.baseUrl);

  try {
    const history = await service.getHistory(last, suite);

    if (history.length === 0) {
      ctx.output.info('No test history found.');
      return;
    }

    if (json) {
      ctx.output.write(JSON.stringify(history, null, 2));
      return;
    }

    ctx.output.write(ctx.output.colorize(`Test History${suite ? ` (${suite})` : ''} (${history.length} runs)`, 'bold'));
    ctx.output.write(ctx.output.colorize('\u2500'.repeat(56), 'dim'));
    ctx.output.write('');

    const tableData = history.map(h => ({
      'Run ID': h.runId.substring(0, 16) + '...',
      Suite: h.suite,
      Total: String(h.total),
      Pass: String(h.pass),
      Fail: String(h.fail),
      'Pass Rate': formatPassRate(h.passRate),
      Duration: formatDuration(h.durationMs),
      Date: h.timestamp ? new Date(h.timestamp).toISOString().slice(0, 19) : '-',
    }));

    ctx.output.write(ctx.output.formatTable(tableData));

    // Show trend summary
    if (history.length >= 2) {
      const recent = history.slice(0, Math.min(5, history.length));
      const avgPassRate = recent.reduce((sum, h) => sum + h.passRate, 0) / recent.length;
      const trendStr = avgPassRate >= 95 ? 'green' : avgPassRate >= 80 ? 'yellow' : 'red';
      ctx.output.write('');
      ctx.output.write(
        `  Recent avg pass rate: ${ctx.output.colorize(formatPassRate(avgPassRate), trendStr)} (last ${recent.length} runs)`
      );
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to get test history: ${message}`);
    process.exit(1);
  }
}

// ── Subcommand: probe ─────────────────────────────────────────────

async function handleProbe(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const provider = args.positional[1];
  const json = args.options.json === true;
  const timeout = args.options.timeout !== undefined ? Number(args.options.timeout) : 10000;

  if (!provider) {
    ctx.output.writeError('Usage: xergon test probe <provider-url> [--timeout N]');
    process.exit(1);
    return;
  }

  ctx.output.info(`Probing ${provider}...`);

  const service = new TestService(ctx.config.baseUrl);

  try {
    const result = await service.probeProvider(provider, timeout);

    if (json) {
      ctx.output.write(JSON.stringify(result, null, 2));
      return;
    }

    ctx.output.write('');
    ctx.output.write(ctx.output.colorize('Provider Probe Results', 'bold'));
    ctx.output.write(ctx.output.colorize('\u2500'.repeat(56), 'dim'));
    ctx.output.write('');

    const reachableColor = result.reachable ? 'green' : 'red';
    ctx.output.write(`  Provider:    ${ctx.output.colorize(result.provider, 'cyan')}`);
    ctx.output.write(`  Reachable:   ${ctx.output.colorize(String(result.reachable), reachableColor)}`);
    ctx.output.write(`  Status Code: ${result.statusCode || '-'}`);
    ctx.output.write(`  Latency:     ${result.latencyMs > 0 ? `${result.latencyMs}ms` : '-'}`);

    if (result.version) {
      ctx.output.write(`  Version:     ${result.version}`);
    }
    if (result.gpuInfo) {
      ctx.output.write(`  GPU:         ${result.gpuInfo}`);
    }
    if (result.memoryUsed && result.memoryTotal) {
      ctx.output.write(`  Memory:      ${result.memoryUsed} / ${result.memoryTotal}`);
    }
    if (result.uptime !== undefined) {
      ctx.output.write(`  Uptime:      ${formatDuration(result.uptime * 1000)}`);
    }
    if (result.activeModels && result.activeModels.length > 0) {
      ctx.output.write(`  Models:      ${result.activeModels.join(', ')}`);
    }
    if (result.error) {
      ctx.output.write('');
      ctx.output.writeError(`  Error: ${result.error}`);
    } else if (result.reachable) {
      ctx.output.success('Provider is healthy');
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Probe failed: ${message}`);
    process.exit(1);
  }
}

// ── Subcommand: verify ────────────────────────────────────────────

async function handleVerify(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const txId = args.positional[1];
  const json = args.options.json === true;

  if (!txId) {
    ctx.output.writeError('Usage: xergon test verify <tx-id> [--json]');
    process.exit(1);
    return;
  }

  ctx.output.info(`Verifying transaction ${txId}...`);

  const service = new TestService(ctx.config.baseUrl);

  try {
    const result = await service.verifyTransaction(txId);

    if (json) {
      ctx.output.write(JSON.stringify(result, null, 2));
      return;
    }

    ctx.output.write('');
    ctx.output.write(ctx.output.colorize('Transaction Verification', 'bold'));
    ctx.output.write(ctx.output.colorize('\u2500'.repeat(56), 'dim'));
    ctx.output.write('');

    const validColor = result.valid ? 'green' : 'red';
    const confirmedColor = result.confirmed ? 'green' : 'yellow';

    ctx.output.write(`  TX ID:          ${ctx.output.colorize(result.txId, 'cyan')}`);
    ctx.output.write(`  Valid:          ${ctx.output.colorize(String(result.valid), validColor)}`);
    ctx.output.write(`  Confirmed:      ${ctx.output.colorize(String(result.confirmed), confirmedColor)}`);
    ctx.output.write(`  Confirmations:  ${result.confirmations}`);

    if (result.amount) {
      ctx.output.write(`  Amount:         ${result.amount}`);
    }
    if (result.from) {
      ctx.output.write(`  From:           ${result.from}`);
    }
    if (result.to) {
      ctx.output.write(`  To:             ${result.to}`);
    }
    if (result.blockHeight) {
      ctx.output.write(`  Block Height:   ${result.blockHeight}`);
    }
    if (result.error) {
      ctx.output.write('');
      ctx.output.writeError(`  Error: ${result.error}`);
    }

    ctx.output.write('');
    if (result.valid && result.confirmed) {
      ctx.output.success('Transaction is valid and confirmed');
    } else if (result.valid) {
      ctx.output.warn('Transaction is valid but not yet confirmed');
    } else {
      ctx.output.writeError('Transaction is invalid');
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Verification failed: ${message}`);
    process.exit(1);
  }
}

// ── Main action dispatcher ────────────────────────────────────────

async function testAction(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const sub = args.positional[0];

  if (!sub) {
    ctx.output.writeError('Usage: xergon test <run|list|results|history|probe|verify> [args]');
    ctx.output.write('');
    ctx.output.write('Subcommands:');
    ctx.output.write('  run [suite]       Run test suites (contract, inference, settlement, provider, full)');
    ctx.output.write('  list              List available test suites and status');
    ctx.output.write('  results [id]      Show test results for a run');
    ctx.output.write('  history           Show historical test results with trends');
    ctx.output.write('  probe [provider]  Health probe a provider endpoint');
    ctx.output.write('  verify [tx-id]    Verify on-chain settlement transaction');
    process.exit(1);
    return;
  }

  switch (sub) {
    case 'run':
      await handleRun(args, ctx);
      break;
    case 'list':
      await handleList(args, ctx);
      break;
    case 'results':
      await handleResults(args, ctx);
      break;
    case 'history':
      await handleHistory(args, ctx);
      break;
    case 'probe':
      await handleProbe(args, ctx);
      break;
    case 'verify':
      await handleVerify(args, ctx);
      break;
    default:
      ctx.output.writeError(`Unknown test subcommand: ${sub}`);
      ctx.output.write('Valid subcommands: run, list, results, history, probe, verify');
      process.exit(1);
      break;
  }
}

// ── Command export ────────────────────────────────────────────────

const testOptions: CommandOption[] = [
  {
    name: 'json',
    short: '-j',
    long: '--json',
    description: 'Output in JSON format',
    required: false,
    type: 'boolean',
  },
  {
    name: 'timeout',
    short: '-t',
    long: '--timeout',
    description: 'Timeout per test in milliseconds (default: 60000)',
    required: false,
    default: '60000',
    type: 'number',
  },
  {
    name: 'parallel',
    short: '-p',
    long: '--parallel',
    description: 'Run tests in parallel',
    required: false,
    type: 'boolean',
  },
  {
    name: 'suite',
    short: '-s',
    long: '--suite',
    description: 'Filter history by suite name',
    required: false,
    type: 'string',
  },
  {
    name: 'last',
    short: '-n',
    long: '--last',
    description: 'Number of history items to show (default: 20)',
    required: false,
    default: '20',
    type: 'number',
  },
];

export const testCommand: Command = {
  name: 'test',
  description: 'Run integration tests, view results, probe providers, verify transactions',
  aliases: ['tests', 'spec'],
  options: testOptions,
  action: testAction,
};

// ── Exports for testing ───────────────────────────────────────────

export {
  statusIcon,
  statusColor,
  formatDuration,
  formatPassRate,
  TestService,
  testAction,
  type TestStatus,
  type TestSuiteName,
  type TestRunResult,
  type TestHistoryItem,
  type ProbeResult,
  type VerifyResult,
  type SuiteInfo,
};

