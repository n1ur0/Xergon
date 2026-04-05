import Link from "next/link";
import { SuspenseWrap } from "@/components/ui/SuspenseWrap";

// ---------------------------------------------------------------------------
// Mock data
// ---------------------------------------------------------------------------

const MOCK_STATS = {
  totalCommitments: 1247,
  proofsBatched: 89432,
  utxoReduction: 94.2,
  lastBatch: "12 min ago",
} as const;

interface CommitmentRow {
  batchId: string;
  proofs: number;
  merkleRoot: string;
  timestamp: string;
  txId: string;
}

const MOCK_COMMITMENTS: CommitmentRow[] = [
  { batchId: "cb-001247", proofs: 72, merkleRoot: "a3f8c1e2d4b5...", timestamp: "2026-04-04 23:42", txId: "7e2a1b9c3d4f5a6b8c0d1e2f3a4b5c6d7e8f9a0b1c2d3e4f5a6b7c8d9e0f1a2b" },
  { batchId: "cb-001246", proofs: 64, merkleRoot: "b7d2e5f8a1c3...", timestamp: "2026-04-04 22:58", txId: "1f2e3d4c5b6a7f8e9d0c1b2a3f4e5d6c7b8a9f0e1d2c3b4a5f6e7d8c9b0a1f2e" },
  { batchId: "cb-001245", proofs: 81, merkleRoot: "c1e4b7a9d2f5...", timestamp: "2026-04-04 22:14", txId: "3a4b5c6d7e8f9a0b1c2d3e4f5a6b7c8d9e0f1a2b3c4d5e6f7a8b9c0d1e2f3a4b" },
  { batchId: "cb-001244", proofs: 55, merkleRoot: "d5f8c2e6a1b3...", timestamp: "2026-04-04 21:30", txId: "5c6d7e8f9a0b1c2d3e4f5a6b7c8d9e0f1a2b3c4d5e6f7a8b9c0d1e2f3a4b5c6d" },
  { batchId: "cb-001243", proofs: 93, merkleRoot: "e9a3b6c8d1f4...", timestamp: "2026-04-04 20:46", txId: "7e8f9a0b1c2d3e4f5a6b7c8d9e0f1a2b3c4d5e6f7a8b9c0d1e2f3a4b5c6d7e8f" },
  { batchId: "cb-001242", proofs: 68, merkleRoot: "f1b4c7d9e2a5...", timestamp: "2026-04-04 20:02", txId: "9a0b1c2d3e4f5a6b7c8d9e0f1a2b3c4d5e6f7a8b9c0d1e2f3a4b5c6d7e8f9a0b" },
  { batchId: "cb-001241", proofs: 77, merkleRoot: "a6c9d2e5f8b1...", timestamp: "2026-04-04 19:18", txId: "b1c2d3e4f5a6b7c8d9e0f1a2b3c4d5e6f7a8b9c0d1e2f3a4b5c6d7e8f9a0b1c2" },
  { batchId: "cb-001240", proofs: 60, merkleRoot: "b3d6e9f2a5c8...", timestamp: "2026-04-04 18:34", txId: "c2d3e4f5a6b7c8d9e0f1a2b3c4d5e6f7a8b9c0d1e2f3a4b5c6d7e8f9a0b1c2d3" },
  { batchId: "cb-001239", proofs: 85, merkleRoot: "c8e1f4a7b9d2...", timestamp: "2026-04-04 17:50", txId: "d3e4f5a6b7c8d9e0f1a2b3c4d5e6f7a8b9c0d1e2f3a4b5c6d7e8f9a0b1c2d3e4" },
  { batchId: "cb-001238", proofs: 71, merkleRoot: "d2f5a8c1e4b7...", timestamp: "2026-04-04 17:06", txId: "e4f5a6b7c8d9e0f1a2b3c4d5e6f7a8b9c0d1e2f3a4b5c6d7e8f9a0b1c2d3e4f5" },
];

// ---------------------------------------------------------------------------
// Sub-components
// ---------------------------------------------------------------------------

function StatCard({
  label,
  value,
  sub,
}: {
  label: string;
  value: string;
  sub?: string;
}) {
  return (
    <div className="rounded-xl border border-surface-200 bg-surface-0 p-5">
      <p className="text-xs text-surface-800/50 mb-1">{label}</p>
      <p className="text-2xl font-bold text-surface-900">{value}</p>
      {sub && (
        <p className="text-xs text-surface-800/40 mt-1">{sub}</p>
      )}
    </div>
  );
}

function truncateTxId(txId: string): string {
  if (txId.length <= 20) return txId;
  return `${txId.slice(0, 10)}...${txId.slice(-6)}`;
}

