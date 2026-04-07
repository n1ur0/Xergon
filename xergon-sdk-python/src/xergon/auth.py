"""
HMAC-SHA256 authentication for Xergon Relay API.

The relay uses three headers for HMAC auth:
  X-Xergon-Public-Key:  User's Ergo public key (hex)
  X-Xergon-Timestamp:   Unix timestamp (seconds)
  X-Xergon-Signature:   HMAC-SHA256(body + timestamp, private_key)
"""

from __future__ import annotations

import hashlib
import hmac
import time
from typing import Dict, Optional


def hmac_sign(message: str, private_key_hex: str) -> str:
    """Generate HMAC-SHA256 signature of a message using a hex-encoded private key.

    Args:
        message: The string to sign (typically JSON body + timestamp).
        private_key_hex: Private key as hex string.

    Returns:
        Signature as hex string.
    """
    key_bytes = bytes.fromhex(private_key_hex)
    data = message.encode("utf-8")
    return hmac.new(key_bytes, data, hashlib.sha256).hexdigest()


def hmac_verify(message: str, signature_hex: str, private_key_hex: str) -> bool:
    """Verify an HMAC-SHA256 signature.

    Args:
        message: The original message.
        signature_hex: Signature as hex string.
        private_key_hex: Private key as hex string.

    Returns:
        Whether the signature is valid.
    """
    expected = hmac_sign(message, private_key_hex)
    return hmac.compare_digest(expected, signature_hex)


def build_hmac_payload(body: str, timestamp: int) -> str:
    """Build the signed payload string for Xergon HMAC auth.

    Format: JSON body + timestamp (Unix seconds).

    Args:
        body: JSON request body string.
        timestamp: Unix timestamp in seconds.

    Returns:
        Concatenated payload string.
    """
    return f"{body}{timestamp}"


class XergonAuth:
    """Manages HMAC authentication headers for Xergon Relay requests."""

    def __init__(
        self,
        public_key: Optional[str] = None,
        private_key: Optional[str] = None,
    ) -> None:
        self.public_key = public_key
        self.private_key = private_key

    def authenticate(self, public_key: str, private_key: str) -> None:
        """Set full keypair for HMAC auth."""
        self.public_key = public_key
        self.private_key = private_key

    def set_public_key(self, public_key: str) -> None:
        """Set only the public key (for wallet-managed signing)."""
        self.public_key = public_key
        self.private_key = None

    def clear(self) -> None:
        """Clear all credentials."""
        self.public_key = None
        self.private_key = None

    def build_headers(
        self,
        method: str,
        path: str,
        body: str = "",
    ) -> Dict[str, str]:
        """Build auth headers for a request.

        Args:
            method: HTTP method (e.g. "POST").
            path: Request path (e.g. "/v1/chat/completions").
            body: JSON request body string.

        Returns:
            Dictionary of auth headers to include in the request.
        """
        headers: Dict[str, str] = {}

        if not self.public_key:
            return headers

        headers["X-Xergon-Public-Key"] = self.public_key

        if self.private_key:
            timestamp = int(time.time())
            payload = build_hmac_payload(body, timestamp)
            signature = hmac_sign(payload, self.private_key)
            headers["X-Xergon-Timestamp"] = str(timestamp)
            headers["X-Xergon-Signature"] = signature

        return headers
