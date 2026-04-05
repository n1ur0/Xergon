/**
 * Transaction History API types and fetch helpers.
 *
 * Shows a user's on-chain inference payment history by scanning for
 * staking box changes and settlement transactions.
 */

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface OnChainTransaction {
  id: string;
  txId: string;
  type: 'staking' | 'settlement' | 'inference_payment' | 'reward';
  amountNanoerg: number;
  amountErg: number;
  status: 'confirmed' | 'pending' | 'failed';
  timestamp: string;
  blockHeight: number;
  confirmations: number;
  counterpart?: string;
  model?: string;
  description: string;
}

export interface TxSummary {
  totalSpent: number;
  totalEarned: number;
  totalTransactions: number;
  pendingCount: number;
  firstTxDate: string | null;
  lastTxDate: string | null;
}

export interface TransactionsResponse {
  transactions: OnChainTransaction[];
  summary: TxSummary;
  page: number;
  limit: number;
  totalPages: number;
}

// ---------------------------------------------------------------------------
// API helper
// ---------------------------------------------------------------------------

/**
 * Fetch user transaction history from the relay proxy.
 *
 * Falls back gracefully when the relay does not have a dedicated
 * transactions endpoint (the server route generates mock data).
 */
export async function fetchUserTransactions(
  address: string,
  page = 1,
  limit = 20,
): Promise<TransactionsResponse> {
  const params = new URLSearchParams({
    address,
    page: String(page),
    limit: String(limit),
  });

  const res = await fetch(`/api/xergon-relay/transactions?${params}`);

  if (!res.ok) {
    throw new Error(`Failed to fetch transactions: ${res.status}`);
  }

  return res.json() as Promise<TransactionsResponse>;
}

// ---------------------------------------------------------------------------
// Client-side helpers
// ---------------------------------------------------------------------------

export function formatNanoerg(nanoerg: number): string {
  const erg = nanoerg / 1e9;
  if (erg >= 1) return `${erg.toFixed(4)} ERG`;
  if (erg >= 0.001) return `${erg.toFixed(6)} ERG`;
  return `${nanoerg} nanoERG`;
}

export function truncateTxId(txId: string): string {
  if (txId.length <= 16) return txId;
  return `${txId.slice(0, 10)}...${txId.slice(-4)}`;
}

export function explorerUrl(txId: string): string {
  return `https://explorer.ergoplatform.com/transactions/${txId}`;
}

export function timeAgo(iso: string): string {
  const diff = Date.now() - new Date(iso).getTime();
  const mins = Math.floor(diff / 60000);
  if (mins < 1) return 'just now';
  if (mins < 60) return `${mins}m ago`;
  const hours = Math.floor(mins / 60);
  if (hours < 24) return `${hours}h ago`;
  const days = Math.floor(hours / 24);
  if (days < 30) return `${days}d ago`;
  const months = Math.floor(days / 30);
  return `${months}mo ago`;
}
