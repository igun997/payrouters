#!/usr/bin/env bash
# Resets the e2e database and runs the real PayRouters server (serving web/build) for Playwright.
set -euo pipefail
cd "$(dirname "$0")/../../.."          # repo root
PORT="${E2E_PORT:-8091}"
DB_NAME="payrouters_e2e_${PORT}"
ADMIN_URL="${E2E_ADMIN_DB_URL:-postgres://payrouters:payrouters@127.0.0.1:5435/postgres}"
export DATABASE_URL="postgres://payrouters:payrouters@127.0.0.1:5435/${DB_NAME}"

psql "$ADMIN_URL" -qc "DROP DATABASE IF EXISTS ${DB_NAME} WITH (FORCE)" >/dev/null
psql "$ADMIN_URL" -qc "CREATE DATABASE ${DB_NAME}" >/dev/null

BIN="${PAYROUTERS_BIN:-target/debug/payrouters-server}"
if [ ! -e "$BIN" ]; then
	echo "missing $BIN: build it with: rcargo --pull build -p payrouters-server (or set PAYROUTERS_BIN)" >&2
	exit 1
fi
if [ ! -x "$BIN" ]; then
	echo "$BIN is not executable; setting the execute bit" >&2
	chmod +x "$BIN" || { echo "cannot chmod +x $BIN: fix its permissions" >&2; exit 1; }
fi
[ -f web/build/index.html ] || { echo "missing web/build: run pnpm build in web/" >&2; exit 1; }

export LISTEN_ADDR="127.0.0.1:${PORT}"
export PUBLIC_BASE_URL="http://127.0.0.1:${PORT}"
export ADMIN_TOKEN="e2e-admin-token-0123456789abcdef"
export MASTER_KEY="AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE="
export WEB_DIR="${E2E_WEB_DIR:-web/build}"
export RUST_LOG="warn"
# Each Playwright test sends its own X-Forwarded-For (see e2e/support/fixtures.ts), so limits are per test.
export TRUSTED_PROXY_HEADER="x-forwarded-for"
# Generous request limits: the shared suite must never trip them. The failed-sign-in lockout keeps
# its default (10 failures per 5 minutes per client); rate-limit.spec.ts exercises it.
export RATE_GLOBAL_PER_MIN="${RATE_GLOBAL_PER_MIN:-6000}"
export RATE_SNAP_PER_MIN="${RATE_SNAP_PER_MIN:-6000}"
export RATE_SNAP_PAY_PER_MIN="${RATE_SNAP_PAY_PER_MIN:-6000}"
export RATE_HOOKS_PER_MIN="${RATE_HOOKS_PER_MIN:-6000}"
exec "$BIN"