function ExplorerLink({ txId }: { txId: string }) {
  return (
    <Link
      href={`https://explorer.ergoplatform.com/en/transactions/${txId}`}
      target="_blank"
      rel="noopener noreferrer"
      className="font-mono text-xs text-brand-600 hover:text-brand-700 hover:underline transition-colors"
    >
      {truncateTxId(txId)}
    </Link>
  );
}

// ---------------------------------------------------------------------------
// Main page
// ---------------------------------------------------------------------------

export default function CommitmentsPage() {
  return (
    <div className="max-w-6xl mx-auto px-4 py-8">
      {/* Header */}
      <div className="mb-6">
        <h1 className="text-2xl font-bold text-surface-900">
          Usage Proof Commitments
        </h1>
        <p className="text-sm text-surface-800/50 mt-0.5">
          Merkle-root batching reduces UTXO bloat from individual usage proofs
        </p>
      </div>

      <SuspenseWrap>
        {/* Architecture diagram */}
        <div className="mb-6 rounded-xl border border-surface-200 bg-surface-0 p-6">
          <h2 className="text-base font-semibold text-surface-900 mb-4">
            Commitment Architecture
          </h2>
          <div className="font-mono text-xs sm:text-sm text-surface-800/80 leading-relaxed bg-surface-50 dark:bg-surface-900/50 rounded-lg p-4 overflow-x-auto">
            <pre className="whitespace-pre">{`[Proof 1] [Proof 2] [Proof 3] ... [Proof N]
    \\       |       /                  |
     \\      |      /                   |
      [blake2b256 Merkle Root]
              |
       [Commitment Box]
    R4: merkle_root
    R5: proof_count
    R6: batch_timestamp
    R7: epoch`}</pre>
          </div>
          <div className="mt-4 grid grid-cols-1 sm:grid-cols-2 gap-3 text-xs text-surface-800/60">
            <div className="flex items-start gap-2">
              <span className="inline-block h-5 w-5 rounded-md bg-brand-100 dark:bg-brand-900/30 text-brand-700 dark:text-brand-400 text-center font-semibold leading-5 shrink-0">1</span>
              <span>Individual usage proof boxes accumulate in the UTXO set</span>
            </div>
            <div className="flex items-start gap-2">
              <span className="inline-block h-5 w-5 rounded-md bg-brand-100 dark:bg-brand-900/30 text-brand-700 dark:text-brand-400 text-center font-semibold leading-5 shrink-0">2</span>
              <span>Agent batches N proofs into a single commitment box</span>
            </div>
            <div className="flex items-start gap-2">
              <span className="inline-block h-5 w-5 rounded-md bg-brand-100 dark:bg-brand-900/30 text-brand-700 dark:text-brand-400 text-center font-semibold leading-5 shrink-0">3</span>
              <span>Commitment box stores merkle root (blake2b256), proof count, timestamp, epoch</span>
            </div>
            <div className="flex items-start gap-2">
              <span className="inline-block h-5 w-5 rounded-md bg-brand-100 dark:bg-brand-900/30 text-brand-700 dark:text-brand-400 text-center font-semibold leading-5 shrink-0">4</span>
              <span>Original proof boxes are consumed — UTXO set is cleaned up</span>
            </div>
          </div>
        </div>

        {/* Stats cards */}
        <div className="grid grid-cols-2 lg:grid-cols-4 gap-4 mb-6">
          <StatCard
            label="Total Commitments"
            value={MOCK_STATS.totalCommitments.toLocaleString()}
            sub="Lifetime batches"
          />
          <StatCard
            label="Proofs Batched"
            value={MOCK_STATS.proofsBatched.toLocaleString()}
            sub="Individual proofs consolidated"
          />
          <StatCard
            label="UTXO Reduction"
            value={`${MOCK_STATS.utxoReduction}%`}
            sub="vs. unbatched proofs"
          />
          <StatCard
            label="Last Batch"
            value={MOCK_STATS.lastBatch}
            sub="Time since last commitment"
          />
        </div>

        {/* Recent commitments table */}
        <div className="rounded-xl border border-surface-200 bg-surface-0 overflow-hidden mb-6">
          <div className="px-5 py-4 border-b border-surface-100">
            <h2 className="text-base font-semibold text-surface-900">
              Recent Commitments
            </h2>
            <p className="text-xs text-surface-800/40 mt-0.5">
              Latest merkle commitment batches on-chain
            </p>
          </div>
          <div className="overflow-x-auto">
            <table className="w-full text-sm">
              <thead>
                <tr className="text-left text-xs text-surface-800/40 border-b border-surface-100">
                  <th className="px-5 py-2.5 font-medium">Batch ID</th>
                  <th className="px-3 py-2.5 font-medium text-right">Proofs</th>
                  <th className="px-3 py-2.5 font-medium">Merkle Root</th>
                  <th className="px-3 py-2.5 font-medium">Timestamp</th>
                  <th className="px-5 py-2.5 font-medium">TX ID</th>
                </tr>
              </thead>
              <tbody>
                {MOCK_COMMITMENTS.map((row) => (
                  <tr
                    key={row.batchId}
                    className="border-b border-surface-50 hover:bg-surface-50 dark:hover:bg-surface-900/30 transition-colors"
                  >
                    <td className="px-5 py-2.5 font-mono text-xs text-surface-900">
                      {row.batchId}
                    </td>
                    <td className="px-3 py-2.5 text-right">
                      <span className="inline-flex items-center px-2 py-0.5 rounded-full bg-brand-50 dark:bg-brand-900/20 text-brand-700 dark:text-brand-400 text-xs font-medium">
                        {row.proofs}
                      </span>
                    </td>
                    <td className="px-3 py-2.5 font-mono text-xs text-surface-800/60">
                      {row.merkleRoot}
                    </td>
                    <td className="px-3 py-2.5 text-xs text-surface-800/60 whitespace-nowrap">
                      {row.timestamp}
                    </td>
                    <td className="px-5 py-2.5">
                      <ExplorerLink txId={row.txId} />
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          <div className="px-5 py-3 border-t border-surface-100 text-xs text-surface-800/30">
            Mock data — real on-chain data coming soon.
          </div>
        </div>

        {/* How it works */}
        <div className="rounded-xl border border-surface-200 bg-surface-0 p-6">
          <h2 className="text-base font-semibold text-surface-900 mb-4">
            How It Works
          </h2>
          <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
            <div className="flex items-start gap-3">
              <div className="flex-shrink-0 h-8 w-8 rounded-lg bg-emerald-100 dark:bg-emerald-900/30 text-emerald-700 dark:text-emerald-400 flex items-center justify-center text-sm font-bold">
                1
              </div>
              <div>
                <h3 className="text-sm font-semibold text-surface-900 mb-1">
                  Proofs Accumulate
                </h3>
                <p className="text-xs text-surface-800/60 leading-relaxed">
                  As agents serve inference requests, each completed session generates a usage proof box in the UTXO set. Over time, these individual boxes grow and bloat the chain state.
                </p>
              </div>
            </div>
            <div className="flex items-start gap-3">
              <div className="flex-shrink-0 h-8 w-8 rounded-lg bg-blue-100 dark:bg-blue-900/30 text-blue-700 dark:text-blue-400 flex items-center justify-center text-sm font-bold">
                2
              </div>
              <div>
                <h3 className="text-sm font-semibold text-surface-900 mb-1">
                  Merkle Aggregation
                </h3>
                <p className="text-xs text-surface-800/60 leading-relaxed">
                  The agent collects N proof boxes and computes a blake2b256 merkle root from all proof hashes. This single hash cryptographically commits to the entire batch of proofs.
                </p>
              </div>
            </div>
            <div className="flex items-start gap-3">
              <div className="flex-shrink-0 h-8 w-8 rounded-lg bg-amber-100 dark:bg-amber-900/30 text-amber-700 dark:text-amber-400 flex items-center justify-center text-sm font-bold">
                3
              </div>
              <div>
                <h3 className="text-sm font-semibold text-surface-900 mb-1">
                  Commitment Box Created
                </h3>
                <p className="text-xs text-surface-800/60 leading-relaxed">
                  A single commitment box is minted with registers R4 (merkle root), R5 (proof count), R6 (batch timestamp), and R7 (epoch). This replaces all N individual proof boxes.
                </p>
              </div>
            </div>
            <div className="flex items-start gap-3">
              <div className="flex-shrink-0 h-8 w-8 rounded-lg bg-purple-100 dark:bg-purple-900/30 text-purple-700 dark:text-purple-400 flex items-center justify-center text-sm font-bold">
                4
              </div>
              <div>
                <h3 className="text-sm font-semibold text-surface-900 mb-1">
                  UTXO Set Cleaned
                </h3>
                <p className="text-xs text-surface-800/60 leading-relaxed">
                  The original proof boxes are consumed as inputs in the commitment transaction. The UTXO set shrinks by N-1 boxes per batch, drastically reducing state size and improving node performance.
                </p>
              </div>
            </div>
          </div>
        </div>
      </SuspenseWrap>
    </div>
  );
}
