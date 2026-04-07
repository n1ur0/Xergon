#!/usr/bin/env bash
#
# codegen.sh — Regenerate TypeScript and Python client code from the
#              Xergon Relay OpenAPI 3.0.2 spec.
#
# Usage:
#   ./scripts/codegen.sh            # generate both TS and Python
#   ./scripts/codegen.sh typescript  # generate only TypeScript
#   ./scripts/codegen.sh python     # generate only Python
#
# Prerequisites:
#   - Node.js >= 18  (uses npx to fetch openapi-generator-cli)
#   - The SDK repos must be siblings of xergon-relay:
#       ../xergon-sdk/
#       ../xergon-sdk-python/
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RELAY_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
SPEC="$RELAY_DIR/docs/openapi.yaml"
TS_OUT="../xergon-sdk/src/generated"
PY_OUT="../xergon-sdk-python/src/xergon/generated"

# ── colours ──────────────────────────────────────────────────────────────
GREEN='\033[0;32m'; YELLOW='\033[1;33m'; RED='\033[0;31m'; NC='\033[0m'
info()  { printf "${GREEN}[codegen]${NC} %s\n" "$*"; }
warn()  { printf "${YELLOW}[codegen]${NC} %s\n" "$*"; }
error() { printf "${RED}[codegen]${NC} %s\n" "$*" >&2; exit 1; }

# ── ensure spec exists ───────────────────────────────────────────────────
if [ ! -f "$SPEC" ]; then
  error "OpenAPI spec not found at $SPEC"
fi

# ── ensure openapi-generator-cli is available ────────────────────────────
ensure_generator() {
  if command -v openapi-generator-cli &>/dev/null; then
    return
  fi

  if command -v npx &>/dev/null; then
    info "openapi-generator-cli not found globally; will use npx."
    return
  fi

  warn "openapi-generator-cli and npx not found. Attempting npm global install..."
  if command -v npm &>/dev/null; then
    npm install -g @openapitools/openapi-generator-cli
    if command -v openapi-generator-cli &>/dev/null; then
      info "Installed openapi-generator-cli via npm."
      return
    fi
  fi

  error "Cannot find or install openapi-generator-cli. Please install Node.js or npm first."
}

# ── runner that works with both global install and npx ───────────────────
OG="openapi-generator-cli"
if ! command -v openapi-generator-cli &>/dev/null && command -v npx &>/dev/null; then
  OG="npx --yes @openapitools/openapi-generator-cli"
fi

# ── TypeScript generation ────────────────────────────────────────────────
gen_typescript() {
  local out_dir="$RELAY_DIR/$TS_OUT"

  if [ ! -d "$(dirname "$out_dir")" ]; then
    error "TypeScript SDK directory not found at $(dirname "$out_dir"). Ensure xergon-sdk is a sibling of xergon-relay."
  fi

  # Clean previous output
  rm -rf "$out_dir"
  mkdir -p "$out_dir"

  info "Generating TypeScript (typescript-fetch) into $TS_OUT ..."

  $OG generate \
    -i "$SPEC" \
    -g typescript-fetch \
    -o "$out_dir" \
    --additional-properties=supportsES6=true,typescriptThreePlus=true,enumPropertyNaming=UPPERCASE

  info "TypeScript generation complete."
}

# ── Python generation ────────────────────────────────────────────────────
# The python generator creates a full package scaffold (setup.py, tests/, etc.)
# at the output root.  We generate into a temp dir and copy just the package
# contents into the SDK's source tree.
gen_python() {
  local py_sdk_dir="$RELAY_DIR/../xergon-sdk-python"
  local target_dir="$py_sdk_dir/src/xergon/generated"

  if [ ! -d "$py_sdk_dir" ]; then
    error "Python SDK directory not found at $py_sdk_dir. Ensure xergon-sdk-python is a sibling of xergon-relay."
  fi

  local tmp_dir
  tmp_dir="$(mktemp -d)"

  # Generate into temp directory
  info "Generating Python client into temp directory ..."
  $OG generate \
    -i "$SPEC" \
    -g python \
    -o "$tmp_dir" \
    --additional-properties=packageName=xergon.generated,enumPropertyNaming=UPPERCASE

  # Copy just the xergon/generated package into the SDK
  rm -rf "$target_dir"
  mkdir -p "$target_dir"

  if [ -d "$tmp_dir/xergon/generated" ]; then
    cp -R "$tmp_dir/xergon/generated/." "$target_dir/"
  else
    rm -rf "$tmp_dir"
    error "Expected output at $tmp_dir/xergon/generated/ not found. Generator layout may have changed."
  fi

  rm -rf "$tmp_dir"

  info "Python generation complete -> $target_dir"
}

# ── main ─────────────────────────────────────────────────────────────────
ensure_generator

if [ $# -eq 0 ]; then
  TARGETS=(typescript python)
else
  TARGETS=("$@")
fi

for target in "${TARGETS[@]}"; do
  case "$target" in
    typescript|ts)  gen_typescript ;;
    python|py)      gen_python ;;
    *)              error "Unknown target: $target. Use 'typescript' or 'python'." ;;
  esac
done

info "Done. Generated code is ready."
