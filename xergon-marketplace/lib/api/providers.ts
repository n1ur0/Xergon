/**
 * Providers API — provider listing types and fetch helpers.
 *
 * Data comes from the xergon-relay via /api/xergon-relay/providers proxy route.
 * If the relay is unreachable, the proxy returns mock data with a "degraded" flag.
 */

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface ProviderInfo {
  endpoint: string;
  name: string;
  region: string;
  models: string[];
  uptime: number;           // percentage 0-100
  totalTokens: number;
  aiPoints: number;
  pricePer1mTokens: number; // nanoerg
  status: 'online' | 'degraded' | 'offline';
  lastSeen: string;
  gpuInfo: string;          // e.g. "NVIDIA RTX 4090"
  latencyMs: number;
  ergoAddress?: string;
  uptimeHistory?: number[]; // last 7 days daily uptime %
  modelPricing?: Record<string, number>; // per-model price in nanoerg
}

export interface ProviderFilters {
  search: string;
  region: string;           // 'all' | 'US' | 'EU' | 'Asia'
  status: string;           // 'all' | 'online' | 'degraded' | 'offline'
  model: string;            // 'all' | specific model name
  sortBy: string;           // 'aiPoints' | 'uptime' | 'tokens' | 'price' | 'name'
  sortOrder: 'asc' | 'desc';
}

export interface ProviderListResponse {
  providers: ProviderInfo[];
  degraded?: boolean;
}

// ---------------------------------------------------------------------------
// Fetch helper
// ---------------------------------------------------------------------------

/**
 * Fetch provider list from the relay proxy, optionally with server-side filters.
 * Falls back to client-side filtering/sorting if needed.
 */
export async function fetchProviders(
  filters?: Partial<ProviderFilters>,
): Promise<ProviderListResponse> {
  try {
    const params = new URLSearchParams();
    if (filters?.region && filters.region !== 'all') params.set('region', filters.region);
    if (filters?.status && filters.status !== 'all') params.set('status', filters.status);
    if (filters?.model && filters.model !== 'all') params.set('model', filters.model);
    if (filters?.sortBy) params.set('sort', filters.sortBy);
    if (filters?.sortOrder) params.set('order', filters.sortOrder);

    const qs = params.toString();
    const url = `/api/xergon-relay/providers${qs ? `?${qs}` : ''}`;

    const res = await fetch(url, {
      next: { revalidate: 30 },
    });
    if (!res.ok) {
      throw new Error(`Providers endpoint returned ${res.status}`);
    }
    return (await res.json()) as ProviderListResponse;
  } catch {
    return {
      providers: [],
      degraded: true,
    };
  }
}

// ---------------------------------------------------------------------------
// Client-side filtering / sorting
// ---------------------------------------------------------------------------

/**
 * Apply filters and sorting on the client side (fallback or supplemental).
 */
export function filterProviders(
  providers: ProviderInfo[],
  filters: Partial<ProviderFilters>,
): ProviderInfo[] {
  let result = [...providers];

  if (filters.search) {
    const q = filters.search.toLowerCase();
    result = result.filter(
      (p) =>
        p.name.toLowerCase().includes(q) ||
        p.endpoint.toLowerCase().includes(q) ||
        p.models.some((m) => m.toLowerCase().includes(q)) ||
        p.gpuInfo.toLowerCase().includes(q),
    );
  }

  if (filters.region && filters.region !== 'all') {
    result = result.filter((p) => p.region === filters.region);
  }

  if (filters.status && filters.status !== 'all') {
    result = result.filter((p) => p.status === filters.status);
  }

  if (filters.model && filters.model !== 'all') {
    result = result.filter((p) => p.models.includes(filters.model!));
  }

  // Sorting
  if (filters.sortBy) {
    const dir = filters.sortOrder === 'asc' ? 1 : -1;
    result.sort((a, b) => {
      switch (filters.sortBy) {
        case 'aiPoints':
          return (a.aiPoints - b.aiPoints) * dir;
        case 'uptime':
          return (a.uptime - b.uptime) * dir;
        case 'tokens':
          return (a.totalTokens - b.totalTokens) * dir;
        case 'price':
          return (a.pricePer1mTokens - b.pricePer1mTokens) * dir;
        case 'name':
          return a.name.localeCompare(b.name) * dir;
        default:
          return 0;
      }
    });
  }

  return result;
}

/**
 * Extract unique model names from a provider list.
 */
export function extractModels(providers: ProviderInfo[]): string[] {
  const set = new Set<string>();
  for (const p of providers) {
    for (const m of p.models) {
      set.add(m);
    }
  }
  return Array.from(set).sort();
}
