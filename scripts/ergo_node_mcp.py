#!/usr/bin/env python3
"""
Xergon Ergo Node MCP Server

Exposes Ergo node REST API operations as MCP tools so any MCP client
(including Hermes Agent) can interact with the local Ergo node.

Usage:
    uvx --with "mcp[server]" --with requests ergo_node_mcp.py
    # or, from the xergon-network directory:
    uvx --with-editable . --with "mcp[server]" --with requests scripts/ergo_node_mcp.py

Requires Ergo node at ERGONODE_URL (default: http://192.168.1.75:9052)
"""

import json
import sys
import os
from typing import Any

try:
    import requests
except ImportError:
    print("requests not installed. Run: uv pip install requests", file=sys.stderr)
    sys.exit(1)

# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------

# Default: Docker-published Ergo node on same host.
# Override with ERGONODE_URL env var in config.yaml env block.
# Inside Docker: http://host.docker.internal:9052
# Outside Docker: http://localhost:9053
# Legacy LAN IP:  http://192.168.1.75:9052
ERGONODE_URL = os.environ.get(
    "ERGONODE_URL",
    "http://host.docker.internal:9052"
    if os.environ.get("INSIDE_DOCKER") == "1"
    else "http://localhost:9053",
)
REQUEST_TIMEOUT = 30

# ---------------------------------------------------------------------------
# Ergo Node API client
# ---------------------------------------------------------------------------

class ErgoNodeClient:
    """Lightweight Ergo node API client."""

    def __init__(self, base_url: str):
        self.base_url = base_url.rstrip("/")
        self.session = requests.Session()
        self.session.headers.update({
            "Accept": "application/json",
            "Content-Type": "application/json",
        })

    def get(self, path: str, params: dict | None = None) -> dict[str, Any]:
        url = f"{self.base_url}{path}"
        resp = self.session.get(url, params=params, timeout=REQUEST_TIMEOUT)
        resp.raise_for_status()
        return resp.json()

    # ---- Node Info ----
    def get_info(self) -> dict[str, Any]:
        return self.get("/info")

    def get_node_state(self) -> dict[str, Any]:
        return self.get("/info/state")

    def get_peers(self) -> dict[str, Any]:
        return self.get("/info/peers")

    # ---- Blockchain ----
    def get_last_headers(self, n: int = 10) -> dict[str, Any]:
        return self.get(f"/blocks/lastHeaders/{n}")

    def get_header_by_id(self, header_id: str) -> dict[str, Any]:
        return self.get(f"/blocks/{header_id}/header")

    def get_block_at(self, height: int) -> dict[str, Any]:
        return self.get(f"/blocks/at/{height}")

    # ---- Boxes / UTXO ----
    def get_unspent_boxes(self, offset: int = 0, limit: int = 100) -> dict[str, Any]:
        return self.get(
            "/boxes/unspent",
            params={"offset": offset, "limit": limit},
        )

    def get_boxes_by_address(self, address: str, offset: int = 0, limit: int = 100) -> dict[str, Any]:
        return self.get(
            f"/boxes/byAddress/{address}",
            params={"offset": offset, "limit": limit},
        )

    def get_box_by_id(self, box_id: str) -> dict[str, Any]:
        return self.get(f"/boxes/{box_id}")

    def get_boxes_by_token_id(self, token_id: str, offset: int = 0, limit: int = 100) -> dict[str, Any]:
        """Get all unspent boxes containing a specific token ID (NFT or FT).
        Used to locate provider boxes, treasury boxes, and token boxes."""
        return self.get(
            f"/boxes/byTokenId/{token_id}",
            params={"offset": offset, "limit": limit},
        )

    # ---- Transactions ----
    def get_unconfirmed_transactions(self) -> dict[str, Any]:
        return self.get("/transactions/unconfirmed")

    def get_transactions_by_address(self, address: str, limit: int = 100) -> dict[str, Any]:
        return self.get(
            f"/transactions/byAddress/{address}",
            params={"limit": limit},
        )


