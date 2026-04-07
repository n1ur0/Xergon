"use client";

import { useState, useEffect, useMemo, useCallback } from "react";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

type TeeType = "SGX" | "SEV" | "TDX" | "Software";
type AttestationStatus = "valid" | "expired" | "pending" | "revoked";

interface AttestationProvider {
  id: string;
  provider: string;
  tee_type: TeeType;
  mrenclave: string;
  status: AttestationStatus;
  last_attested: string;
  quote_verified: boolean;
  enclave_version: string;
  security_version: number;
}

interface TimelineEvent {
  id: string;
  provider: string;
  event: string;
  timestamp: string;
  type: "attestation" | "expiry" | "revocation" | "verification";
}

// ---------------------------------------------------------------------------
// Mock data
// ---------------------------------------------------------------------------

const MOCK_PROVIDERS: AttestationProvider[] = [
  { id: "ap1", provider: "NeuralForge Alpha", tee_type: "SGX", mrenclave: "7a3f8e2b1c4d9f6e0a5b8c3d2e1f4a7b9c0d5e8f2a3b6c9d0e4f7a1b2c5d8", status: "valid", last_attested: "2025-11-12T08:30:00Z", quote_verified: true, enclave_version: "v2.4.1", security_version: 8 },
  { id: "ap2", provider: "DeepCompute Sigma", tee_type: "TDX", mrenclave: "2c8df1a4e9b23c7d5a1f8e4b6c0d3a7e9f2b5c8d1a4e7f0b3c6d9a2e5f8b1c4", status: "valid", last_attested: "2025-11-12T06:15:00Z", quote_verified: true, enclave_version: "v3.1.0", security_version: 6 },
  { id: "ap3", provider: "SafeInfer Gamma", tee_type: "SGX", mrenclave: "3a9f1e5d4c8b2a7f0e3d6c9b5a2f8e1d4c7b0a3f6e9d2c5b8a1e4f7d0c3b6", status: "valid", last_attested: "2025-11-11T22:00:00Z", quote_verified: true, enclave_version: "v2.4.1", security_version: 8 },
  { id: "ap4", provider: "EnclaveAI Zeta", tee_type: "SEV", mrenclave: "8b2d4c6a1e9f3b7d0a5c8e2f6b9d1a4e7c0f3b6d9a2e5f8c1b4d7a0e3f6b9", status: "valid", last_attested: "2025-11-12T10:45:00Z", quote_verified: true, enclave_version: "v1.8.2", security_version: 5 },
  { id: "ap5", provider: "QuantumEdge Delta", tee_type: "TDX", mrenclave: "b4c7a2f9e1d3b5c8a0f2e4d6b9c1a3f5e7d0b2c4a6f8e1d3b5c7a9f0e2d4b6", status: "expired", last_attested: "2025-11-05T14:20:00Z", quote_verified: false, enclave_version: "v2.9.0", security_version: 4 },
  { id: "ap6", provider: "TrustGrid Omega", tee_type: "SGX", mrenclave: "d1e87b3c9f2a5e8d1c4b7f0a3e6d9c2b5f8a1e4d7c0b3f6a9e2d5c8b1f4a7e0", status: "pending", last_attested: "2025-11-11T16:30:00Z", quote_verified: false, enclave_version: "v2.3.0", security_version: 7 },
  { id: "ap7", provider: "VertexMind Beta", tee_type: "SEV", mrenclave: "9e1bc3d75a2f8e4b1c6d9a0e3f7b5c2d8a4e1f6b9c3d7a0e5f2b8c1d4a7e0f3", status: "revoked", last_attested: "2025-10-28T09:00:00Z", quote_verified: false, enclave_version: "v1.5.0", security_version: 3 },
  { id: "ap8", provider: "OpenNet Theta", tee_type: "Software", mrenclave: "c5f13a8e6b9d2c4a7f0e3d5b8c1a6f9e2d4b7c0a3f5e8d1b4c6a9e2f5d8b1c3", status: "valid", last_attested: "2025-11-10T18:45:00Z", quote_verified: true, enclave_version: "v1.0.0", security_version: 1 },
];

