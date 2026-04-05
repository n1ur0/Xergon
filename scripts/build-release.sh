#!/usr/bin/env bash
# build-release.sh — Local release build script for Xergon
#
# Usage:
#   ./scripts/build-release.sh                    # current platform, auto-detect version
#   ./scripts/build-release.sh --target x86_64-unknown-linux-gnu
#   ./scripts/build-release.sh --version 0.2.0
#   ./scripts/build-release.sh --target aarch64-apple-darwin --version v1.0.0
#
# Produces:
#   dist/xergon-agent-{version}-{target}.tar.gz
#   dist/xergon-relay-{version}-{target}.tar.gz
#   dist/xergon-{version}-checksums.txt

set -euo pipefail

# ── Defaults ────────────────────────────────────────────────────────────────
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
DIST_DIR="$PROJECT_ROOT/dist"

# Auto-detect version from git tag, fallback to 0.1.0-dev
DEFAULT_VERSION="$(git describe --tags --exact-match 2>/dev/null || git describe --tags --abbrev=0 2>/dev/null || echo '0.1.0-dev')"
DEFAULT_VERSION="${DEFAULT_VERSION#v}"  # strip leading 'v' if present

# Auto-detect current platform
DEFAULT_TARGET="$(rustc -vV 2>/dev/null | grep '^host:' | sed 's/host: //' || echo 'unknown')"

VERSION="$DEFAULT_VERSION"
TARGET="$DEFAULT_TARGET"
CLEAN=false

# ── Parse arguments ─────────────────────────────────────────────────────────
while [[ $# -gt 0 ]]; do
  case "$1" in
    --target)
      TARGET="$2"
      shift 2
      ;;
    --version)
      VERSION="$2"
      # Strip leading 'v' if present
      VERSION="${VERSION#v}"
      shift 2
      ;;
    --clean)
      CLEAN=true
      shift
      ;;
    -h|--help)
      echo "Usage: $0 [OPTIONS]"
      echo ""
      echo "Options:"
      echo "  --target TARGET   Build target triple (default: ${DEFAULT_TARGET})"
      echo "  --version VERSION Version string (default: auto-detected from git tag)"
      echo "  --clean           Clean build artifacts before building"
      echo "  -h, --help        Show this help message"
      exit 0
      ;;
    *)
      echo "ERROR: Unknown option: $1" >&2
      echo "Use --help for usage information" >&2
      exit 1
      ;;
  esac
done

# ── Validation ──────────────────────────────────────────────────────────────
if [ ! -f "$PROJECT_ROOT/xergon-agent/Cargo.toml" ]; then
  echo "ERROR: Cannot find xergon-agent/Cargo.toml in $PROJECT_ROOT" >&2
  exit 1
fi

if [ ! -f "$PROJECT_ROOT/xergon-relay/Cargo.toml" ]; then
  echo "ERROR: Cannot find xergon-relay/Cargo.toml in $PROJECT_ROOT" >&2
  exit 1
fi

# ── Display build info ─────────────────────────────────────────────────────
echo "========================================"
echo " Xergon Local Release Build"
echo "========================================"
echo "  Version:  v${VERSION}"
echo "  Target:   ${TARGET}"
echo "  Output:   ${DIST_DIR}/"
echo "  Clean:    ${CLEAN}"
echo "========================================"
echo ""

# ── Clean if requested ─────────────────────────────────────────────────────
if [ "$CLEAN" = true ]; then
  echo "[1/5] Cleaning previous artifacts..."
  rm -rf "$DIST_DIR"
  cargo clean --manifest-path "$PROJECT_ROOT/xergon-agent/Cargo.toml" --target "$TARGET" 2>/dev/null || true
  cargo clean --manifest-path "$PROJECT_ROOT/xergon-relay/Cargo.toml" --target "$TARGET" 2>/dev/null || true
  echo "  Done."
else
  echo "[1/5] Skipping clean (use --clean to remove previous artifacts)"
fi

# ── Ensure target is installed ─────────────────────────────────────────────
echo "[2/5] Ensuring Rust target ${TARGET} is installed..."
if ! rustup target list --installed | grep -q "$TARGET"; then
  echo "  Installing target ${TARGET}..."
  rustup target add "$TARGET"
else
  echo "  Target ${TARGET} already installed."
fi

# ── Build ───────────────────────────────────────────────────────────────────
echo "[3/5] Building xergon-agent..."
cargo build --release --target "$TARGET" --manifest-path "$PROJECT_ROOT/xergon-agent/Cargo.toml"
echo "  xergon-agent built successfully."

echo "[3/5] Building xergon-relay..."
cargo build --release --target "$TARGET" --manifest-path "$PROJECT_ROOT/xergon-relay/Cargo.toml"
echo "  xergon-relay built successfully."

# ── Package ─────────────────────────────────────────────────────────────────
echo "[4/5] Packaging release artifacts..."
mkdir -p "$DIST_DIR"

AGENT_STAGING=$(mktemp -d)
RELAY_STAGING=$(mktemp -d)
trap "rm -rf '$AGENT_STAGING' '$RELAY_STAGING'" EXIT

# Agent package
cp "$PROJECT_ROOT/xergon-agent/target/$TARGET/release/xergon-agent" "$AGENT_STAGING/"
chmod +x "$AGENT_STAGING/xergon-agent"
if [ -f "$PROJECT_ROOT/xergon-agent/config.toml.example" ]; then
  cp "$PROJECT_ROOT/xergon-agent/config.toml.example" "$AGENT_STAGING/"
fi

AGENT_ARCHIVE="xergon-agent-v${VERSION}-${TARGET}.tar.gz"
tar czf "$DIST_DIR/$AGENT_ARCHIVE" -C "$AGENT_STAGING" .
echo "  Created: $AGENT_ARCHIVE"

# Relay package
cp "$PROJECT_ROOT/xergon-relay/target/$TARGET/release/xergon-relay" "$RELAY_STAGING/"
chmod +x "$RELAY_STAGING/xergon-relay"
if [ -f "$PROJECT_ROOT/xergon-relay/config.toml.example" ]; then
  cp "$PROJECT_ROOT/xergon-relay/config.toml.example" "$RELAY_STAGING/"
fi

RELAY_ARCHIVE="xergon-relay-v${VERSION}-${TARGET}.tar.gz"
tar czf "$DIST_DIR/$RELAY_ARCHIVE" -C "$RELAY_STAGING" .
echo "  Created: $RELAY_ARCHIVE"

# ── Checksums ───────────────────────────────────────────────────────────────
echo "[5/5] Generating SHA256 checksums..."
CHECKSUM_FILE="xergon-v${VERSION}-checksums.txt"

(
  echo "# SHA256 Checksums for v${VERSION}"
  echo "# Generated: $(date -u '+%Y-%m-%d %H:%M:%S UTC')"
  echo "# Target:    ${TARGET}"
  echo ""
  cd "$DIST_DIR"
  sha256sum "$AGENT_ARCHIVE"
  sha256sum "$RELAY_ARCHIVE"
) > "$DIST_DIR/$CHECKSUM_FILE"

echo "  Created: $CHECKSUM_FILE"

# ── Summary ─────────────────────────────────────────────────────────────────
echo ""
echo "========================================"
echo " Build Complete!"
echo "========================================"
echo ""
echo "Artifacts in ${DIST_DIR}/:"
ls -lh "$DIST_DIR/"*.tar.gz "$DIST_DIR/"*.txt 2>/dev/null || true
echo ""
echo "Verify with:"
echo "  cd dist && sha256sum -c xergon-v${VERSION}-checksums.txt"
echo ""
