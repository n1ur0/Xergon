/**
 * Provider Dashboard API types and endpoint stubs.
 *
 * Now fully independent of Paperclip — all data comes from xergon-agent
 * via the aggregated /xergon/dashboard endpoint.
 */

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/** Node health from dashboard */
export interface XergonNodeStatus {
  synced: boolean;
  height: number;
  bestHeight: number;
  peers: number;
  uptimeSeconds: number;
  version: string;
  ergoAddress: string;
}

/** Peer detail from dashboard */
export interface XergonPeer {
  address: string;
  connectionType: string;
  height: number;
  lastSeen: string;
}

/** PoNW / AI Points summary from dashboard */
export interface AiPointsData {
  totalInputTokens: number;
  totalOutputTokens: number;
  totalTokens: number;
  aiPoints: number;
  byModel: AiPointsModelBreakdown[];
}

export interface AiPointsModelBreakdown {
  model: string;
  totalTokens: number;
  points: number;
  difficultyMultiplier: number;
  // Legacy fields kept for UI compat
  inputTokens: number;
  outputTokens: number;
  difficultyBreakdown: {
    compositeMult: number;
    gpuMult: number;
    benchmarkMult: number;
    paramsMult: number;
    contextMult: number;
    quantMult: number;
  };
}

/** Provider score from dashboard */
export interface ProviderScoreData {
  weightedCompositeScore: number;
  bestCompositeScore: number;
}

/** Settlement record from dashboard */
export interface SettlementRecord {
  id: string;
  txId: string;
  amountNanoerg: number;
  amountErg: number;
  creditsConverted?: number;
  status: "pending" | "confirmed" | "failed";
  createdAt: string;
  confirmedAt: string | null;
}

/** Combined provider dashboard state */
export interface ProviderDashboardData {
  nodeStatus: XergonNodeStatus | null;
  peers: XergonPeer[];
  aiPoints: AiPointsData | null;
  providerScore: ProviderScoreData | null;
  hardware: HardwareInfo | null;
  settlements: SettlementRecord[];
  hasWallet: boolean;
}

// GPU hardware reported by the agent
interface HardwareInfo {
  devices: Array<{
    name: string;
    deviceName: string;
    vendor: string;
    vramBytes: number;
    vramMb: number;
    computeVersion: string;
    detectionMethod: string;
    isActive: boolean;
    driver: string;
  }>;
  lastReportedAt: string | null;
}

// ---------------------------------------------------------------------------
// Raw API response from /xergon/dashboard
// ---------------------------------------------------------------------------

interface DashboardRaw {
  node_status: {
    synced: boolean;
    height: number;
    best_height: number;
    peers: number;
    uptime_seconds: number;
    version: string;
    ergo_address: string;
  } | null;
  peers: Array<{
    address: string;
    connection_type: string;
    height: number;
    last_seen: string;
  }>;
  ai_points: {
    total_input_tokens: number;
    total_output_tokens: number;
    total_tokens: number;
    ai_points: number;
    by_model: Array<{
      model: string;
      total_tokens: number;
      points: number;
      difficulty_multiplier: number;
    }>;
  } | null;
  provider_score: {
    weighted_composite_score: number;
    best_composite_score: number;
  } | null;
  settlements: Array<{
    id: string;
    tx_id: string;
    amount_nanoerg: number;
    amount_erg: number;
    status: string;
    created_at: string;
    confirmed_at: string | null;
  }>;
  has_wallet: boolean;
  hardware: {
    devices: Array<{
      name: string;
      device_name: string;
      vendor: string;
      vram_bytes: number;
      vram_mb: number;
      compute_version: string;
      detection_method: string;
      is_active: boolean;
      driver: string;
    }>;
    last_reported_at: string | null;
  } | null;
}

// ---------------------------------------------------------------------------
// API helpers
// ---------------------------------------------------------------------------

/** Fetch from xergon-agent via the Next.js proxy route (server-side, no direct browser access) */
async function agentFetch<T>(path: string): Promise<T | null> {
  try {
    const res = await fetch(`/api/xergon-agent${path}`);
    if (!res.ok) return null;
    return (await res.json()) as T;
  } catch {
    return null;
  }
}

// ---------------------------------------------------------------------------
// Endpoint methods
// ---------------------------------------------------------------------------

/** Fetch all provider dashboard data from xergon-agent /xergon/dashboard */
export async function fetchProviderDashboardData(_companyId: string): Promise<ProviderDashboardData> {
  const raw = await agentFetch<DashboardRaw>("/xergon/dashboard");

  if (!raw) {
    return {
      nodeStatus: null,
      peers: [],
      aiPoints: null,
      providerScore: null,
      hardware: null,
      settlements: [],
      hasWallet: false,
    };
  }

  // Map snake_case to camelCase
  const nodeStatus: XergonNodeStatus | null = raw.node_status
    ? {
        synced: raw.node_status.synced,
        height: raw.node_status.height,
        bestHeight: raw.node_status.best_height,
        peers: raw.node_status.peers,
        uptimeSeconds: raw.node_status.uptime_seconds,
        version: raw.node_status.version,
        ergoAddress: raw.node_status.ergo_address,
      }
    : null;

  const peers: XergonPeer[] = raw.peers.map((p) => ({
    address: p.address,
    connectionType: p.connection_type,
    height: p.height,
    lastSeen: p.last_seen,
  }));

  const aiPoints: AiPointsData | null = raw.ai_points
    ? {
        totalInputTokens: raw.ai_points.total_input_tokens,
        totalOutputTokens: raw.ai_points.total_output_tokens,
        totalTokens: raw.ai_points.total_tokens,
        aiPoints: raw.ai_points.ai_points,
        byModel: raw.ai_points.by_model.map((m) => ({
          model: m.model,
          totalTokens: m.total_tokens,
          points: m.points,
          difficultyMultiplier: m.difficulty_multiplier,
          // Legacy compat fields
          inputTokens: 0,
          outputTokens: m.total_tokens,
          difficultyBreakdown: {
            compositeMult: m.difficulty_multiplier,
            gpuMult: m.difficulty_multiplier >= 2.0 ? 2.0 : 1.0,
            benchmarkMult: 1.0,
            paramsMult: 1.0,
            contextMult: 1.0,
            quantMult: 1.0,
          },
        })),
      }
    : null;

  const providerScore: ProviderScoreData | null = raw.provider_score
    ? {
        weightedCompositeScore: raw.provider_score.weighted_composite_score,
        bestCompositeScore: raw.provider_score.best_composite_score,
      }
    : null;

  const settlements: SettlementRecord[] = raw.settlements.map((s) => ({
    id: s.id,
    txId: s.tx_id,
    amountNanoerg: s.amount_nanoerg,
    amountErg: s.amount_erg,
    status: s.status as SettlementRecord["status"],
    createdAt: s.created_at,
    confirmedAt: s.confirmed_at,
  }));

  const hardware: HardwareInfo | null = raw.hardware
    ? {
        devices: raw.hardware.devices.map((d) => ({
          name: d.name,
          deviceName: d.device_name,
          vendor: d.vendor,
          vramBytes: d.vram_bytes,
          vramMb: d.vram_mb,
          computeVersion: d.compute_version,
          detectionMethod: d.detection_method,
          isActive: d.is_active,
          driver: d.driver,
        })),
        lastReportedAt: raw.hardware.last_reported_at,
      }
    : null;

  return {
    nodeStatus,
    peers,
    aiPoints,
    providerScore,
    hardware,
    settlements,
    hasWallet: raw.has_wallet,
  };
}