const MOCK_TIMELINE: TimelineEvent[] = [
  { id: "t1", provider: "EnclaveAI Zeta", event: "SEV attestation verified successfully", timestamp: "2025-11-12T10:45:00Z", type: "attestation" },
  { id: "t2", provider: "NeuralForge Alpha", event: "SGX quote verified, MRENCLAVE match confirmed", timestamp: "2025-11-12T08:30:00Z", type: "verification" },
  { id: "t3", provider: "DeepCompute Sigma", event: "TDX attestation report validated", timestamp: "2025-11-12T06:15:00Z", type: "attestation" },
  { id: "t4", provider: "TrustGrid Omega", event: "Attestation submitted, pending review", timestamp: "2025-11-11T16:30:00Z", type: "attestation" },
  { id: "t5", provider: "SafeInfer Gamma", event: "SGX quote signature verification passed", timestamp: "2025-11-11T22:00:00Z", type: "verification" },
  { id: "t6", provider: "QuantumEdge Delta", event: "TDX attestation expired", timestamp: "2025-11-05T14:20:00Z", type: "expiry" },
  { id: "t7", provider: "VertexMind Beta", event: "SEV attestation revoked: key compromise detected", timestamp: "2025-10-28T09:00:00Z", type: "revocation" },
  { id: "t8", provider: "OpenNet Theta", event: "Software attestation verified via code hash", timestamp: "2025-11-10T18:45:00Z", type: "verification" },
];

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function statusBadge(s: AttestationStatus): string {
  if (s === "valid") return "bg-green-100 text-green-700 dark:bg-green-900/30 dark:text-green-400";
  if (s === "expired") return "bg-yellow-100 text-yellow-700 dark:bg-yellow-900/30 dark:text-yellow-400";
  if (s === "pending") return "bg-blue-100 text-blue-700 dark:bg-blue-900/30 dark:text-blue-400";
  return "bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-400";
}

function teeBadge(t: TeeType): string {
  if (t === "SGX") return "bg-indigo-100 text-indigo-700 dark:bg-indigo-900/30 dark:text-indigo-400";
  if (t === "SEV") return "bg-orange-100 text-orange-700 dark:bg-orange-900/30 dark:text-orange-400";
  if (t === "TDX") return "bg-cyan-100 text-cyan-700 dark:bg-cyan-900/30 dark:text-cyan-400";
  return "bg-surface-100 text-surface-700 dark:bg-surface-800 dark:text-surface-400";
}

function timelineIcon(type: TimelineEvent["type"]): { color: string; icon: string } {
  switch (type) {
    case "attestation": return { color: "bg-green-500", icon: "M5 13l4 4L19 7" };
    case "verification": return { color: "bg-blue-500", icon: "M9 12l2 2 4-4" };
    case "expiry": return { color: "bg-yellow-500", icon: "M12 9v4m0 4h.01" };
    case "revocation": return { color: "bg-red-500", icon: "M18 6L6 18M6 6l12 12" };
  }
}

// ---------------------------------------------------------------------------
// Skeleton
// ---------------------------------------------------------------------------

function CardSkeleton() {
  return (
    <div className="rounded-xl border border-surface-200 bg-surface-0 p-5 space-y-3 animate-pulse">
      <div className="flex items-center justify-between">
        <div className="h-5 w-32 rounded bg-surface-200" />
        <div className="h-5 w-16 rounded-full bg-surface-200" />
      </div>
      <div className="flex gap-2">
        <div className="h-5 w-14 rounded-md bg-surface-200" />
        <div className="h-5 w-16 rounded-md bg-surface-200" />
      </div>
      <div className="h-3 w-full rounded bg-surface-200" />
      <div className="h-3 w-40 rounded bg-surface-200" />
    </div>
  );
}

// ---------------------------------------------------------------------------
// Pie chart (simple CSS conic gradient)
// ---------------------------------------------------------------------------

