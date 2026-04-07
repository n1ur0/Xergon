import { NextRequest } from "next/server";

const RELAY_BASE =
  process.env.XERGON_RELAY_BASE ?? "http://127.0.0.1:9090";

/**
 * SSE proxy: streams events from the Xergon relay's /v1/events endpoint
 * to the client via Server-Sent Events.
 *
 * Handles:
 * - Reconnection headers (Last-Event-ID passthrough)
 * - Graceful error handling when relay is unavailable
 * - Abort on client disconnect
 */
export async function GET(req: NextRequest) {
  const relayUrl = `${RELAY_BASE}/v1/events`;

  // Pass through Last-Event-ID for reconnection
  const lastEventId = req.headers.get("last-event-id");
  const headers: Record<string, string> = {
    Accept: "text/event-stream",
  };
  if (lastEventId) {
    headers["Last-Event-ID"] = lastEventId;
  }

  let relayRes: Response | null = null;

  try {
    relayRes = await fetch(relayUrl, {
      headers,
      signal: AbortSignal.timeout(5000), // 5s connection timeout
    });
  } catch {
    // Relay unreachable — return an empty SSE stream with a retry hint
    return new Response(
      `retry: 5000\n\nevent: error\ndata: ${JSON.stringify({ message: "Relay unreachable", reconnect: true })}\n\n`,
      {
        status: 200,
        headers: {
          "Content-Type": "text/event-stream",
          "Cache-Control": "no-cache, no-transform",
          Connection: "keep-alive",
          "X-Accel-Buffering": "no",
        },
      },
    );
  }

  if (!relayRes.ok || !relayRes.body) {
    // Relay returned error — return empty SSE stream
    return new Response(
      `retry: 10000\n\nevent: error\ndata: ${JSON.stringify({ message: "Relay returned error", status: relayRes.status, reconnect: true })}\n\n`,
      {
        status: 200,
        headers: {
          "Content-Type": "text/event-stream",
          "Cache-Control": "no-cache, no-transform",
          Connection: "keep-alive",
          "X-Accel-Buffering": "no",
        },
      },
    );
  }

  // Stream relay SSE response to client
  const stream = new ReadableStream({
    async start(controller) {
      const reader = relayRes!.body!.getReader();
      const decoder = new TextDecoder();

      try {
        while (true) {
          const { done, value } = await reader.read();
          if (done) break;
          const chunk = decoder.decode(value, { stream: true });
          controller.enqueue(new TextEncoder().encode(chunk));
        }
      } catch (err) {
        // Client disconnected or relay stream ended — stop gracefully
        if ((err as Error).name !== "AbortError") {
          // Try to send an error event before closing
          try {
            const errorMsg = `event: error\ndata: ${JSON.stringify({ message: "Stream interrupted", reconnect: true })}\n\n`;
            controller.enqueue(new TextEncoder().encode(errorMsg));
          } catch {
            // Controller may already be closed
          }
        }
      } finally {
        try {
          reader.releaseLock();
        } catch {
          // Already released
        }
        controller.close();
      }
    },

    cancel() {
      // Client disconnected
      try {
        relayRes?.body?.cancel();
      } catch {
        // Ignore
      }
    },
  });

  return new Response(stream, {
    status: 200,
    headers: {
      "Content-Type": "text/event-stream",
      "Cache-Control": "no-cache, no-transform",
      Connection: "keep-alive",
      "X-Accel-Buffering": "no",
    },
  });
}