# ---------------------------------------------------------------------------
# MCP Protocol Handler
# ---------------------------------------------------------------------------

PROTOCOL_VERSION = "2024-11-05"

TOOL_DEFINITIONS = [
    {
        "name": "get_info",
        "description": "Get Ergo node info — version, state, peer count, syncing status.",
        "inputSchema": {
            "type": "object",
            "properties": {},
            "required": [],
        },
    },
    {
        "name": "get_node_state",
        "description": "Get current Ergo blockchain state — height, difficulty, score.",
        "inputSchema": {
            "type": "object",
            "properties": {},
            "required": [],
        },
    },
    {
        "name": "get_peers",
        "description": "List connected Ergo node peers.",
        "inputSchema": {
            "type": "object",
            "properties": {},
            "required": [],
        },
    },
    {
        "name": "get_last_headers",
        "description": "Get the last N block headers from the Ergo blockchain.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "n": {
                    "type": "integer",
                    "description": "Number of headers to retrieve (default 10, max 100).",
                    "default": 10,
                },
            },
            "required": [],
        },
    },
    {
        "name": "get_unspent_boxes",
        "description": "Get unspent boxes (UTXOs) from the Ergo node. May require Explorer plugin.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "offset": {
                    "type": "integer",
                    "description": "Pagination offset (default 0).",
                    "default": 0,
                },
                "limit": {
                    "type": "integer",
                    "description": "Max boxes to return (default 100, max 1000).",
                    "default": 100,
                },
            },
            "required": [],
        },
    },
    {
        "name": "get_boxes_by_address",
        "description": "Get all UTXO boxes belonging to a P2PK or P2SH Ergo address.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "address": {
                    "type": "string",
                    "description": "Ergo address (P2PK, P2SH, or EIP-712).",
                },
                "offset": {
                    "type": "integer",
                    "description": "Pagination offset (default 0).",
                    "default": 0,
                },
                "limit": {
                    "type": "integer",
                    "description": "Max boxes to return (default 100).",
                    "default": 100,
                },
            },
            "required": ["address"],
        },
    },
    {
        "name": "get_box_by_id",
        "description": "Get a single UTXO box by its box ID (full id, 64-char hex).",
        "inputSchema": {
            "type": "object",
            "properties": {
                "box_id": {
                    "type": "string",
                    "description": "Full box ID (64-character hex string).",
                },
            },
            "required": ["box_id"],
        },
    },
    {
        "name": "get_boxes_by_token_id",
        "description": "Get all unspent boxes that contain a specific token (NFT or fungible token). Use this to locate provider boxes by their NFT token ID, or treasury boxes by their token ID.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "token_id": {
                    "type": "string",
                    "description": "Token ID (64-character hex string). For provider boxes this is the provider's NFT token ID.",
                },
                "offset": {
                    "type": "integer",
                    "description": "Pagination offset (default 0).",
                    "default": 0,
                },
                "limit": {
                    "type": "integer",
                    "description": "Max boxes to return (default 100, max 1000).",
                    "default": 100,
                },
            },
            "required": ["token_id"],
        },
    },
    {
        "name": "get_unconfirmed_transactions",
        "description": "Get transactions currently in the Ergo mempool (unconfirmed).",
        "inputSchema": {
            "type": "object",
            "properties": {},
            "required": [],
        },
    },
    {
        "name": "get_transactions_by_address",
        "description": "Get confirmed transactions involving a given Ergo address.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "address": {
                    "type": "string",
                    "description": "Ergo address.",
                },
                "limit": {
                    "type": "integer",
                    "description": "Max transactions to return (default 100).",
                    "default": 100,
                },
            },
            "required": ["address"],
        },
    },
]