function StatusPieChart({ counts }: { counts: { valid: number; expired: number; pending: number; revoked: number } }) {
  const total = counts.valid + counts.expired + counts.pending + counts.revoked;
  if (total === 0) return null;

  const segments = [
    { count: counts.valid, color: "#22c55e", label: "Valid" },
    { count: counts.expired, color: "#eab308", label: "Expired" },
    { count: counts.pending, color: "#3b82f6", label: "Pending" },
    { count: counts.revoked, color: "#ef4444", label: "Revoked" },
  ].filter((s) => s.count > 0);

  let gradientParts: string[] = [];
  let currentAngle = 0;
  for (const seg of segments) {
    const pct = (seg.count / total) * 100;
    gradientParts.push(`${seg.color} ${currentAngle}% ${currentAngle + pct}%`);
    currentAngle += pct;
  }

  return (
    <div className="flex items-center gap-6">
      <div
        className="w-28 h-28 rounded-full flex-shrink-0"
        style={{ background: `conic-gradient(${gradientParts.join(", ")})` }}
      />
      <div className="space-y-2">
        {segments.map((seg) => (
          <div key={seg.label} className="flex items-center gap-2 text-xs">
            <div className="w-3 h-3 rounded-sm flex-shrink-0" style={{ backgroundColor: seg.color }} />
            <span className="text-surface-800/70">{seg.label}</span>
            <span className="font-medium text-surface-900">{seg.count}</span>
          </div>
        ))}
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Main component
// ---------------------------------------------------------------------------

export default function AttestationPage() {
  const [loading, setLoading] = useState(true);
  const [filterTeeType, setFilterTeeType] = useState<TeeType | "all">("all");
  const [filterStatus, setFilterStatus] = useState<AttestationStatus | "all">("all");
  const [verifyProviderId, setVerifyProviderId] = useState("");
  const [verifyReport, setVerifyReport] = useState("");
  const [verifyResult, setVerifyResult] = useState<string | null>(null);
  const [verifying, setVerifying] = useState(false);

  useEffect(() => {
    const t = setTimeout(() => setLoading(false), 800);
    return () => clearTimeout(t);
  }, []);

  const filteredProviders = useMemo(() => {
    let list = [...MOCK_PROVIDERS];
    if (filterTeeType !== "all") list = list.filter((p) => p.tee_type === filterTeeType);
    if (filterStatus !== "all") list = list.filter((p) => p.status === filterStatus);
    list.sort((a, b) => new Date(b.last_attested).getTime() - new Date(a.last_attested).getTime());
    return list;
  }, [filterTeeType, filterStatus]);

  const statusCounts = useMemo(() => ({
    valid: MOCK_PROVIDERS.filter((p) => p.status === "valid").length,
    expired: MOCK_PROVIDERS.filter((p) => p.status === "expired").length,
    pending: MOCK_PROVIDERS.filter((p) => p.status === "pending").length,
    revoked: MOCK_PROVIDERS.filter((p) => p.status === "revoked").length,
  }), []);

  const handleVerify = useCallback(() => {
    if (!verifyProviderId.trim() || !verifyReport.trim()) return;
    setVerifying(true);
    setVerifyResult(null);
    setTimeout(() => {
      const success = verifyReport.length > 15;
      setVerifyResult(
        success
          ? "Attestation report verified. Provider enclave identity confirmed. Quote signature valid."
          : "Verification failed: Invalid attestation report format or signature mismatch."
      );
      setVerifying(false);
    }, 2000);
  }, [verifyProviderId, verifyReport]);

  return (
    <main className="mx-auto max-w-6xl px-4 py-6 space-y-6">
      {/* Header */}
      <div className="space-y-1">
        <div className="flex items-center gap-3">
          <svg className="w-7 h-7 text-brand-600" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
            <rect x="3" y="11" width="18" height="11" rx="2" ry="2" />
            <path d="M7 11V7a5 5 0 0110 0v4" />
          </svg>
          <h1 className="text-2xl font-bold text-surface-900">TEE Attestation</h1>
        </div>
        <p className="text-sm text-surface-800/60">Monitor and verify TEE attestation status for all network providers</p>
      </div>

      {/* Stats + Pie */}
      <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
        <div className="rounded-xl border border-surface-200 bg-surface-0 p-5">
          <h3 className="text-sm font-semibold text-surface-900 mb-4">Status Distribution</h3>
          <StatusPieChart counts={statusCounts} />
        </div>
        <div className="grid grid-cols-2 gap-4">
          {[
            { label: "SGX Providers", value: MOCK_PROVIDERS.filter((p) => p.tee_type === "SGX").length, color: "text-indigo-600" },
            { label: "SEV Providers", value: MOCK_PROVIDERS.filter((p) => p.tee_type === "SEV").length, color: "text-orange-600" },
            { label: "TDX Providers", value: MOCK_PROVIDERS.filter((p) => p.tee_type === "TDX").length, color: "text-cyan-600" },
            { label: "Software", value: MOCK_PROVIDERS.filter((p) => p.tee_type === "Software").length, color: "text-surface-600" },
          ].map((s) => (
            <div key={s.label} className="rounded-xl border border-surface-200 bg-surface-0 p-4 flex flex-col justify-center">
              <div className="text-xs text-surface-800/50 mb-1">{s.label}</div>
              <div className={`text-2xl font-bold ${s.color}`}>{s.value}</div>
            </div>
          ))}
        </div>
      </div>

      {/* Filters */}
      <div className="flex flex-wrap gap-2">
        <select value={filterTeeType} onChange={(e) => setFilterTeeType(e.target.value as TeeType | "all")} className="px-3 py-1.5 text-sm rounded-lg border border-surface-200 bg-surface-0 text-surface-800 focus:outline-none focus:ring-2 focus:ring-brand-500/30">
          <option value="all">All TEE Types</option>
          <option value="SGX">SGX</option>
          <option value="SEV">SEV</option>
          <option value="TDX">TDX</option>
          <option value="Software">Software</option>
        </select>
        <select value={filterStatus} onChange={(e) => setFilterStatus(e.target.value as AttestationStatus | "all")} className="px-3 py-1.5 text-sm rounded-lg border border-surface-200 bg-surface-0 text-surface-800 focus:outline-none focus:ring-2 focus:ring-brand-500/30">
          <option value="all">All Status</option>
          <option value="valid">Valid</option>
          <option value="expired">Expired</option>
          <option value="pending">Pending</option>
          <option value="revoked">Revoked</option>
        </select>
      </div>

      {/* Provider cards */}
      {loading ? (
        <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
          {Array.from({ length: 6 }, (_, i) => <CardSkeleton key={i} />)}
        </div>
      ) : filteredProviders.length === 0 ? (
        <div className="flex flex-col items-center justify-center py-16 text-center space-y-3">
          <svg className="w-12 h-12 text-surface-300" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5">
            <rect x="3" y="11" width="18" height="11" rx="2" ry="2" /><path d="M7 11V7a5 5 0 0110 0v4" />
          </svg>
          <p className="font-medium text-surface-800/70">No providers match your filters</p>
        </div>
      ) : (
        <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
          {filteredProviders.map((prov) => (
            <div key={prov.id} className="rounded-xl border border-surface-200 bg-surface-0 p-5 space-y-3 transition-all hover:shadow-sm">
              <div className="flex items-center justify-between">
                <span className="font-medium text-surface-900 truncate">{prov.provider}</span>
                <span className={`px-2 py-0.5 text-xs rounded-md font-medium capitalize ${statusBadge(prov.status)}`}>{prov.status}</span>
              </div>
              <div className="flex gap-2">
                <span className={`px-2 py-0.5 text-xs rounded-md font-medium ${teeBadge(prov.tee_type)}`}>{prov.tee_type}</span>
                {prov.quote_verified && (
                  <span className="px-2 py-0.5 text-xs rounded-md bg-green-100 text-green-700 dark:bg-green-900/30 dark:text-green-400 font-medium">Quote Verified</span>
                )}
              </div>
              <div>
                <div className="text-xs text-surface-800/40 mb-0.5">MRENCLAVE</div>
                <div className="font-mono text-xs text-surface-800/60 truncate">{prov.mrenclave.slice(0, 16)}...{prov.mrenclave.slice(-8)}</div>
              </div>
              <div className="flex items-center justify-between text-xs text-surface-800/40">
                <span>Last attested: {new Date(prov.last_attested).toLocaleDateString()}</span>
                <span>SVN: {prov.security_version}</span>
              </div>
            </div>
          ))}
        </div>
      )}

      {/* Attestation Timeline */}
      <div className="rounded-xl border border-surface-200 bg-surface-0 p-5 space-y-4">
        <h3 className="text-sm font-semibold text-surface-900">Attestation Timeline</h3>
        <div className="space-y-3">
          {MOCK_TIMELINE.map((evt, idx) => {
            const icon = timelineIcon(evt.type);
            return (
              <div key={evt.id} className="flex gap-3">
                <div className="flex flex-col items-center">
                  <div className={`w-6 h-6 rounded-full ${icon.color} flex items-center justify-center flex-shrink-0`}>
                    <svg className="w-3.5 h-3.5 text-white" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round">
                      <path d={icon.icon} />
                    </svg>
                  </div>
                  {idx < MOCK_TIMELINE.length - 1 && (
                    <div className="w-px flex-1 bg-surface-200 dark:bg-surface-700 mt-1" />
                  )}
                </div>
                <div className="pb-4 min-w-0">
                  <div className="text-sm text-surface-800/80">{evt.event}</div>
                  <div className="text-xs text-surface-800/40 mt-0.5">
                    {evt.provider} &middot; {new Date(evt.timestamp).toLocaleString()}
                  </div>
                </div>
              </div>
            );
          })}
        </div>
      </div>

      {/* Verify Attestation Form */}
      <div className="rounded-xl border border-surface-200 bg-surface-0 p-5 space-y-3">
        <h3 className="text-sm font-semibold text-surface-900">Verify Attestation</h3>
        <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
          <div>
            <label className="block text-xs text-surface-800/50 mb-1">Provider ID</label>
            <input
              type="text"
              placeholder="e.g. ap1"
              value={verifyProviderId}
              onChange={(e) => setVerifyProviderId(e.target.value)}
              className="w-full px-3 py-2 text-sm rounded-lg border border-surface-200 bg-surface-0 text-surface-800 focus:outline-none focus:ring-2 focus:ring-brand-500/30"
            />
          </div>
          <div>
            <label className="block text-xs text-surface-800/50 mb-1">Attestation Report JSON</label>
            <input
              type="text"
              placeholder='{"report": "...", "signature": "..."}'
              value={verifyReport}
              onChange={(e) => setVerifyReport(e.target.value)}
              className="w-full px-3 py-2 text-sm font-mono rounded-lg border border-surface-200 bg-surface-0 text-surface-800 focus:outline-none focus:ring-2 focus:ring-brand-500/30"
            />
          </div>
        </div>
        <div className="flex items-center gap-3">
          <button
            type="button"
            onClick={handleVerify}
            disabled={verifying || !verifyProviderId.trim() || !verifyReport.trim()}
            className="px-4 py-2 text-sm font-medium rounded-lg bg-brand-600 text-white hover:bg-brand-700 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
          >
            {verifying ? "Verifying..." : "Verify Attestation"}
          </button>
          {verifyResult && (
            <span className={`text-xs ${verifyResult.startsWith("Attestation report verified") ? "text-green-600" : "text-red-600"}`}>
              {verifyResult}
            </span>
          )}
        </div>
      </div>

      {/* Security Info */}
      <div className="rounded-xl border border-surface-200 bg-surface-0 p-5 space-y-4">
        <h3 className="text-sm font-semibold text-surface-900">Security Information</h3>
        <div className="grid grid-cols-1 md:grid-cols-3 gap-4 text-sm">
          <div className="rounded-lg bg-surface-50 dark:bg-surface-800/50 p-4 space-y-2">
            <h4 className="font-medium text-surface-800/80 flex items-center gap-2">
              <svg className="w-4 h-4 text-green-500" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                <path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z" />
              </svg>
              What TEE Proves
            </h4>
            <p className="text-xs text-surface-800/50 leading-relaxed">
              TEE attestation proves that computation occurred inside a hardware-isolated enclave with a specific code identity (MRENCLAVE). It guarantees the code was not tampered with during execution and that the runtime environment matches expectations.
            </p>
          </div>
          <div className="rounded-lg bg-surface-50 dark:bg-surface-800/50 p-4 space-y-2">
            <h4 className="font-medium text-surface-800/80 flex items-center gap-2">
              <svg className="w-4 h-4 text-yellow-500" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                <path d="M10.29 3.86L1.82 18a2 2 0 001.71 3h16.94a2 2 0 001.71-3L13.71 3.86a2 2 0 00-3.42 0z" />
                <line x1="12" y1="9" x2="12" y2="13" /><line x1="12" y1="17" x2="12.01" y2="17" />
              </svg>
              Limitations
            </h4>
            <p className="text-xs text-surface-800/50 leading-relaxed">
              TEE attestation does not guarantee correctness of the AI model output. It only proves the execution environment. Side-channel attacks, firmware vulnerabilities, and supply-chain issues remain potential risks. Attestation must be combined with other verification mechanisms.
            </p>
          </div>
          <div className="rounded-lg bg-surface-50 dark:bg-surface-800/50 p-4 space-y-2">
            <h4 className="font-medium text-surface-800/80 flex items-center gap-2">
              <svg className="w-4 h-4 text-blue-500" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                <circle cx="12" cy="12" r="10" /><line x1="12" y1="16" x2="12" y2="12" /><line x1="12" y1="8" x2="12.01" y2="8" />
              </svg>
              Trust Model
            </h4>
            <p className="text-xs text-surface-800/50 leading-relaxed">
              Xergon uses a multi-layer trust model: hardware-rooted TEE attestation (SGX/SEV/TDX), ZK proof verification for output integrity, and on-chain anchoring for auditability. Providers must pass all layers for full trust score benefits.
            </p>
          </div>
        </div>
      </div>
    </main>
  );
}
