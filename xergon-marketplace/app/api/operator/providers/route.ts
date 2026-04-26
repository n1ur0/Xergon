import { NextRequest, NextResponse } from "next/server";

import { RELAY_BASE } from "@/lib/api/server-sdk";

/**
 * GET /api/operator/providers
 *
 * Proxies to the relay's GET /v1/providers endpoint.
 * Supports query params: ?status=healthy&model=gpt-4&region=US&sort=aiPoints&order=desc
 */
export async function GET(request: NextRequest) {
  try {
    const { searchParams } = new URL(request.url);

    // Build relay query params
    const relayParams = new URLSearchParams();
    const status = searchParams.get("status");
    const model = searchParams.get("model");
    const region = searchParams.get("region");

    if (status && status !== "all") relayParams.set("status", status);
    if (model && model !== "all") relayParams.set("model", model);
    if (region && region !== "all") relayParams.set("region", region);

    const qs = relayParams.toString();
    const relayUrl = `${RELAY_BASE}/v1/providers${qs ? `?${qs}` : ""}`;

    const controller = new AbortController();
    const timeout = setTimeout(() => controller.abort(), 5000);

    const res = await fetch(relayUrl, { signal: controller.signal });
    clearTimeout(timeout);

    if (!res.ok) {
      return NextResponse.json(
        { error: "Failed to fetch providers", status: res.status },
        { status: res.status },
      );
    }

    const data = await res.json();
    return NextResponse.json(data);
  } catch (err) {
    if ((err as Error).name === "AbortError") {
      return NextResponse.json(
        { error: "Relay timeout", details: "Relay did not respond within 5 seconds" },
        { status: 504 },
      );
    }
    return NextResponse.json(
      { error: "Internal server error", details: (err as Error).message },
      { status: 500 },
    );
  }
}
