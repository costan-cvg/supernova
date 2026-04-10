#!/usr/bin/env bash
# RiskStar RMIS — Project Setup
#
# Installs dependencies, builds the project, runs tests,
# and optionally onboards sample data.
#
# Usage: ./scripts/setup-project.sh

set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

info()  { echo -e "${BLUE}[info]${NC}  $1"; }
ok()    { echo -e "${GREEN}[ok]${NC}    $1"; }
warn()  { echo -e "${YELLOW}[warn]${NC}  $1"; }
fail()  { echo -e "${RED}[fail]${NC}  $1"; exit 1; }

echo ""
echo "==============================="
echo "  RiskStar RMIS — Project Setup"
echo "==============================="
echo ""

# ── Check prerequisites ─────────────────────────────────────────────────────

info "Checking prerequisites..."

# Rust
if command -v cargo &>/dev/null; then
    RUST_VER=$(rustc --version | awk '{print $2}')
    ok "Rust $RUST_VER"
elif [ -f "$HOME/.cargo/bin/cargo" ]; then
    export PATH="$HOME/.cargo/bin:$PATH"
    RUST_VER=$(rustc --version | awk '{print $2}')
    ok "Rust $RUST_VER (from ~/.cargo/bin)"
else
    fail "Rust not found. Install from https://rustup.rs"
fi

# Node.js
if command -v node &>/dev/null; then
    NODE_VER=$(node --version)
    ok "Node.js $NODE_VER"
else
    warn "Node.js not found — Playwright E2E tests will not be available"
fi

# Python 3
if command -v python3 &>/dev/null; then
    PY_VER=$(python3 --version | awk '{print $2}')
    ok "Python $PY_VER"
else
    warn "Python 3 not found — onboarding script requires it"
fi

echo ""

# ── Build the project ────────────────────────────────────────────────────────

info "Building the workspace (this may take a few minutes on first run)..."
cargo build --workspace 2>&1 | tail -3
ok "Workspace built"

echo ""

# ── Run Rust tests ───────────────────────────────────────────────────────────

info "Running Rust tests..."
RUST_TEST_OUTPUT=$(cargo test 2>&1)
RUST_PASSED=$(echo "$RUST_TEST_OUTPUT" | grep "^test result" | awk '{sum += $4} END {print sum}')
RUST_FAILED=$(echo "$RUST_TEST_OUTPUT" | grep "^test result" | awk '{sum += $6} END {print sum}')

if [ "$RUST_FAILED" -gt 0 ] 2>/dev/null; then
    warn "$RUST_PASSED passed, $RUST_FAILED failed"
    echo "$RUST_TEST_OUTPUT" | grep "FAILED"
else
    ok "$RUST_PASSED Rust tests passed"
fi

echo ""

# ── Install Playwright (optional) ───────────────────────────────────────────

if command -v node &>/dev/null; then
    info "Installing Node.js dependencies..."
    npm install --silent 2>&1 | tail -1
    ok "npm packages installed"

    if [ ! -d "$HOME/.cache/ms-playwright" ] && [ ! -d "$HOME/Library/Caches/ms-playwright" ]; then
        info "Installing Playwright browsers (one-time download)..."
        npx playwright install chromium 2>&1 | tail -1
        ok "Chromium installed for Playwright"
    else
        ok "Playwright browsers already installed"
    fi

    info "Running E2E tests..."
    E2E_OUTPUT=$(npx playwright test 2>&1) || true
    E2E_PASSED=$(echo "$E2E_OUTPUT" | grep "passed" | tail -1)
    if echo "$E2E_OUTPUT" | grep -q "failed"; then
        warn "E2E: $E2E_PASSED"
    else
        ok "E2E: $E2E_PASSED"
    fi
else
    warn "Skipping Playwright setup (Node.js not installed)"
fi

echo ""

# ── Create data directory ────────────────────────────────────────────────────

mkdir -p data
ok "Data directory ready (./data/)"

# ── Summary ──────────────────────────────────────────────────────────────────

echo ""
echo "==============================="
echo "  Setup complete"
echo "==============================="
echo ""
echo "  Start the server:"
echo "    cargo run -p centurisk-server"
echo ""
echo "  Onboard sample data (server must be running):"
echo "    ./scripts/onboard-samples.sh"
echo ""
echo "  Then open http://localhost:3000"
echo ""

if [ -n "${HONEYCOMB_API_KEY:-}" ]; then
    ok "HONEYCOMB_API_KEY is set — traces will export to Honeycomb"
else
    info "Set HONEYCOMB_API_KEY to enable trace export to Honeycomb"
fi

echo ""
