/**
 * Custom Next.js server with WebSocket proxy support.
 *
 * WHY: Next.js App Router does not natively support WebSocket upgrades.
 * The rewrite rule in next.config.ts for /ws/* only works for HTTP requests,
 * not for WebSocket upgrade requests. This server intercepts /ws/status
 * upgrade requests and proxies them to the Xergon relay's WebSocket endpoint.
 *
 * HOW IT WORKS:
 * 1. Starts the standard Next.js dev/production server on the configured port.
 * 2. Creates a WebSocket server (noServer mode) that listens for upgrade requests.
 * 3. When a browser client connects to /ws/status, this server:
 *    a. Maintains a single upstream connection to the relay at ws://localhost:9090/ws/status
 *    b. Fans out relay messages to all connected browser clients
 * 4. Auto-reconnects to the relay with 3-second retry on disconnect/error.
 *
 * USAGE:
 *   Development:  npm run dev:ws        (uses this server instead of "next dev")
 *   Production:   npm run start:ws       (uses this server instead of "next start")
 *
 * ENVIRONMENT VARIABLES:
 *   PORT            - Port for this server (default: 3000)
 *   RELAY_WS_URL    - Upstream relay WebSocket URL (default: ws://localhost:9090/ws/status)
 *   RELAY_URL       - HTTP relay URL (passed through to Next.js, default: http://127.0.0.1:9090)
 */

import { createServer } from "node:http";
import { parse } from "node:url";
import next from "next";
import { WebSocketServer, WebSocket } from "ws";

const dev = process.env.NODE_ENV !== "production";
const hostname = "0.0.0.0";
const port = parseInt(process.env.PORT || "3000", 10);

const app = next({ dev, hostname, port });
const handle = app.getRequestHandler();

const RELAY_WS_URL =
  process.env.RELAY_WS_URL || "ws://localhost:9090/ws/status";
const RECONNECT_DELAY_MS = 3000;

app.prepare().then(() => {
  const server = createServer((req, res) => {
    handle(req, res);
  });

  // ── WebSocket server (noServer: we handle upgrades manually) ──
  const wss = new WebSocketServer({ noServer: true });

  // ── Upstream relay connection ──
  let relayWs = null;
  let reconnectTimer = null;
  let isShuttingDown = false;

  function connectRelay() {
    if (isShuttingDown) return;

    // Clear any pending reconnect timer
    if (reconnectTimer) {
      clearTimeout(reconnectTimer);
      reconnectTimer = null;
    }

    relayWs = new WebSocket(RELAY_WS_URL);

    relayWs.on("open", () => {
      console.log(
        `[ws-proxy] Connected to relay: ${RELAY_WS_URL} (${wss.clients.size} browser clients)`,
      );
    });

    relayWs.on("message", (data, isBinary) => {
      // Fan out every relay message to all connected browser clients
      const msg = isBinary ? data : data.toString();
      for (const client of wss.clients) {
        if (client.readyState === WebSocket.OPEN) {
          client.send(msg, { binary: isBinary });
        }
      }
    });

    relayWs.on("close", (code, reason) => {
      console.log(
        `[ws-proxy] Relay disconnected (code=${code}). Reconnecting in ${RECONNECT_DELAY_MS}ms...`,
      );
      relayWs = null;
      scheduleReconnect();
    });

    relayWs.on("error", (err) => {
      console.error(`[ws-proxy] Relay error: ${err.message}`);
      relayWs = null;
      scheduleReconnect();
    });
  }

  function scheduleReconnect() {
    if (isShuttingDown) return;
    reconnectTimer = setTimeout(() => {
      connectRelay();
    }, RECONNECT_DELAY_MS);
  }

  // ── Handle HTTP upgrade requests ──
  server.on("upgrade", (req, socket, head) => {
    const { pathname } = parse(req.url, true);

    if (pathname === "/ws/status") {
      // Handle the WebSocket upgrade for our proxy endpoint
      wss.handleUpgrade(req, socket, head, (ws) => {
        wss.emit("connection", ws, req);
      });
    } else {
      // Let Next.js handle any other upgrade requests (if any)
      handleUpgrade(req, socket, head);
    }
  });

  // ── Track browser client connections ──
  wss.on("connection", (ws, req) => {
    const clientIp = req.socket.remoteAddress;
    console.log(
      `[ws-proxy] Browser client connected from ${clientIp} (total: ${wss.clients.size})`,
    );

    ws.on("close", () => {
      console.log(
        `[ws-proxy] Browser client disconnected (total: ${wss.clients.size})`,
      );
    });
  });

  // ── Fallback: pass non-/ws/status upgrades to Next.js ──
  function handleUpgrade(req, socket, head) {
    // Next.js doesn't expose an upgrade handler in the app router,
    // so we just destroy the socket for unrecognized upgrade paths.
    socket.write(
      "HTTP/1.1 404 Not Found\r\n\r\nWebSocket path not found\r\n",
    );
    socket.destroy();
  }

  // ── Start ──
  server.listen(port, hostname, () => {
    console.log(`\n  > Xergon Marketplace + WS Proxy`);
    console.log(`  > Local:    http://localhost:${port}`);
    console.log(`  > WS Proxy: /ws/status -> ${RELAY_WS_URL}`);
    console.log(`  > Mode:     ${dev ? "development" : "production"}\n`);

    // Connect to the relay
    connectRelay();
  });

  // ── Graceful shutdown ──
  function shutdown() {
    isShuttingDown = true;

    if (reconnectTimer) {
      clearTimeout(reconnectTimer);
      reconnectTimer = null;
    }

    // Close all browser client connections
    for (const client of wss.clients) {
      client.close(1001, "Server shutting down");
    }

    // Close relay connection
    if (relayWs && relayWs.readyState === WebSocket.OPEN) {
      relayWs.close(1001, "Server shutting down");
    }

    // Close the HTTP server
    server.close(() => {
      console.log("[ws-proxy] Server shut down gracefully.");
      process.exit(0);
    });

    // Force exit after 5 seconds if graceful shutdown hangs
    setTimeout(() => {
      console.error("[ws-proxy] Forced shutdown after timeout.");
      process.exit(1);
    }, 5000);
  }

  process.on("SIGTERM", shutdown);
  process.on("SIGINT", shutdown);
});