# Maps tool names -> client methods + default args
TOOL_METHODS = {
    "get_info":                       (lambda c, a: c.get_info()),
    "get_node_state":                 (lambda c, a: c.get_node_state()),
    "get_peers":                      (lambda c, a: c.get_peers()),
    "get_last_headers":                (lambda c, a: c.get_last_headers(a.get("n", 10))),
    "get_unspent_boxes":              (lambda c, a: c.get_unspent_boxes(a.get("offset", 0), a.get("limit", 100))),
    "get_boxes_by_address":           (lambda c, a: c.get_boxes_by_address(a["address"], a.get("offset", 0), a.get("limit", 100))),
    "get_box_by_id":                  (lambda c, a: c.get_box_by_id(a["box_id"])),
    "get_boxes_by_token_id":          (lambda c, a: c.get_boxes_by_token_id(a["token_id"], a.get("offset", 0), a.get("limit", 100))),
    "get_unconfirmed_transactions":   (lambda c, a: c.get_unconfirmed_transactions()),
    "get_transactions_by_address":    (lambda c, a: c.get_transactions_by_address(a["address"], a.get("limit", 100))),
}


# ---------------------------------------------------------------------------
# MCP Stdio Transport
# ---------------------------------------------------------------------------

def send_response(req_id: Any, result: Any) -> None:
    """Send a JSON-RPC success response."""
    msg = {"jsonrpc": "2.0", "id": req_id, "result": result}
    sys.stdout.write(json.dumps(msg) + "\n")
    sys.stdout.flush()


def send_error(req_id: Any, code: int, message: str, data: Any = None) -> None:
    """Send a JSON-RPC error response."""
    err = {"jsonrpc": "2.0", "id": req_id, "error": {"code": code, "message": message}}
    if data is not None:
        err["error"]["data"] = data
    sys.stdout.write(json.dumps(err) + "\n")
    sys.stdout.flush()


def send_notification(method: str, params: dict | None = None) -> None:
    """Send a JSON-RPC notification (no id)."""
    msg = {"jsonrpc": "2.0", "method": method}
    if params is not None:
        msg["params"] = params
    sys.stdout.write(json.dumps(msg) + "\n")
    sys.stdout.flush()


def read_request() -> dict | None:
    """Read a single JSON-RPC request line from stdin. Returns None on EOF."""
    try:
        line = sys.stdin.readline()
        if not line:
            return None
        return json.loads(line.strip())
    except json.JSONDecodeError as e:
        send_error(None, -32700, f"Parse error: {e}")
        return None


def main() -> None:
    client = ErgoNodeClient(ERGONODE_URL)
    req_id = None

    while True:
        req = read_request()
        if req is None:
            break

        method = req.get("method", "")
        req_id = req.get("id")
        params = req.get("params", {})

        # ---- Initialize ----
        if method == "initialize":
            send_response(req_id, {
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {
                    "tools": {},
                },
                "serverInfo": {
                    "name": "xergon-ergo-node",
                    "version": "0.1.0",
                },
            })
            send_notification("initialized", {})

        # ---- Ping ----
        elif method == "ping":
            send_response(req_id, {"pong": True})

        # ---- List Tools ----
        elif method == "tools/list":
            send_response(req_id, {"tools": TOOL_DEFINITIONS})

        # ---- Call Tool ----
        elif method == "tools/call":
            tool_name = params.get("name", "")
            tool_args = params.get("arguments", {})

            if tool_name not in TOOL_METHODS:
                send_error(req_id, -32601, f"Unknown tool: {tool_name}")
                continue

            try:
                result = TOOL_METHODS[tool_name](client, tool_args)
                send_response(req_id, {
                    "content": [
                        {
                            "type": "text",
                            "text": json.dumps(result, indent=2),
                        }
                    ]
                })
            except requests.HTTPError as e:
                send_error(req_id, -32000, f"Ergo node request failed: {e.response.status_code} {e.response.reason}")
            except requests.RequestException as e:
                send_error(req_id, -32000, f"Ergo node connection error: {e}")
            except Exception as e:
                send_error(req_id, -32000, f"Unexpected error: {e}")

        # ---- Unknown method ----
        else:
            send_error(req_id, -32601, f"Unknown method: {method}")


if __name__ == "__main__":
    main()
