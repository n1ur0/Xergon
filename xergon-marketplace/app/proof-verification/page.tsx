"use client";

import { useState, useEffect, useMemo, useCallback } from "react";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

type ProofType = "ZK" | "TEE" | "Combined";
type ProofStatus = "verified" | "pending" | "failed";

interface Proof {
  proof_id: string;
  provider: string;
  model: string;
  type: ProofType;
  status: ProofStatus;
  created_at: string;
  anchor_tx: string | null;
  commitment_hash: string;
  verify_time_ms: number;
  verification_result: string;
}

type SortCol = "proof_id" | "provider" | "model" | "type" | "status" | "created_at";

// ---------------------------------------------------------------------------
// Mock data
// ---------------------------------------------------------------------------

const MOCK_PROOFS: Proof[] = [
  { proof_id: "zkp-0x001a", provider: "NeuralForge Alpha", model: "Llama 3.1 70B", type: "ZK", status: "verified", created_at: "2025-11-12T10:30:00Z", anchor_tx: "0xab12...cd34", commitment_hash: "0x7f3a...e9b2", verify_time_ms: 1240, verification_result: "Proof valid: circuit output matches public inputs" },
  { proof_id: "tee-0x002b", provider: "DeepCompute Sigma", model: "Qwen 2.5 72B", type: "TEE", status: "verified", created_at: "2025-11-12T09:45:00Z", anchor_tx: "0xef56...gh78", commitment_hash: "0x2c8d...f1a4", verify_time_ms: 890, verification_result: "SGX quote verified, MRENCLAVE matches expected" },
  { proof_id: "cmb-0x003c", provider: "SafeInfer Gamma", model: "Mistral Large", type: "Combined", status: "verified", created_at: "2025-11-12T08:20:00Z", anchor_tx: "0xij90...kl12", commitment_hash: "0x3a9f...1e5d", verify_time_ms: 2100, verification_result: "ZK proof valid, TEE quote verified" },
  { proof_id: "zkp-0x004d", provider: "EnclaveAI Zeta", model: "DeepSeek V3", type: "ZK", status: "pending", created_at: "2025-11-12T07:55:00Z", anchor_tx: null, commitment_hash: "0x8b2d...4c6a", verify_time_ms: 0, verification_result: "Awaiting verifier assignment" },
  { proof_id: "tee-0x005e", provider: "QuantumEdge Delta", model: "Llama 3.1 8B", type: "TEE", status: "verified", created_at: "2025-11-12T07:10:00Z", anchor_tx: "0xmn34...op56", commitment_hash: "0xb4c7...a2f9", verify_time_ms: 760, verification_result: "TDX attestation valid, quote signature verified" },
  { proof_id: "zkp-0x006f", provider: "CipherNode Pro", model: "Phi-3 Medium", type: "ZK", status: "failed", created_at: "2025-11-12T06:30:00Z", anchor_tx: null, commitment_hash: "0x5f2a...d8e1", verify_time_ms: 3400, verification_result: "Proof rejected: public input mismatch at index 42" },
  { proof_id: "cmb-0x007g", provider: "NeuralForge Alpha", model: "Qwen 2.5 7B", type: "Combined", status: "verified", created_at: "2025-11-11T22:15:00Z", anchor_tx: "0xqr78...st90", commitment_hash: "0x1d4e...6f8a", verify_time_ms: 1890, verification_result: "Both ZK and TEE proofs verified successfully" },
  { proof_id: "tee-0x008h", provider: "TrustGrid Omega", model: "Mistral 7B", type: "TEE", status: "pending", created_at: "2025-11-11T21:00:00Z", anchor_tx: null, commitment_hash: "0xd1e8...7b3c", verify_time_ms: 0, verification_result: "Attestation report received, pending verification" },
  { proof_id: "zkp-0x009i", provider: "OpenNet Theta", model: "Llama 3.1 70B", type: "ZK", status: "verified", created_at: "2025-11-11T19:40:00Z", anchor_tx: "0xuv12...wx34", commitment_hash: "0xc5f1...3a8e", verify_time_ms: 1560, verification_result: "Groth16 proof verified on-chain" },
  { proof_id: "cmb-0x010j", provider: "EnclaveAI Zeta", model: "DeepSeek Coder", type: "Combined", status: "verified", created_at: "2025-11-11T18:20:00Z", anchor_tx: "0xyz56...ab78", commitment_hash: "0x9e1b...c3d7", verify_time_ms: 2340, verification_result: "Combined proof batch verified" },
  { proof_id: "tee-0x011k", provider: "VertexMind Beta", model: "Gemma 2 9B", type: "TEE", status: "failed", created_at: "2025-11-11T17:05:00Z", anchor_tx: null, commitment_hash: "0x4e7c...a1f3", verify_time_ms: 4100, verification_result: "SEV attestation expired, quote older than 24h" },
  { proof_id: "zkp-0x012l", provider: "DeepCompute Sigma", model: "CodeLlama 34B", type: "ZK", status: "verified", created_at: "2025-11-11T15:50:00Z", anchor_tx: "0xcd90...ef12", commitment_hash: "0x6a2f...d5b8", verify_time_ms: 980, verification_result: "PLONK proof valid" },
  { proof_id: "tee-0x013m", provider: "SafeInfer Gamma", model: "Llama 3.1 70B", type: "TEE", status: "verified", created_at: "2025-11-11T14:30:00Z", anchor_tx: "0xgh34...ij56", commitment_hash: "0xf8c3...2e7d", verify_time_ms: 820, verification_result: "SGX quote verified with valid enclave identity" },
  { proof_id: "cmb-0x014n", provider: "RawPower Epsilon", model: "Mistral Nemo", type: "Combined", status: "pending", created_at: "2025-11-11T13:15:00Z", anchor_tx: null, commitment_hash: "0x3b9d...8f1c", verify_time_ms: 0, verification_result: "Awaiting TEE component verification" },
  { proof_id: "zkp-0x015o", provider: "QuantumEdge Delta", model: "Phi-3 Small", type: "ZK", status: "verified", created_at: "2025-11-11T12:00:00Z", anchor_tx: "0xkl78...mn90", commitment_hash: "0xe1a5...4b9f", verify_time_ms: 1100, verification_result: "Proof valid, inference hash committed on-chain" },
];

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function statusColor(s: ProofStatus): string {
  if (s === "verified") return "bg-green-100 text-green-700 dark:bg-green-900/30 dark:text-green-400";
  if (s === "pending") return "bg-yellow-100 text-yellow-700 dark:bg-yellow-900/30 dark:text-yellow-400";
  return "bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-400";
}

