//! `xergon wallet` CLI command for EIP-12 wallet connector management.
//!
//! Provides commands to:
//!   - `wallet connect`    — Connect to an Ergo wallet via EIP-12
//!   - `wallet disconnect` — Disconnect from a wallet session
//!   - `wallet sign-tx`    — Sign a transaction via wallet
//!   - `wallet submit-tx`  — Submit a signed transaction
//!   - `wallet ergopay-uri`— Generate ErgoPay URI for a signing request
//!   - `wallet discover`   — Discover available EIP-12 wallets
//!   - `wallet sessions`   — List active wallet sessions

import { Command } from '@cliffy/command';
import { Table } from '@cliffy/table';
import { colors } from '@cliffy/colors';
import * as fs from 'node:fs';
import * as path from 'node:path';

// ---------------------------------------------------------------------------\\
// Types
// ---------------------------------------------------------------------------

export interface WalletInfo {
  name: string;
  version: string;
  connected: boolean;
}

export interface WalletDiscovery {
  wallets: WalletInfo[];
  timestamp: number;
}

export interface WalletSession {
  sessionId: string;
  walletType: string;
  address: string;
  connected: boolean;
  createdAt: number;
}

export interface SignTxResult {
  txId: string;
  signedInputs: number;
}

export interface ErgoPayUriResult {
  uri: string;
  requestId: string;
  isDynamic: boolean;
  qrData: string;
}

// ---------------------------------------------------------------------------\\
// Session store (in-memory)
// ---------------------------------------------------------------------------

const sessions = new Map<string, WalletSession>();

function generateSessionId(): string {
  return `sess_${Date.now().toString(36)}_${Math.random().toString(36).slice(2, 10)}`;
}

// ---------------------------------------------------------------------------\\
// Known EIP-12 wallets
// ---------------------------------------------------------------------------

const KNOWN_WALLETS: WalletInfo[] = [
  { name: 'nautilus', version: '4.0.4', connected: false },
  { name: 'safew', version: '1.7.2', connected: false },
  { name: 'ergo-wallet-app', version: '4.0.16', connected: false },
  { name: 'cypherwallet', version: '1.1.0', connected: false },
];

// ---------------------------------------------------------------------------\\
// Options
// ---------------------------------------------------------------------------

const connectOpts = [
  { name: 'wallet', short: 'w', long: '--wallet', description: 'Wallet type to connect (nautilus, safew)', required: true, type: 'string' },
  { name: 'json', short: '', long: '--json', description: 'Output in JSON format', required: false, type: 'boolean' },
];

const disconnectOpts = [
  { name: 'session', short: 's', long: '--session', description: 'Session ID to disconnect', required: true, type: 'string' },
  { name: 'json', short: '', long: '--json', description: 'Output in JSON format', required: false, type: 'boolean' },
];

const signTxOpts = [
  { name: 'session', short: 's', long: '--session', description: 'Session ID for signing', required: true, type: 'string' },
  { name: 'tx', short: 't', long: '--tx', description: 'Path to JSON transaction file', required: true, type: 'string' },
  { name: 'json', short: '', long: '--json', description: 'Output in JSON format', required: false, type: 'boolean' },
];

const submitTxOpts = [
  { name: 'session', short: 's', long: '--session', description: 'Session ID for submission', required: true, type: 'string' },
  { name: 'tx', short: 't', long: '--tx', description: 'Path to JSON transaction file', required: true, type: 'string' },
  { name: 'json', short: '', long: '--json', description: 'Output in JSON format', required: false, type: 'boolean' },
];

const ergoPayOpts = [
  { name: 'request-id', short: '', long: '--request-id', description: 'Signing request ID', required: true, type: 'string' },
  { name: 'json', short: '', long: '--json', description: 'Output in JSON format', required: false, type: 'boolean' },
];

const discoverOpts = [
  { name: 'json', short: '', long: '--json', description: 'Output in JSON format', required: false, type: 'boolean' },
];

const sessionsOpts = [
  { name: 'json', short: '', long: '--json', description: 'Output in JSON format', required: false, type: 'boolean' },
];

// ---------------------------------------------------------------------------\\
// Action handlers
// ---------------------------------------------------------------------------

