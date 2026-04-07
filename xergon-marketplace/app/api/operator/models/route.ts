import { NextRequest, NextResponse } from "next/server";

const RELAY_BASE =
  process.env.XERGON_RELAY_BASE ?? "http://127.0.0.1:9090";

/**
 * GET /api/operator/models
 *
 * Proxies to the relay's GET /v1/models endpoint.
 * Returns model list with provider and pricing info.
 */
export async function GET(request: NextRequest) {
  try {
    const { searchParams } = new URL(request.url);
    const provider = searchParams.get("provider");

    const relayParams = new URLSearchParams();
    if (provider) relayParams.set("provider", provider);

    const qs = relayParams.toString();
    const relayUrl = `${RELAY_BASE}/v1/models${qs ? `?${qs}` : ""}`;

    const controller = new AbortController();
    const timeout = setTimeout(() => controller.abort(), 5000);

    const res = await fetch(relayUrl, { signal: controller.signal });
    clearTimeout(timeout);

    if (!res.ok) {
      return NextResponse.json(
        { error: "Failed to fetch models", status: res.status },
        { status: res.status },
      );
    }

    const data = await res.json();
    return NextResponse.json(data);
  } catch (err) {
    if ((err as Error).name === "AbortError") {
      return NextResponse.json({ error: "Relay timeout" }, { status: 504 });
    }
    return NextResponse.json(
      { error: "Internal server error", details: (err as Error).message },
      { status: 500 },
    );
  }
}
