#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REQUIREMENTS="$SCRIPT_DIR/requirements.txt"

# Check Python version
if ! command -v python3 &>/dev/null; then
    echo "ERROR: python3 not found. Install Python 3.10+." >&2
    exit 1
fi

PYVER=$(python3 -c "import sys; print(f'{sys.version_info.major}.{sys.version_info.minor}')")
PYMAJOR=$(echo "$PYVER" | cut -d. -f1)
PYMINOR=$(echo "$PYVER" | cut -d. -f2)

if [ "$PYMAJOR" -lt 3 ] || { [ "$PYMAJOR" -eq 3 ] && [ "$PYMINOR" -lt 10 ]; }; then
    echo "ERROR: Python 3.10+ required (found $PYVER)." >&2
    exit 1
fi

echo "Python $PYVER detected."

# Install deps
echo "Installing dependencies..."
pip3 install -q -r "$REQUIREMENTS"

# Run load_test.py, forwarding all arguments
echo "Running load test..."
exec python3 "$SCRIPT_DIR/load_test.py" "$@"
