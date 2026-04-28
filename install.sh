#!/usr/bin/env bash
set -e
# ─────────────────────────────────────────────────────────────────────────────
# Xergon Network — Installer
# Usage: curl -sSL https://github.com/n1ur0x/Xergon-Network/releases/latest/download/install.sh | sh
# ─────────────────────────────────────────────────────────────────────────────

BINARY_NAME="${BINARY_NAME:-xergon-agent}"
REPO_OWNER="n1ur0x"
REPO_NAME="Xergon-Network"

# ── Colour support ───────────────────────────────────────────────────────────
if [ -t 1 ]; then
  RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'; BLUE='\033[0;34m'
  BOLD='\033[1m'; RESET='\033[0m'
else
  RED=''; GREEN=''; YELLOW=''; BLUE=''; BOLD=''; RESET=''
fi
info()    { echo -e "${BLUE}[info]${RESET} $*"; }
success() { echo -e "${GREEN}[ok]${RESET}  $*"; }
warn()   { echo -e "${YELLOW}[warn]${RESET} $*"; }
error()  { echo -e "${RED}[error]${RESET} $*"; exit 1; }

# ── Platform detection ───────────────────────────────────────────────────────
OS="$(uname -s | tr '[:upper:]' '[:lower:]')"
ARCH="$(uname -m)"
case "$ARCH" in x86_64) ARCH=amd64 ;; aarch64|arm64) ARCH=aarch64 ;; esac

case "$OS-$ARCH" in
  linux-amd64)   TARGET="x86_64-unknown-linux-musl" ;;
  linux-aarch64) TARGET="aarch64-unknown-linux-musl" ;;
  darwin-amd64)  TARGET="x86_64-apple-darwin" ;;
  darwin-aarch64) TARGET="aarch64-apple-darwin" ;;
  *)
    error "Unsupported platform: $OS-$ARCH"
    ;;
esac

# ── Resolve latest tag ───────────────────────────────────────────────────────
info "Checking latest release…"
TAG="$(curl -sL "https://api.github.com/repos/${REPO_OWNER}/${REPO_NAME}/releases/latest" \
  | python3 -c "import json,sys; print(json.load(sys.stdin)['tag_name'])" 2>/dev/null)"
[ -z "$TAG" ] && error "Could not resolve latest release tag."
info "Latest version: ${TAG}"

BASE_URL="https://github.com/${REPO_OWNER}/${REPO_NAME}/releases/download/${TAG}"

# ── Download & extract ───────────────────────────────────────────────────────
XERGON_HOME="${HOME}/.xergon"
XERGON_BIN="${XERGON_HOME}/bin"
ASSET_NAME="${BINARY_NAME}-${TARGET}.tar.gz"
DOWNLOAD_URL="${BASE_URL}/${ASSET_NAME}"

info "Downloading ${ASSET_NAME}…"
mkdir -p "$XERGON_BIN"
cd "$XERGON_BIN"

if curl -fL "$DOWNLOAD_URL" -o "${ASSET_NAME}" 2>/dev/null; then
  tar -xzf "${ASSET_NAME}" --strip-components=1
  rm -f "${ASSET_NAME}"
  chmod +x "${XERGON_BIN}/${BINARY_NAME}"
  success "Installed ${BINARY_NAME} to ${XERGON_BIN}/"
else
  error "Download failed: ${DOWNLOAD_URL}\n\
    Ensure the release has been published with CI/CD."
fi

# ── PATH setup ───────────────────────────────────────────────────────────────
PATH_MARKER="# XERGON-NETWORK-PATH"
PATH_ENTRY='export PATH="$HOME/.xergon/bin:$PATH"'

add_to_path() {
  local file="$1"
  if [ -f "$file" ] && ! grep -qF "$PATH_MARKER" "$file" 2>/dev/null; then
    printf '\n%s\n%s\n' "$PATH_MARKER" "$PATH_ENTRY" >> "$file"
  fi
}

for f in "${HOME}/.bashrc" "${HOME}/.zshrc" "${HOME}/.profile"; do
  [ -f "$f" ] && add_to_path "$f"
done

if ! grep -qF "$PATH_MARKER" "${HOME}/.bashrc" 2>/dev/null; then
  touch "${HOME}/.bashrc"
  add_to_path "${HOME}/.bashrc"
fi
success "PATH configured."

# ── Config scaffold (only if no config exists) ───────────────────────────────
CONFIG_PATH="${XERGON_HOME}/config.toml"
if [ ! -f "$CONFIG_PATH" ]; then
  mkdir -p "${XERGON_HOME}"
  cat > "${CONFIG_PATH}" << 'EOFCFG'
# Xergon Agent configuration
# Edit this file or run: xergon-agent setup

[agent]
name = "anonymous"
region = "us-east"
gpu_mode = "mine-serve"

[relay]
url = "https://relay.xergon.gg"
EOFCFG
  success "Config written: ${CONFIG_PATH}"
else
  info "Config already exists, skipping."
fi

echo ""
success "Xergon Network installed! Run '${BINARY_NAME} --help' to get started."
