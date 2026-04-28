import { NextResponse } from 'next/server';

import { RELAY_BASE } from "@/lib/api/server-sdk";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface OnChainTransaction {
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

interface TxSummary {
  totalSpent: number;
  totalEarned: number;
  totalTransactions: number;
  pendingCount: number;
  firstTxDate: string | null;
  lastTxDate: string | null;
}

// ---------------------------------------------------------------------------
// Mock data generator
// ---------------------------------------------------------------------------

function generateMockTransactions(
  address: string,
  page: number,
  limit: number,
): { transactions: OnChainTransaction[]; summary: TxSummary; totalPages: number } {
  const now = Date.now();
  const totalMock = 47; // total mock transactions available

  const types: Array<'staking' | 'settlement' | 'inference_payment' | 'reward'> = [
    'staking', 'settlement', 'inference_payment', 'reward',
  ];
  const statuses: Array<'confirmed' | 'pending' | 'failed'> = [
    'confirmed', 'confirmed', 'confirmed', 'confirmed', 'confirmed',
    'pending', 'confirmed', 'confirmed', 'failed', 'confirmed',
  ];
  const models = [
    'llama-3.1-70b', 'qwen2.5-72b', 'mistral-7b', 'deepseek-coder-33b',
    'gemma-2-27b', 'phi-3-medium', 'codestral-22b', 'yi-1.5-34b',
  ];

  // Seed a deterministic sequence based on address hash
  let seed = 0;
  for (let i = 0; i < address.length; i++) seed = (seed * 31 + address.charCodeAt(i)) | 0;

  function pseudoRandom(): number {
    seed = (seed * 16807 + 12345) & 0x7fffffff;
    return seed / 0x7fffffff;
  }

  // Generate all mock transactions sorted by timestamp descending
  const all: OnChainTransaction[] = Array.from({ length: totalMock }, (_, i) => {
    const txType = types[Math.floor(pseudoRandom() * types.length)];
    const status = statuses[Math.floor(pseudoRandom() * statuses.length)];
    const amountNanoerg = Math.floor(50_000_000 + pseudoRandom() * 5_000_000_000);
    const hoursAgo = i * 6 + Math.floor(pseudoRandom() * 4);
    const timestamp = new Date(now - hoursAgo * 3600_000).toISOString();
    const blockHeight = 1_200_000 - i * 100 - Math.floor(pseudoRandom() * 50);
    const confirmations = status === 'confirmed' ? 10 + Math.floor(pseudoRandom() * 900) : 0;

    let description: string;
    let counterpart: string | undefined;
    let model: string | undefined;

    switch (txType) {
      case 'staking':
        description = 'Staking box deposit';
        counterpart = `9${Array.from({ length: 9 }, () => Math.floor(pseudoRandom() * 10)).join('')}`;
        break;
      case 'settlement':
        description = 'Provider settlement payment';
        counterpart = `3${Array.from({ length: 9 }, () => Math.floor(pseudoRandom() * 10)).join('')}`;
        break;
      case 'inference_payment':
        model = models[Math.floor(pseudoRandom() * models.length)];
        description = `Inference payment for ${model}`;
        counterpart = `9${Array.from({ length: 9 }, () => Math.floor(pseudoRandom() * 10)).join('')}`;
        break;
      case 'reward':
        description = 'PoNW reward distribution';
        break;
    }

    return {
      id: `mock-tx-${i}-${address.slice(0, 8)}`,
      txId: Array.from({ length: 64 }, () => '0123456789abcdef'[Math.floor(pseudoRandom() * 16)]).join(''),
      type: txType,
      amountNanoerg,
      amountErg: amountNanoerg / 1e9,
      status,
      timestamp,
      blockHeight,
      confirmations,
      counterpart,
      model,
      description,
    };
  });

  // Sort newest first
  all.sort((a, b) => new Date(b.timestamp).getTime() - new Date(a.timestamp).getTime());

  // Paginate
  const totalPages = Math.ceil(all.length / limit);
  const start = (page - 1) * limit;
  const pageItems = all.slice(start, start + limit);

  // Compute summary from all transactions
  const earnedTypes = new Set(['settlement', 'reward']);
  const spentTypes = new Set(['inference_payment', 'staking']);
  let totalSpent = 0;
  let totalEarned = 0;
  let pendingCount = 0;

  for (const tx of all) {
    if (earnedTypes.has(tx.type)) totalEarned += tx.amountNanoerg;
    if (spentTypes.has(tx.type)) totalSpent += tx.amountNanoerg;
    if (tx.status === 'pending') pendingCount++;
  }

  const summary: TxSummary = {
    totalSpent,
    totalEarned,
    totalTransactions: all.length,
    pendingCount,
    firstTxDate: all.length > 0 ? all[all.length - 1].timestamp : null,
    lastTxDate: all.length > 0 ? all[0].timestamp : null,
  };

  return { transactions: pageItems, summary, totalPages };
}

// ---------------------------------------------------------------------------
// GET handler
// ---------------------------------------------------------------------------

export async function GET(request: Request) {
  try {
    const { searchParams } = new URL(request.url);
    const address = searchParams.get('address');
    const page = Math.max(1, parseInt(searchParams.get('page') ?? '1', 10) || 1);
    const limit = Math.min(100, Math.max(1, parseInt(searchParams.get('limit') ?? '20', 10) || 20));

    if (!address) {
      return NextResponse.json(
        { error: 'Missing required parameter: address' },
        { status: 400 },
      );
    }

    // Try the relay first
    const controller = new AbortController();
    const timeout = setTimeout(() => controller.abort(), 5000);

    let relayOk = false;
    try {
      const relayParams = new URLSearchParams({ address, page: String(page), limit: String(limit) });
      const res = await fetch(
        `${RELAY_BASE}/v1/transactions?${relayParams}`,
        { signal: controller.signal },
      );

      clearTimeout(timeout);

      if (res.ok) {
        const data = await res.json();
        // If relay returns a well-formed response, use it
        if (data?.transactions && Array.isArray(data.transactions)) {
          return NextResponse.json({
            ...data,
            page,
            limit,
            totalPages: data.totalPages ?? Math.ceil((data.totalTransactions ?? data.transactions.length) / limit),
          });
        }
      }
    } catch {
      // Relay unavailable — fall through to mock
    } finally {
      clearTimeout(timeout);
    }

    // Also try fetching settlements from xergon-agent and merge
    let agentSettlements: OnChainTransaction[] = [];
    try {
      const agentRes = await fetch(
        `${RELAY_BASE}/xergon/dashboard`,
        { signal: AbortSignal.timeout(3000) },
      );
      if (agentRes.ok) {
        const agentData = await agentRes.json();
        const settlements = agentData?.settlements ?? [];
        agentSettlements = settlements.map(
          (s: { id: string; tx_id: string; amount_nanoerg: number; amount_erg: number; status: string; created_at: string; confirmed_at: string | null }) => ({
            id: s.id,
            txId: s.tx_id,
            type: 'settlement' as const,
            amountNanoerg: s.amount_nanoerg,
            amountErg: s.amount_erg,
            status: (s.status === 'pending' || s.status === 'confirmed' || s.status === 'failed')
              ? s.status
              : 'confirmed',
            timestamp: s.created_at,
            blockHeight: 0,
            confirmations: s.confirmed_at ? 10 : 0,
            description: 'Provider settlement payment',
          }),
        );
      }
    } catch {
      // Agent not reachable — use mock only
    }

    // Use mock data as the primary source (supplemented by agent settlements)
    const mock = generateMockTransactions(address, page, limit);

    // If we got real settlements, prepend them to the first page
    if (agentSettlements.length > 0 && page === 1) {
      const existingIds = new Set(mock.transactions.map((t) => t.txId));
      const uniqueSettlements = agentSettlements.filter((t) => !existingIds.has(t.txId));
      mock.transactions = [...uniqueSettlements, ...mock.transactions].slice(0, limit);
      mock.summary.totalTransactions += uniqueSettlements.length;
    }

    return NextResponse.json({
      ...mock,
      page,
      limit,
      degraded: true,
    });
  } catch (err) {
    return NextResponse.json(
      { error: err instanceof Error ? err.message : 'Internal server error' },
      { status: 500 },
    );
  }
}
