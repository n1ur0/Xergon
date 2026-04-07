import { NextResponse } from "next/server";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

type RentalStatus = "active" | "completed" | "cancelled" | "expired";

interface Rental {
  id: string;
  modelId: string;
  providerPk: string;
  providerRegion: string;
  status: RentalStatus;
  startedAt: string;
  endedAt?: string;
  durationHours: number;
  costNanoErg: number;
  tokensUsed: number;
}

// ---------------------------------------------------------------------------
// Deterministic mock data generator
// ---------------------------------------------------------------------------

function generateMockRentals(address: string): Rental[] {
  let seed = 0;
  for (let i = 0; i < address.length; i++)
    seed = (seed * 31 + address.charCodeAt(i)) | 0;

  function rand(): number {
    seed = (seed * 16807 + 12345) & 0x7fffffff;
    return seed / 0x7fffffff;
  }

  const models = [
    "llama-3.1-70b",
    "qwen2.5-72b",
    "mistral-7b",
    "deepseek-coder-33b",
    "phi-3-medium",
  ];

  const regions = [
    "North America",
    "Europe",
    "Asia",
    "South America",
    "Oceania",
  ];

  const statuses: RentalStatus[] = [
    "active",
    "completed",
    "completed",
    "completed",
    "cancelled",
    "expired",
  ];

  const count = Math.floor(rand() * 40) + 15;
  const now = Date.now();

  return Array.from({ length: count }, (_, i) => {
    const startedAt = new Date(now - Math.floor(rand() * 30 * 86400000));
    const durationHours = [1, 2, 4, 8, 24, 48][Math.floor(rand() * 6)];
    const status = statuses[Math.floor(rand() * statuses.length)];

    const endedAt =
      status !== "active"
        ? new Date(startedAt.getTime() + durationHours * 3600000)
        : undefined;

    return {
      id: `rental_${i.toString().padStart(4, "0")}_${address.slice(0, 4)}`,
      modelId: models[Math.floor(rand() * models.length)],
      providerPk: `9${Math.random().toString(36).slice(2, 42).padEnd(40, "0")}`,
      providerRegion: regions[Math.floor(rand() * regions.length)],
      status,
      startedAt: startedAt.toISOString(),
      endedAt: endedAt?.toISOString(),
      durationHours,
      costNanoErg: Math.floor(durationHours * (1_000_000_000 + rand() * 4_000_000_000)),
      tokensUsed: Math.floor(rand() * 5_000_000),
    };
  });
}

// ---------------------------------------------------------------------------
// GET handler
// ---------------------------------------------------------------------------

export async function GET(request: Request) {
  try {
    const { searchParams } = new URL(request.url);
    const address =
      searchParams.get("address") || "3WxTQSY6VxEL3UdRp2Nxv5Tf1T9K7mMNcVv3";
    const status = searchParams.get("status");
    const limit = parseInt(searchParams.get("limit") || "20", 10);
    const offset = parseInt(searchParams.get("offset") || "0", 10);
    const sort = searchParams.get("sort") || "date_desc";

    let rentals = generateMockRentals(address);

    // Filter
    if (status && status !== "all") {
      rentals = rentals.filter((r) => r.status === status);
    }

    // Sort
    if (sort === "date_asc") {
      rentals.sort(
        (a, b) =>
          new Date(a.startedAt).getTime() - new Date(b.startedAt).getTime(),
      );
    } else if (sort === "cost_desc") {
      rentals.sort((a, b) => b.costNanoErg - a.costNanoErg);
    } else {
      // date_desc (default)
      rentals.sort(
        (a, b) =>
          new Date(b.startedAt).getTime() - new Date(a.startedAt).getTime(),
      );
    }

    const total = rentals.length;
    rentals = rentals.slice(offset, offset + limit);

    return NextResponse.json({ rentals, total });
  } catch (err) {
    return NextResponse.json(
      { error: err instanceof Error ? err.message : "Internal server error" },
      { status: 500 },
    );
  }
}