function typeColor(t: ProofType): string {
  if (t === "ZK") return "bg-purple-100 text-purple-700 dark:bg-purple-900/30 dark:text-purple-400";
  if (t === "TEE") return "bg-blue-100 text-blue-700 dark:bg-blue-900/30 dark:text-blue-400";
  return "bg-brand-100 text-brand-700 dark:bg-brand-900/30 dark:text-brand-400";
}

// ---------------------------------------------------------------------------
// Skeleton
// ---------------------------------------------------------------------------

function TableSkeleton() {
  return (
    <div className="rounded-xl border border-surface-200 bg-surface-0 overflow-hidden animate-pulse">
      <div className="p-4 space-y-3">
        {Array.from({ length: 6 }, (_, i) => (
          <div key={i} className="h-12 rounded-lg bg-surface-200" />
        ))}
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Modal
// ---------------------------------------------------------------------------

function ProofModal({ proof, onClose }: { proof: Proof; onClose: () => void }) {
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/40" onClick={onClose}>
      <div className="bg-surface-0 rounded-2xl border border-surface-200 shadow-xl max-w-lg w-full max-h-[80vh] overflow-y-auto" onClick={(e) => e.stopPropagation()}>
        <div className="flex items-center justify-between p-5 border-b border-surface-100">
          <h2 className="text-lg font-bold text-surface-900">Proof Details</h2>
          <button onClick={onClose} className="p-1 rounded-lg hover:bg-surface-100 dark:hover:bg-surface-800 transition-colors">
            <svg className="w-5 h-5 text-surface-800/50" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <line x1="18" y1="6" x2="6" y2="18" /><line x1="6" y1="6" x2="18" y2="18" />
            </svg>
          </button>
        </div>
        <div className="p-5 space-y-4">
          <div className="grid grid-cols-2 gap-3 text-sm">
            <div>
              <div className="text-xs text-surface-800/40 mb-0.5">Proof ID</div>
              <div className="font-mono text-surface-800/80">{proof.proof_id}</div>
            </div>
            <div>
              <div className="text-xs text-surface-800/40 mb-0.5">Status</div>
              <span className={`inline-block px-2 py-0.5 text-xs rounded-md font-medium ${statusColor(proof.status)}`}>{proof.status}</span>
            </div>
            <div>
              <div className="text-xs text-surface-800/40 mb-0.5">Provider</div>
              <div className="text-surface-800/80">{proof.provider}</div>
            </div>
            <div>
              <div className="text-xs text-surface-800/40 mb-0.5">Model</div>
              <div className="text-surface-800/80">{proof.model}</div>
            </div>
            <div>
              <div className="text-xs text-surface-800/40 mb-0.5">Type</div>
              <span className={`inline-block px-2 py-0.5 text-xs rounded-md font-medium ${typeColor(proof.type)}`}>{proof.type}</span>
            </div>
            <div>
              <div className="text-xs text-surface-800/40 mb-0.5">Created</div>
              <div className="text-surface-800/80">{new Date(proof.created_at).toLocaleString()}</div>
            </div>
          </div>

          <div>
            <div className="text-xs text-surface-800/40 mb-1">Commitment Hash</div>
            <div className="p-2 rounded-lg bg-surface-50 dark:bg-surface-800/50 font-mono text-xs text-surface-800/70 break-all">{proof.commitment_hash}</div>
          </div>

          <div>
            <div className="text-xs text-surface-800/40 mb-1">Verification Result</div>
            <div className={`p-2 rounded-lg text-xs ${
              proof.status === "verified" ? "bg-green-50 dark:bg-green-900/20 text-green-700 dark:text-green-400" :
              proof.status === "failed" ? "bg-red-50 dark:bg-red-900/20 text-red-700 dark:text-red-400" :
              "bg-yellow-50 dark:bg-yellow-900/20 text-yellow-700 dark:text-yellow-400"
            }`}>{proof.verification_result}</div>
          </div>

          {proof.verify_time_ms > 0 && (
            <div>
              <div className="text-xs text-surface-800/40 mb-0.5">Verification Time</div>
              <div className="text-sm text-surface-800/80">{proof.verify_time_ms}ms</div>
            </div>
          )}

          {proof.anchor_tx && (
            <div>
              <div className="text-xs text-surface-800/40 mb-1">On-Chain Anchor</div>
              <a href="#" className="inline-flex items-center gap-1 px-3 py-1.5 rounded-lg bg-surface-50 dark:bg-surface-800/50 text-xs font-mono text-brand-600 hover:bg-surface-100 dark:hover:bg-surface-800 transition-colors">
                <svg className="w-3.5 h-3.5" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                  <path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6" /><polyline points="15 3 21 3 21 9" /><line x1="10" y1="14" x2="21" y2="3" />
                </svg>
                {proof.anchor_tx}
              </a>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Main component
// ---------------------------------------------------------------------------

export default function ProofVerificationPage() {
  const [loading, setLoading] = useState(true);
  const [selectedProof, setSelectedProof] = useState<Proof | null>(null);
  const [filterStatus, setFilterStatus] = useState<ProofStatus | "all">("all");
  const [filterType, setFilterType] = useState<ProofType | "all">("all");
  const [filterProvider, setFilterProvider] = useState("all");
  const [sortCol, setSortCol] = useState<SortCol>("created_at");
  const [sortDir, setSortDir] = useState<"asc" | "desc">("desc");
  const [verifyJson, setVerifyJson] = useState("");
  const [verifyResult, setVerifyResult] = useState<string | null>(null);
  const [verifying, setVerifying] = useState(false);

  useEffect(() => {
    const t = setTimeout(() => setLoading(false), 700);
    return () => clearTimeout(t);
  }, []);

  const uniqueProviders = useMemo(() => [...new Set(MOCK_PROOFS.map((p) => p.provider))], []);

  const filteredProofs = useMemo(() => {
    let list = [...MOCK_PROOFS];
    if (filterStatus !== "all") list = list.filter((p) => p.status === filterStatus);
    if (filterType !== "all") list = list.filter((p) => p.type === filterType);
    if (filterProvider !== "all") list = list.filter((p) => p.provider === filterProvider);
    list.sort((a, b) => {
      const av = a[sortCol];
      const bv = b[sortCol];
      const cmp = String(av).localeCompare(String(bv));
      return sortDir === "asc" ? cmp : -cmp;
    });
    return list;
  }, [filterStatus, filterType, filterProvider, sortCol, sortDir]);

  const stats = useMemo(() => {
    const total = MOCK_PROOFS.length;
    const verified = MOCK_PROOFS.filter((p) => p.status === "verified").length;
    const anchored = MOCK_PROOFS.filter((p) => p.anchor_tx !== null).length;
    const times = MOCK_PROOFS.filter((p) => p.verify_time_ms > 0).map((p) => p.verify_time_ms);
    const avgTime = times.length > 0 ? Math.round(times.reduce((a, b) => a + b, 0) / times.length) : 0;
    return { total, verified, verifiedRate: Math.round((verified / total) * 100), avgTime, anchorRate: Math.round((anchored / total) * 100) };
  }, []);

  const recentActivity = useMemo(() => {
    return MOCK_PROOFS.slice(0, 5).map((p) => ({
      id: p.proof_id,
      provider: p.provider,
      status: p.status,
      time: p.created_at,
    }));
  }, []);

  const handleSort = useCallback((col: SortCol) => {
    setSortCol((prev) => {
      if (prev === col) {
        setSortDir((d) => (d === "asc" ? "desc" : "asc"));
        return prev;
      }
      setSortDir("desc");
      return col;
    });
  }, []);

  const handleVerify = useCallback(() => {
    if (!verifyJson.trim()) return;
    setVerifying(true);
    setVerifyResult(null);
    setTimeout(() => {
      const success = verifyJson.length > 20;
      setVerifyResult(
        success
          ? "Proof verified successfully. Commitment hash: 0x" + Math.random().toString(16).slice(2, 10) + "... Valid circuit output confirmed."
          : "Verification failed: Invalid proof format. Expected JSON with proof, public_inputs, and verification_key fields."
      );
      setVerifying(false);
    }, 1500);
  }, [verifyJson]);

  const SortIcon = ({ col }: { col: SortCol }) => (
    <svg className={`w-3 h-3 inline-block ml-1 ${sortCol === col ? "text-brand-600" : "text-surface-800/20"}`} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5">
      {sortCol === col && sortDir === "asc" ? (
        <polyline points="18 15 12 9 6 15" />
      ) : (
        <polyline points="6 9 12 15 18 9" />
      )}
    </svg>
  );

  return (
    <main className="mx-auto max-w-6xl px-4 py-6 space-y-6">
      {/* Header */}
      <div className="space-y-1">
        <div className="flex items-center gap-3">
          <svg className="w-7 h-7 text-brand-600" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
            <path d="M9 12l2 2 4-4" /><path d="M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
          </svg>
          <h1 className="text-2xl font-bold text-surface-900">Proof Verification</h1>
        </div>
        <p className="text-sm text-surface-800/60">View, filter, and verify ZK and TEE proofs from network providers</p>
      </div>

      {/* Stats */}
      <div className="grid grid-cols-2 sm:grid-cols-4 gap-4">
        {[
          { label: "Total Proofs", value: stats.total, color: "text-surface-900" },
          { label: "Verified Rate", value: `${stats.verifiedRate}%`, color: "text-green-600 dark:text-green-400" },
          { label: "Avg Verify Time", value: `${stats.avgTime}ms`, color: "text-surface-900" },
          { label: "Anchor Rate", value: `${stats.anchorRate}%`, color: "text-blue-600 dark:text-blue-400" },
        ].map((s) => (
          <div key={s.label} className="rounded-xl border border-surface-200 bg-surface-0 p-4">
            <div className="text-xs text-surface-800/50 mb-1">{s.label}</div>
            <div className={`text-xl font-bold ${s.color}`}>{s.value}</div>
          </div>
        ))}
      </div>

      <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
        {/* Table section */}
        <div className="lg:col-span-2 space-y-4">
          {/* Filters */}
          <div className="flex flex-wrap gap-2">
            <select value={filterStatus} onChange={(e) => setFilterStatus(e.target.value as ProofStatus | "all")} className="px-3 py-1.5 text-sm rounded-lg border border-surface-200 bg-surface-0 text-surface-800 focus:outline-none focus:ring-2 focus:ring-brand-500/30">
              <option value="all">All Status</option>
              <option value="verified">Verified</option>
              <option value="pending">Pending</option>
              <option value="failed">Failed</option>
            </select>
            <select value={filterType} onChange={(e) => setFilterType(e.target.value as ProofType | "all")} className="px-3 py-1.5 text-sm rounded-lg border border-surface-200 bg-surface-0 text-surface-800 focus:outline-none focus:ring-2 focus:ring-brand-500/30">
              <option value="all">All Types</option>
              <option value="ZK">ZK</option>
              <option value="TEE">TEE</option>
              <option value="Combined">Combined</option>
            </select>
            <select value={filterProvider} onChange={(e) => setFilterProvider(e.target.value)} className="px-3 py-1.5 text-sm rounded-lg border border-surface-200 bg-surface-0 text-surface-800 focus:outline-none focus:ring-2 focus:ring-brand-500/30">
              <option value="all">All Providers</option>
              {uniqueProviders.map((p) => (
                <option key={p} value={p}>{p}</option>
              ))}
            </select>
          </div>

          {/* Table */}
          {loading ? (
            <TableSkeleton />
          ) : (
            <div className="rounded-xl border border-surface-200 bg-surface-0 overflow-hidden">
              <div className="overflow-x-auto">
                <table className="w-full text-sm">
                  <thead>
                    <tr className="border-b border-surface-100 text-left">
                      {([
                        ["proof_id", "ID"], ["provider", "Provider"], ["model", "Model"],
                        ["type", "Type"], ["status", "Status"], ["created_at", "Created"],
                      ] as [SortCol, string][]).map(([col, label]) => (
                        <th key={col} className="px-4 py-3 text-xs font-semibold text-surface-800/50 uppercase tracking-wide cursor-pointer hover:text-surface-800/70" onClick={() => handleSort(col)}>
                          {label}<SortIcon col={col} />
                        </th>
                      ))}
                    </tr>
                  </thead>
                  <tbody>
                    {filteredProofs.map((proof) => (
                      <tr key={proof.proof_id} className="border-b border-surface-50 last:border-0 cursor-pointer hover:bg-surface-50 dark:hover:bg-surface-800/50 transition-colors" onClick={() => setSelectedProof(proof)}>
                        <td className="px-4 py-3 font-mono text-xs text-surface-800/70">{proof.proof_id}</td>
                        <td className="px-4 py-3 text-surface-800/80">{proof.provider}</td>
                        <td className="px-4 py-3 text-surface-800/60">{proof.model}</td>
                        <td className="px-4 py-3">
                          <span className={`px-2 py-0.5 text-xs rounded-md font-medium ${typeColor(proof.type)}`}>{proof.type}</span>
                        </td>
                        <td className="px-4 py-3">
                          <span className={`px-2 py-0.5 text-xs rounded-md font-medium ${statusColor(proof.status)}`}>{proof.status}</span>
                        </td>
                        <td className="px-4 py-3 text-xs text-surface-800/50">{new Date(proof.created_at).toLocaleDateString()}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </div>
          )}

          {/* Verify Proof Form */}
          <div className="rounded-xl border border-surface-200 bg-surface-0 p-5 space-y-3">
            <h3 className="text-sm font-semibold text-surface-900">Verify Proof</h3>
            <textarea
              placeholder='Paste proof JSON here... e.g. {"proof": "...", "public_inputs": [...], "verification_key": "..."}'
              value={verifyJson}
              onChange={(e) => setVerifyJson(e.target.value)}
              rows={4}
              className="w-full px-3 py-2 text-sm font-mono rounded-lg border border-surface-200 bg-surface-0 text-surface-800 focus:outline-none focus:ring-2 focus:ring-brand-500/30 resize-none"
            />
            <div className="flex items-center gap-3">
              <button
                type="button"
                onClick={handleVerify}
                disabled={verifying || !verifyJson.trim()}
                className="px-4 py-2 text-sm font-medium rounded-lg bg-brand-600 text-white hover:bg-brand-700 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
              >
                {verifying ? "Verifying..." : "Verify Proof"}
              </button>
              {verifyResult && (
                <span className={`text-xs ${verifyResult.startsWith("Proof verified") ? "text-green-600" : "text-red-600"}`}>
                  {verifyResult}
                </span>
              )}
            </div>
          </div>
        </div>

        {/* Sidebar: Recent Activity */}
        <div className="space-y-4">
          <div className="rounded-xl border border-surface-200 bg-surface-0 p-5 space-y-3">
            <h3 className="text-sm font-semibold text-surface-900">Recent Activity</h3>
            <div className="space-y-3">
              {recentActivity.map((a) => (
                <div key={a.id} className="flex items-start gap-3">
                  <div className={`mt-1 w-2 h-2 rounded-full flex-shrink-0 ${
                    a.status === "verified" ? "bg-green-500" : a.status === "pending" ? "bg-yellow-500" : "bg-red-500"
                  }`} />
                  <div className="min-w-0">
                    <div className="text-xs font-mono text-surface-800/70 truncate">{a.id}</div>
                    <div className="text-xs text-surface-800/40 truncate">{a.provider}</div>
                    <div className="text-[10px] text-surface-800/30">{new Date(a.time).toLocaleString()}</div>
                  </div>
                </div>
              ))}
            </div>
          </div>
        </div>
      </div>

      {/* Modal */}
      {selectedProof && <ProofModal proof={selectedProof} onClose={() => setSelectedProof(null)} />}
    </main>
  );
}