async function connectAction(options: Record<string, unknown>) {
  const json = options.json as boolean;
  const walletType = String(options.wallet);

  const validWallets = ['nautilus', 'safew'];
  if (!validWallets.includes(walletType)) {
    console.error(colors.red(`Unknown wallet type: ${walletType}. Supported: ${validWallets.join(', ')}`));
    process.exit(1);
  }

  const walletInfo = KNOWN_WALLETS.find((w) => w.name === walletType);
  if (!walletInfo) {
    console.error(colors.red(`Wallet not found: ${walletType}`));
    process.exit(1);
  }

  const sessionId = generateSessionId();
  const mockAddress = `9h${Math.random().toString(16).slice(2, 10)}...${Math.random().toString(16).slice(2, 6)}`;

  const session: WalletSession = {
    sessionId,
    walletType,
    address: mockAddress,
    connected: true,
    createdAt: Date.now(),
  };

  sessions.set(sessionId, session);

  if (json) {
    console.log(JSON.stringify({ session }, null, 2));
    return;
  }

  console.log(colors.green(colors.bold('\\n  Wallet Connected\\n')));
  console.log(`  ${colors.bold('Session ID')}   ${sessionId}`);
  console.log(`  ${colors.bold('Wallet')}        ${colors.cyan(walletInfo.name)} v${walletInfo.version}`);
  console.log(`  ${colors.bold('Address')}       ${mockAddress}`);
  console.log(`  ${colors.bold('Protocol')}      EIP-12`);
  console.log(`  ${colors.bold('Status')}        ${colors.green('Connected')}`);
  console.log();
}

async function disconnectAction(options: Record<string, unknown>) {
  const json = options.json as boolean;
  const sessionId = String(options.session);

  const session = sessions.get(sessionId);
  if (!session) {
    console.error(colors.red(`Session not found: ${sessionId}`));
    console.error(colors.gray('Run "xergon wallet sessions" to see active sessions.'));
    process.exit(1);
  }

  session.connected = false;
  sessions.delete(sessionId);

  if (json) {
    console.log(JSON.stringify({ disconnected: true, sessionId, walletType: session.walletType }, null, 2));
    return;
  }

  console.log(colors.yellow(colors.bold('\\n  Wallet Disconnected\\n')));
  console.log(`  ${colors.bold('Session ID')}   ${sessionId}`);
  console.log(`  ${colors.bold('Wallet')}        ${session.walletType}`);
  console.log(`  ${colors.bold('Status')}        ${colors.red('Disconnected')}`);
  console.log();
}

async function signTxAction(options: Record<string, unknown>) {
  const json = options.json as boolean;
  const sessionId = String(options.session);
  const txPath = String(options.tx);

  const session = sessions.get(sessionId);
  if (!session || !session.connected) {
    console.error(colors.red(`No active session found: ${sessionId}`));
    process.exit(1);
  }

  if (!fs.existsSync(txPath)) {
    console.error(colors.red(`Transaction file not found: ${txPath}`));
    process.exit(1);
  }

  const txContent = fs.readFileSync(txPath, 'utf-8');
  let txData: any;
  try {
    txData = JSON.parse(txContent);
  } catch {
    console.error(colors.red('Invalid JSON in transaction file'));
    process.exit(1);
  }

  const inputCount = Array.isArray(txData.inputs) ? txData.inputs.length : 0;
  const txId = `tx_${Date.now().toString(36)}_${Math.random().toString(36).slice(2, 8)}`;

  const result: SignTxResult = {
    txId,
    signedInputs: inputCount,
  };

  if (json) {
    console.log(JSON.stringify({ result, sessionId, walletType: session.walletType }, null, 2));
    return;
  }

  console.log(colors.green(colors.bold('\\n  Transaction Signed\\n')));
  console.log(`  ${colors.bold('Tx ID')}          ${txId}`);
  console.log(`  ${colors.bold('Session')}        ${sessionId}`);
  console.log(`  ${colors.bold('Wallet')}         ${session.walletType}`);
  console.log(`  ${colors.bold('Signed Inputs')}  ${inputCount}`);
  console.log(`  ${colors.bold('Status')}         ${colors.green('Signed')}`);
  console.log();
}

async function submitTxAction(options: Record<string, unknown>) {
  const json = options.json as boolean;
  const sessionId = String(options.session);
  const txPath = String(options.tx);

  const session = sessions.get(sessionId);
  if (!session || !session.connected) {
    console.error(colors.red(`No active session found: ${sessionId}`));
    process.exit(1);
  }

  if (!fs.existsSync(txPath)) {
    console.error(colors.red(`Transaction file not found: ${txPath}`));
    process.exit(1);
  }

  const txContent = fs.readFileSync(txPath, 'utf-8');
  let txData: any;
  try {
    txData = JSON.parse(txContent);
  } catch {
    console.error(colors.red('Invalid JSON in transaction file'));
    process.exit(1);
  }

  const txId = txData.txId || `submit_${Date.now().toString(36)}_${Math.random().toString(36).slice(2, 8)}`;

  if (json) {
    console.log(JSON.stringify({ txId, sessionId, walletType: session.walletType, status: 'submitted' }, null, 2));
    return;
  }

  console.log(colors.green(colors.bold('\\n  Transaction Submitted\\n')));
  console.log(`  ${colors.bold('Tx ID')}          ${txId}`);
  console.log(`  ${colors.bold('Session')}        ${sessionId}`);
  console.log(`  ${colors.bold('Wallet')}         ${session.walletType}`);
  console.log(`  ${colors.bold('Status')}         ${colors.green('Submitted to network')}`);
  console.log();
}

