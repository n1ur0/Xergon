import { NextRequest, NextResponse } from "next/server";

const RELAY_BASE =
  process.env.XERGON_RELAY_BASE ?? "http://127.0.0.1:9090";

/**
 * POST /api/onboard
 *
 * Proxies provider onboarding requests to the relay's POST /v1/providers/onboard.
 * Returns the relay's response (provider_id, status, etc.) to the client.
 */
export async function POST(request: NextRequest) {
  try {
    const body = await request.json();

    // Validate required fields
    const { providerName, apiEndpoint, supportedModels, region } = body;
    if (!providerName || !apiEndpoint || !supportedModels?.length || !region) {
      return NextResponse.json(
        {
          error: "Missing required fields",
          details: {
            providerName: !providerName ? "required" : undefined,
            apiEndpoint: !apiEndpoint ? "required" : undefined,
            supportedModels: !supportedModels?.length ? "at least one model required" : undefined,
            region: !region ? "required" : undefined,
          },
        },
        { status: 400 },
      );
    }

    // Transform form data to relay onboard payload
    const payload = {
      name: providerName,
      endpoint: apiEndpoint,
      models: supportedModels,
      region,
      gpu: body.gpuType ?? undefined,
      gpu_count: body.gpuCount ?? undefined,
      vram_gb: body.vramPerGpu ?? undefined,
      contact_email: body.contactEmail ?? undefined,
      max_concurrent: body.maxConcurrentRequests ?? undefined,
      pricing_input: body.pricingInput ?? undefined,
      pricing_output: body.pricingOutput ?? undefined,
      specialties: body.specialties ?? undefined,
    };

    const controller = new AbortController();
    const timeout = setTimeout(() => controller.abort(), 10_000);

    const res = await fetch(`${RELAY_BASE}/v1/providers/onboard`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(payload),
      signal: controller.signal,
    });

    clearTimeout(timeout);

    if (!res.ok) {
      let errorMsg = `Relay returned ${res.status}`;
      try {
        const errBody = await res.json();
        errorMsg = errBody.error ?? errBody.message ?? errorMsg;
      } catch {
        // ignore parse failure
      }
      return NextResponse.json(
        { error: "Onboarding failed", details: errorMsg },
        { status: res.status },
      );
    }

    const data = await res.json();
    return NextResponse.json(data);
  } catch (err) {
    if ((err as Error).name === "AbortError") {
      return NextResponse.json(
        { error: "Request timed out", details: "Relay did not respond within 10 seconds" },
        { status: 504 },
      );
    }
    return NextResponse.json(
      { error: "Internal server error", details: (err as Error).message },
      { status: 500 },
    );
  }
}
