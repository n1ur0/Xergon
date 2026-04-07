#!/bin/bash
# =============================================================================
# Xergon Network -- System Deployment Setup
# =============================================================================
#
# Creates the xergon user, directories, and installs systemd service files.
# Run as root or with sudo.
#
# Usage:
#   sudo ./deploy/setup-systemd.sh
# =============================================================================

set -euo pipefail

XERGON_USER="xergon"
XERGON_GROUP="xergon"
CONFIG_DIR="/etc/xergon"
DATA_DIR="/var/lib/xergon"
LOG_DIR="/var/log/xergon"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

echo "=== Xergon Network System Setup ==="

# Create user/group if they don't exist
if ! id "$XERGON_USER" &>/dev/null; then
    echo "Creating user: $XERGON_USER"
    useradd -r -s /usr/sbin/nologin -d "$DATA_DIR" "$XERGON_USER"
else
    echo "User $XERGON_USER already exists"
fi

# Create directories
echo "Creating directories..."
mkdir -p "$CONFIG_DIR" "$DATA_DIR" "$LOG_DIR"
chown "$XERGON_USER:$XERGON_GROUP" "$CONFIG_DIR" "$DATA_DIR" "$LOG_DIR"
chmod 750 "$CONFIG_DIR" "$DATA_DIR" "$LOG_DIR"

# Install systemd service files
echo "Installing systemd service files..."
cp "$SCRIPT_DIR/xergon-agent.service" /etc/systemd/system/
cp "$SCRIPT_DIR/xergon-relay.service" /etc/systemd/system/
systemctl daemon-reload

# Copy example configs if real configs don't exist
if [ ! -f "$CONFIG_DIR/config.toml" ]; then
    if [ -f "xergon-agent/config.toml.example" ]; then
        cp xergon-agent/config.toml.example "$CONFIG_DIR/config.toml"
        echo "Copied agent config example to $CONFIG_DIR/config.toml"
        echo "  -> EDIT THIS FILE with your Ergo node URL, provider ID, etc."
    else
        echo "WARNING: No xergon-agent/config.toml.example found. Create $CONFIG_DIR/config.toml manually."
    fi
fi

if [ ! -f "$CONFIG_DIR/relay.toml" ]; then
    if [ -f "xergon-relay/config.toml.example" ]; then
        cp xergon-relay/config.toml.example "$CONFIG_DIR/relay.toml"
        echo "Copied relay config example to $CONFIG_DIR/relay.toml"
        echo "  -> EDIT THIS FILE with your relay settings."
    else
        echo "WARNING: No xergon-relay/config.toml.example found. Create $CONFIG_DIR/relay.toml manually."
    fi
fi

echo ""
echo "=== Setup complete ==="
echo ""
echo "Next steps:"
echo "  1. Edit $CONFIG_DIR/config.toml  (agent config)"
echo "  2. Edit $CONFIG_DIR/relay.toml    (relay config)"
echo "  3. sudo systemctl enable --now xergon-agent"
echo "  4. sudo systemctl enable --now xergon-relay"
echo ""
echo "Logs:  journalctl -u xergon-agent -f"
echo "       journalctl -u xergon-relay -f"
