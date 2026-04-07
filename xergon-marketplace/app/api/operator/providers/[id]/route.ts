import { NextRequest, NextResponse } from "next/server";

const RELAY_BASE =
  process.env.XERGON_RELAY_BASE ?? "http://127.0.0.1:9090";

type RouteContext = { params: Promise<{ id: string }> };

/**
 * GET /api/operator/providers/[id]
 *
 * Proxies to the relay's GET /v1/providers/{id} endpoint.
 */
export async function GET(
  _request: NextRequest,
  context: RouteContext,
) {
  try {
    const { id } = await context.params;

    const controller = new AbortController();
    const timeout = setTimeout(() => controller.abort(), 5000);

    const res = await fetch(`${RELAY_BASE}/v1/providers/${encodeURIComponent(id)}`, {
      signal: controller.signal,
    });
    clearTimeout(timeout);

    if (!res.ok) {
      return NextResponse.json(
        { error: "Provider not found", status: res.status },
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

/**
 * PATCH /api/operator/providers/[id]
 *
 * Proxies to the relay's PATCH /v1/providers/{id} endpoint.
 * Supports pause/resume/remove actions via { action: "pause"|"resume"|"remove" } body.
 */
export async function PATCH(
  request: NextRequest,
  context: RouteContext,
) {
  try {
    const { id } = await context.params;
    const body = await request.json();

    const controller = new AbortController();
    const timeout = setTimeout(() => controller.abort(), 5000);

    const res = await fetch(`${RELAY_BASE}/v1/providers/${encodeURIComponent(id)}`, {
      method: "PATCH",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(body),
      signal: controller.signal,
    });
    clearTimeout(timeout);

    if (!res.ok) {
      let errorMsg = `Relay returned ${res.status}`;
      try {
        const errBody = await res.json();
        errorMsg = errBody.error ?? errBody.message ?? errorMsg;
      } catch {
        // ignore
      }
      return NextResponse.json(
        { error: "Provider update failed", details: errorMsg },
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
