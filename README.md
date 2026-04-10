# RiskStar RMIS

Risk Management Information System for public sector risk pools. Manages exposure data, quality scoring, approval workflows, and renewal cycles for municipal asset portfolios.

## Prerequisites

- **Rust** 1.75+ (stable) with `cargo` — [rustup.rs](https://rustup.rs)
- **Node.js** 18+ with `npm` — for Playwright E2E tests
- **Python 3** — for the onboarding script

## Quick Start

```bash
# Clone and set up
git clone <repo-url>
cd supernova
./scripts/setup-project.sh
```

The setup script checks prerequisites, builds the workspace, runs all tests (136 Rust + 17 Playwright E2E), installs Playwright browsers, and creates the data directory. Once it completes:

```bash
# Start the server
cargo run -p centurisk-server

# In another terminal — onboard sample pool data via the API
./scripts/onboard-samples.sh

# Open http://localhost:3000
```

The server creates a SQLite database at `./data/centurisk.db` on first run. The system admin user is created by migration. Pool data is imported through the onboarding API.

## Development

### Project Structure

```
supernova/
  Cargo.toml                          # Workspace root
  crates/
    centurisk-core/                   # Pure domain logic (no I/O, no async)
    centurisk-auth/                   # PolicyGate trait, Cedar ABAC, TenantContext
    centurisk-db/                     # SQLite persistence + migrations
    centurisk-api/                    # Axum HTTP handlers and middleware
    centurisk-search/                 # FTS5 search index + NL query translation
    centurisk-export/                 # SOV CSV export
    centurisk-import/                 # Bulk import pipeline (stub)
    centurisk-notify/                 # Notifications (stub)
    centurisk-web/                    # Static Vanilla JS Web Components
    centurisk-server/                 # Binary — composition root
    centurisk-perf/                   # Performance benchmark suite
    spike-temporal/                   # Spike 1: temporal resolution benchmark
  samples/                            # Sample SOV CSV files for onboarding
  scripts/                            # Onboarding and utility scripts
  e2e/                                # Playwright E2E tests
  adr/                                # Architecture Decision Records
```

### Running the Server

```bash
cargo run -p centurisk-server
```

Server listens on `http://localhost:3000`. Configuration via environment variables:

| Variable | Default | Description |
|----------|---------|-------------|
| `CENTURISK_DB_PATH` | `./data/centurisk.db` | SQLite database file path |
| `CENTURISK_STATIC_DIR` | `./crates/centurisk-web/static` | Static frontend assets directory |
| `HONEYCOMB_API_KEY` | *(none)* | Honeycomb API key for OpenTelemetry trace export |
| `RUST_LOG` | `info` | Log level filter (e.g. `debug`, `info,tower_http=debug`) |

### Onboarding Data

The database starts empty (with only the system admin from migration V003). To load sample data:

```bash
# Start the server first, then in another terminal:
./scripts/onboard-samples.sh
```

This calls `POST /api/onboard` for each pool directory in `samples/`, creating pools, members, assets, field mutations, and user accounts through the same API path that real onboarding would use.

To onboard a new pool programmatically:

```bash
curl -X POST localhost:3000/api/onboard -H 'Content-Type: application/json' -d '{
  "pool_name": "My Pool",
  "members": [{
    "member_name": "City of Example",
    "sov_csv": "asset_type,building_name,address,replacement_cost\nBuilding,City Hall,100 Main St,5000000"
  }]
}'
```

### Running Tests

```bash
# Rust unit + integration tests (136 tests)
cargo test

# Playwright E2E tests (17 tests)
npm install                    # First time only
npx playwright install chromium  # First time only
npm run test:e2e

# E2E with visible browser
npm run test:e2e:headed
```

The Playwright config auto-starts the server and onboards sample data via a global setup script before running tests.

### Database Migrations

Migrations are embedded in the `centurisk-db` crate via `refinery` and run automatically on server startup. Migration files are in `crates/centurisk-db/src/migrations/`:

| Migration | Tables |
|-----------|--------|
| V001 | pools, members, users, access_grants, audit_entries |
| V002 | assets, field_mutations |
| V003 | System admin user (seed) |
| V004 | renewals, renewal_proposals, renewal_flags |
| V005 | loss_events |
| V006 | notifications |
| V007 | custom_field_definitions |

To reset the database: `rm data/centurisk.db` and restart the server.

### Frontend

Vanilla JS Web Components with Shadow DOM. No build step, no npm bundler, no framework. ES modules loaded directly by the browser.

Static files are in `crates/centurisk-web/static/`. Edit and refresh — no compilation needed.

### Observability

Structured JSON logging to stdout by default. To export traces to Honeycomb:

```bash
HONEYCOMB_API_KEY=your-key cargo run -p centurisk-server
```

Traces are sent via OTLP/gRPC to `api.honeycomb.io` with service name `riskstar`. HTTP request spans from `tower-http`, API handler spans from `tracing::instrument`, and Cedar authorization decisions are all instrumented.

### Performance Benchmarks

```bash
# Generate 1M synthetic assets and benchmark against ADR targets
cargo run --release -p centurisk-perf

# Spike 1: temporal resolution benchmark (100K assets)
cargo run --release -p spike-temporal
```

## Architecture

See `adr/` for Architecture Decision Records. Key decisions:

- **Pure core / impure edges** — `centurisk-core` has zero I/O dependencies
- **Cedar ABAC** — 10 named profiles enforced on every endpoint with field-level visibility
- **Field-level mutations** — every field change is a timestamped fact; current state is a projection
- **Tenant isolation** — `TenantContext` required on every DB query; cross-tenant leak test in CI
- **SOV pipeline** — all data changes route through approval decisions (valuation changes always pend)

## Demo Users

After running `./scripts/onboard-samples.sh`, these users are available on the login page:

| User | Role | What they see |
|------|------|---------------|
| RiskStar Admin | System admin | All pools, all exposures |
| Demo Risk Pool Admin | Pool administrator | Springfield + Shelbyville exposures |
| City of Springfield User | Member user | Springfield exposures only, can edit |
| City of Springfield (Read-Only) | Member read-only | Springfield exposures, no edit, no valuation fields |
| Coastal Counties Pool Admin | Pool administrator | Oceanview exposures |
| City of Oceanview User | Member user | Oceanview exposures only |

## API Endpoints

| Method | Path | Description |
|--------|------|-------------|
| GET | `/health` | Health check with DB status |
| POST | `/api/login` | Login by user_id, returns JWT |
| GET | `/api/users` | List all users (for login selector) |
| GET | `/api/me` | Current authenticated user |
| GET | `/api/assets` | List exposures (filterable by type, lifecycle, search) |
| POST | `/api/assets` | Create a new exposure |
| GET | `/api/assets/:id` | Exposure detail with temporal resolution (`?as_of=`, `?before=`) |
| PUT | `/api/assets/:id/fields` | Edit fields (routed through SOV pipeline) |
| GET | `/api/assets/:id/mutations` | Field mutation history |
| GET | `/api/assets/:id/recommendations` | Computed recommendations |
| POST | `/api/assets/:id/loss-events` | Record a loss event |
| GET | `/api/assets/:id/loss-events` | List loss events |
| GET | `/api/quality/asset/:id` | Quality scores (completeness, accuracy, recency) |
| GET | `/api/quality/summary` | Pool-level quality summary (worst-first) |
| GET | `/api/approvals` | Pending approval queue (admin only) |
| POST | `/api/approvals/:id` | Approve or reject a pending mutation |
| GET | `/api/dashboard/overview` | Portfolio stats (TIV, asset counts) |
| GET | `/api/dashboard/tiv?group_by=city` | TIV accumulation by dimension |
| GET | `/api/search?q=...` | Natural language search |
| POST | `/api/onboard` | Onboard a new pool with members and SOV CSV |
| GET | `/api/export/sov?format=csv` | Export SOV as CSV download |
| GET | `/api/export/preflight` | Export readiness check with gap report |
| GET | `/api/notifications` | List notifications for current user |
| GET | `/api/notifications/count` | Unread notification count |
| POST | `/api/notifications/:id/acknowledge` | Acknowledge a notification |
| GET | `/api/renewals` | List renewal cycles |
| POST | `/api/renewals` | Create a renewal with proposed valuations |
| GET | `/api/custom-fields` | List custom field definitions |
| POST | `/api/custom-fields` | Create a custom field definition (admin only) |