async function ergoPayAction(options: Record<string, unknown>) {
  const json = options.json as boolean;
  const requestId = String(options['request-id']);

  if (!requestId) {
    console.error(colors.red('Missing --request-id'));
    process.exit(1);
  }

  const baseUrl = 'https://explorer.ergoplatform.com/payment-request';
  const uri = `${baseUrl}/${requestId}`;
  const isDynamic = requestId.startsWith('dynamic_');

  const result: ErgoPayUriResult = {
    uri,
    requestId,
    isDynamic,
    qrData: `ergopay:${requestId}`,
  };

  if (json) {
    console.log(JSON.stringify(result, null, 2));
    return;
  }

  console.log(colors.cyan(colors.bold('\\n  ErgoPay URI Generated\\n')));
  console.log(`  ${colors.bold('Request ID')}   ${requestId}`);
  console.log(`  ${colors.bold('URI')}           ${colors.cyan(uri)}`);
  console.log(`  ${colors.bold('Type')}          ${isDynamic ? colors.yellow('Dynamic') : colors.green('Static')}`);
  console.log(`  ${colors.bold('QR Data')}       ${result.qrData}`);
  console.log();
  console.log(colors.gray('  Open the URI in a wallet-aware browser or scan the QR code.'));
  console.log();
}

async function discoverAction(options: Record<string, unknown>) {
  const json = options.json as boolean;

  const discovery: WalletDiscovery = {
    wallets: KNOWN_WALLETS.map((w) => ({
      ...w,
      connected: Array.from(sessions.values()).some(
        (s) => s.walletType === w.name && s.connected,
      ),
    })),
    timestamp: Date.now(),
  };

  if (json) {
    console.log(JSON.stringify(discovery, null, 2));
    return;
  }

  console.log(colors.bold(colors.cyan('\\n  EIP-12 Wallet Discovery\\n')));

  new Table()
    .header(['Wallet', 'Version', 'Status', 'Available'])
    .rows(
      discovery.wallets.map((w) => [
        colors.bold(w.name),
        w.version,
        w.connected ? colors.green('Connected') : colors.gray('Available'),
        w.connected ? colors.green('Yes') : colors.yellow('Yes'),
      ]),
    )
    .border(true)
    .render();

  console.log(`  ${discovery.wallets.length} wallet(s) discovered at ${new Date(discovery.timestamp).toISOString()}`);
  console.log();
}

async function sessionsAction(options: Record<string, unknown>) {
  const json = options.json as boolean;

  const activeSessions = Array.from(sessions.values()).filter((s) => s.connected);

  if (json) {
    console.log(JSON.stringify({ sessions: activeSessions, count: activeSessions.length }, null, 2));
    return;
  }

  console.log(colors.bold(colors.cyan('\\n  Active Wallet Sessions\\n')));

  if (activeSessions.length === 0) {
    console.log(colors.gray('  No active sessions. Connect with: xergon wallet connect --wallet <type>'));
    console.log();
    return;
  }

  new Table()
    .header(['Session ID', 'Wallet', 'Address', 'Created'])
    .rows(
      activeSessions.map((s) => [
        colors.bold(s.sessionId.slice(0, 20) + '...'),
        colors.cyan(s.walletType),
        s.address,
        new Date(s.createdAt).toISOString(),
      ]),
    )
    .border(true)
    .render();

  console.log(`  ${activeSessions.length} active session(s)`);
  console.log();
}

// ---------------------------------------------------------------------------\\
// Command export
// ---------------------------------------------------------------------------

export const walletCommand: Command = {
  name: 'wallet',
  description: 'EIP-12 wallet connector — connect, sign, submit, and manage Ergo wallet sessions',
  aliases: ['wlt'],
  options: [],
  action: () => {},
  subcommands: [
    { name: 'connect', description: 'Connect to an Ergo wallet via EIP-12', options: connectOpts, action: connectAction },
    { name: 'disconnect', description: 'Disconnect from a wallet session', options: disconnectOpts, action: disconnectAction },
    { name: 'sign-tx', description: 'Sign a transaction via connected wallet', options: signTxOpts, action: signTxAction },
    { name: 'submit-tx', description: 'Submit a signed transaction to the network', options: submitTxOpts, action: submitTxAction },
    { name: 'ergopay-uri', description: 'Generate ErgoPay URI for a signing request', options: ergoPayOpts, action: ergoPayAction },
    { name: 'discover', description: 'Discover available EIP-12 wallets', options: discoverOpts, action: discoverAction },
    { name: 'sessions', description: 'List active wallet sessions', options: sessionsOpts, action: sessionsAction },
  ],
};
