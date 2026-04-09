# ADR: CentuRisk RMIS Implementation Plan

## Status
Proposed

## Context

CentuRisk is a greenfield multi-tenant RMIS (Risk Management Information System) for public sector risk pools. Fourteen ADRs define the architecture, a systems design review identified 2 P0 and 6 P1 issues, and there is currently **no application code** -- only ADRs, source specs, and dev infrastructure scripts.

This ADR sequences the 14 delivery increments into concrete implementation work with parallelism opportunities, resolves the P0/P1 blockers from the systems design review, and breaks each increment into thin vertical slices per the value-first development philosophy.

**Tech Stack:** Rust + Axum + SQLite backend, Vanilla JS Web Components frontend, Cargo workspace.

## Decision

### P0/P1 Blocker Resolution Strategy

These must be resolved before or during implementation. They change the increment structure.

| Blocker | Severity | Resolution | When |
|---------|----------|------------|------|
| Cedar enforcement gap (data paths in Inc 2-4 have no auth) | P0 | Build `PolicyGate` trait in Inc 1 with `AllowAllPolicy` stub. Every handler calls it from day 1. Cedar replaces stub in Inc 5. | Inc 1 |
| Untyped `Any` in `FieldChange` | P0 | Define `FieldValue` typed discriminated union in `centurisk-core` contracts. Seven variants: Text, Number, Date, Bool, Enum, Money, Null. | Inc 0 (workspace setup) |
| Temporal resolution spike underspecified | P1 | Spike 1 must produce go/no-go with fallback decision tree. Gate Inc 2 on it. | Spike 1 |
| Expression language unspecified | P1 | Build minimal predicate evaluator (~150 LOC recursive descent parser). Shared by `ResolutionRule.condition` and `AccuracyRule.condition`. | Inc 0 / Inc 2 |
| Quality score invalidation chain | P1 | Eager for single mutations, debounced/deferred for bulk. Document in Inc 3. | Inc 3 |
| Bulk import deduplication criteria | P1 | Natural key fallback (`member_id + address + asset_type + year_built`). Import idempotency key. | Before Inc 11 |

### Cargo Workspace Structure

```
centurisk/
  Cargo.toml                    # workspace root
  crates/
    centurisk-core/             # Pure domain logic. NO I/O, NO async.
    centurisk-auth/             # PolicyGate trait, TenantContext, AllowAllPolicy, Cedar
    centurisk-db/               # SQLite persistence (rusqlite + r2d2)
    centurisk-api/              # Axum HTTP handlers and middleware
    centurisk-search/           # Search index + NL query layer (after Spike 2)
    centurisk-import/           # Bulk import pipeline (after Spike 3)
    centurisk-export/           # SOV generation, CAT export, CoI
    centurisk-notify/           # In-app notifications + email digest
    centurisk-web/              # Static Vanilla JS Web Components
    centurisk-server/           # Binary. Composition root. Wires everything.
```

**Dependency rules:**
- `centurisk-core` depends on NOTHING in workspace. Only: `rust_decimal`, `time`, `serde`, `uuid`.
- `centurisk-auth` depends on `centurisk-core` (types only).
- `centurisk-db` depends on `centurisk-core` + `centurisk-auth` (for `TenantContext`).
- `centurisk-api` depends on `core`, `auth`, `db`. Owns `axum`, `tokio`.
- `centurisk-search/import/export/notify` each depend on `core` but NOT on each other (enables parallel development).
- `centurisk-server` depends on everything (composition root).

### Dependency Graph and Parallelism

```
                     [Spike 1] --------gates--------> [Inc 2]
                         |
            [Spike 2]    |    [Spike 3]
            (background)  |   (background)
               |         |       |
               |    [Inc 1: Auth + Cedar Stub]
               |         |
               |    [Inc 2: Asset Registry] --------+------------------+
               |         |                          |                  |
               |    [Inc 3: Quality Scoring]        |           [Inc 13: Custom Fields]*
               |         |          \               |
               |    [Inc 4: SOV Pipeline]    [Inc 8: Recommendations]
               |         |          \
               |    [Inc 5: Cedar]  [Inc 10: Notifications (partial)]
               |    /    |     \
               |   /     |      \
          [Inc 9: NL]  [Inc 6: Member Portal]  [Inc 12: IO Adapters]
                         |
                    [Inc 7: Renewal]           [Inc 11: Bulk Import] <-- [Spike 3]
                         |
                    [Inc 14: Performance Validation]
```

*Inc 13 extension points stubbed from Inc 2; config UI deferred.

### Three Parallel Tracks (after Inc 4 completes)

| Track | Increments | Blocking Gate |
|-------|-----------|---------------|
| **A: Member Experience (critical path)** | Inc 5 -> Inc 6 -> Inc 7 | Inc 4 |
| **B: Intelligence** | Inc 8 (needs Inc 3) + Inc 9 (needs Inc 5 + Spike 2) | Inc 3 / Inc 5 |
| **C: Data Pipelines** | Inc 11 (needs Inc 4 + Spike 3) + Inc 12 (needs Inc 4 + Inc 5) | Inc 4 |

Inc 10 (Notifications) is cross-cutting: starts after Inc 4, extends as Inc 7 completes.

### Critical Path

```
Spike 1 (1w) -> Inc 1 (2w) -> Inc 2 (2w) -> Inc 3 (1w) -> Inc 4 (2w) -> Inc 5 (1w) -> Inc 6 (2w) -> Inc 7 (2w) -> Inc 14 (2w)
= ~15 weeks
```

Everything NOT on this path (Inc 8, 9, 10, 11, 12, 13) runs in parallel off the critical path.

**Timeline estimates:**
- 1 developer (serial): 18-20 weeks
- 2 developers (critical path + 1 parallel track): 15-17 weeks
- 3 developers (critical path + 2 parallel tracks): 14-16 weeks

---

## Implementation Plan

### Phase 0: Foundation (Week 1-2)

#### Spike 1: Temporal Resolution (1 week, BLOCKING)

**Goal:** Validate field-level state resolution at 50ms/single asset, 500ms/100 batch with 1M assets.

Create `spike-temporal` binary crate:
- Generate 1M assets, 30 fields each, ~90M mutations (power-law distribution)
- Benchmark 3 strategies against SQLite WAL mode:
  1. Indexed field-level scan: `(asset_id, field_name, effective_date DESC)`
  2. Pre-computed record-level snapshots regenerated on write
  3. **Hybrid** (most likely winner): field-level mutations as source of truth + `current_asset_state` materialized table updated in same transaction

**Pass/fail:** p95 single-asset < 50ms, 100-batch < 500ms. If only hybrid meets target, adopt it and document write-path overhead.

**Gate:** Inc 2 cannot start until Spike 1 has an affirmative outcome.

#### Spikes 2 & 3 (background, non-blocking)

- **Spike 2** (Search + Cedar): Evaluate SQLite FTS5 vs Tantivy. Must complete before Inc 9.
- **Spike 3** (Bulk Import): 1M-row pipeline throughput test. Must complete before Inc 11.

Both run in background during Inc 1-4.

#### Workspace Scaffolding (Day 1, alongside Spike 1)

- Create Cargo workspace with all crate directories (empty `lib.rs` stubs)
- Define `FieldValue` discriminated union in `centurisk-core` (P0 fix):

```rust
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum FieldValue {
    Text(String),
    Number(Decimal),        // rust_decimal for financial precision
    Date(Date),             // time::Date
    Bool(bool),
    Enum(String),           // variant name, validated at adapter layer
    Money { amount: Decimal, currency: String }, // ISO 4217
    Null,                   // explicit absence
}
```

- Define ID newtypes: `AssetId`, `PoolId`, `MemberId`, `ActorId` (UUID v7)
- Set up CI (cargo check, cargo test, cargo clippy)

---

### Phase 1: Auth + Asset Core (Weeks 2-5)

#### Increment 1: Authentication and Tenant Isolation (2 weeks)

| Slice | User Sees | Effort | Deps | Parallel? |
|-------|-----------|--------|------|-----------|
| 1.1 Cargo workspace + health endpoint | `curl localhost:3000/health` -> `{"status":"ok"}` | 30m | None | Yes with 1.2 |
| 1.2 Static frontend shell | Browser shows `<centurisk-app>` with nav sidebar | 60m | None | Yes with 1.1 |
| 1.3 SQLite + migration framework | Health returns `{"status":"ok","db":"connected"}`. Migration 001: users, pools, members, access_grants, audit_entries | 60m | 1.1 | Yes with 1.2 |
| 1.4 AuthenticatedUser + hardcoded auth middleware | Nav shows "Logged in as: System Admin" | 60m | 1.1-1.3 | No |
| 1.5 Login page + role selector | Login form with role dropdown, JWT session | 90m | 1.4 | Yes with 1.6 |
| 1.6 TenantContext + cross-tenant leak test | CI test: Pool A admin cannot see Pool B data | 60m | 1.3, 1.4 | Yes with 1.5 |
| 1.7 Role-scoped dashboard routing | CentuRisk Admin / Pool Admin / Member see different dashboards | 60m | 1.5 | No |

**Key deliverables in `centurisk-auth`:**

```rust
#[async_trait]
pub trait PolicyGate: Send + Sync {
    async fn authorize(&self, principal: &Principal, action: &Action, resource: &Resource) -> AuthzDecision;
    async fn visible_fields(&self, principal: &Principal, resource_type: &ResourceType, pool_id: &PoolId) -> Vec<String>;
}

pub struct AllowAllPolicy; // Logs every decision via tracing, always returns Permit
```

- `TenantContext` injected into all repository operations
- Cross-tenant leakage prevention test (permanent CI fixture, never removed)

#### Increment 2: Asset Registry Core (2 weeks)

| Slice | User Sees | Effort | Deps | Parallel? |
|-------|-----------|--------|------|-----------|
| 2.1 Asset domain types + in-memory store | `cargo test` passes for asset CRUD | 60m | 1.3 | Yes with 2.2 |
| 2.2 `<centurisk-asset-card>` mock component | Dashboard shows "Fire Station #7" card | 30m | 1.2 | Yes with 2.1 |
| 2.3 SQLite persistence + materialized path | Assets persisted, path prefix queries work | 60m | 2.1 | No |
| 2.4 Create asset API + form | "Add Building" form -> creates asset -> detail page | 90m | 2.2, 2.3 | No |
| 2.5 Asset list view + table | Tabular list, tenant-scoped, click-to-detail | 60m | 2.3, 2.4 | Yes with 2.6 |
| 2.6 Field-level mutation store | FieldMutation records with effective dates, approval state | 60m | 2.3 | Yes with 2.4, 2.5 |
| 2.7 Edit asset + history tab | Edit replacement cost, "History" tab shows changelog | 90m | 2.5, 2.6 | No |
| 2.8 Temporal resolution (as-of-date) | Date picker on detail page shows historical state | 90m | 2.7 | No |

**Key deliverables in `centurisk-core`:**
- `AssetIdentity`, `LifecycleState` (Draft -> Active -> PendingChange -> Archived)
- `MaterializedPath` with `ancestors()`, `is_descendant_of()`, `depth()`
- `FieldMutation` struct + `resolve_asset_state()` pure function (implements Spike 1 winner)
- All field values stored as `(asset_id, field_name, value_json)` rows -- custom fields work from day 1 without schema changes

**Gate:** CentuRisk admin creates a pool, member, and building. Asset visible with temporal state. All paths call `PolicyGate`. Cross-tenant test passes.

---

### Phase 2: Quality + Pipeline (Weeks 5-8)

#### Increment 3: Data Quality Scoring (1 week)

| Slice | User Sees | Effort | Parallel? |
|-------|-----------|--------|-----------|
| 3.1 Completeness scoring (pure function) | cargo test: correct score for varying field populations | 60m | Yes with 3.2, 3.3, 3.4 |
| 3.2 Accuracy rule engine + 3 starter rules | Tests: frame/sprinkler, cost/appraisal, VIN format | 60m | Yes with 3.1, 3.3, 3.4 |
| 3.3 Field-scoped recency scoring | Tests: updating field A doesn't reset field B staleness | 60m | Yes with 3.1, 3.2, 3.4 |
| 3.4 `<centurisk-quality-badge>` component | Asset cards show colored score badge (green/yellow/red) | 30m | Yes with 3.1, 3.2, 3.3 |
| 3.5 Composed scoring pipeline + quality API | Asset detail shows 3 scores + specific gaps | 90m | After 3.1-3.4 |
| 3.6 QualityEvent emission at thresholds | Fill missing field -> score increases, event logged | 60m | After 3.5, parallel with 3.7 |
| 3.7 Quality dashboard for member portfolio | Portfolio-level aggregation, worst-assets-first | 90m | After 3.5, parallel with 3.6 |

**P1 resolution:** Invalidation strategy: eager for single mutations, debounced for bulk. QualityEvents rate-limited during bulk import.

#### Increment 4: SOV Pipeline and Approval (2 weeks)

| Slice | User Sees | Effort | Parallel? |
|-------|-----------|--------|-----------|
| 4.1 SOVProcessingResult + diff engine | Tests: correct FieldChange records with typed FieldValue | 60m | No |
| 4.2 Validation stage | Tests: invalid values produce structured errors | 60m | Yes with 4.3 |
| 4.3 Approval state machine | Tests: new assets/valuations always pend, auto-approve checks | 60m | Yes with 4.2 |
| 4.4 Inline edit through pipeline | Member edits address -> auto-approve or PendingChange | 90m | After 4.2, 4.3 |
| 4.5 `<centurisk-approval-queue>` | Pool Admin sees pending changes with diffs, approves/rejects | 90m | After 4.4 |
| 4.6 Valuation approval + permission check | Replacement cost changes always pend, require valuation permission | 60m | After 4.5, parallel with 4.7 |
| 4.7 Four view modes (approved/provisional x current/as-of) | Toggle "Show pending changes" on asset detail | 60m | After 4.5, parallel with 4.6 |

**Gate:** Member edits replacement cost -> pipeline produces diff + quality assessment -> Pool Admin approves -> asset updates -> quality scores reflect change.

---

### Phase 3: Authorization + Member Portal (Weeks 8-12)

#### TRACK A: Critical Path

##### Increment 5: Cedar ABAC Hardening (1 week)

| Slice | User Sees | Effort |
|-------|-----------|--------|
| 5.1 Cedar policy engine integration | Tests: Cedar evaluate returns permit/deny | 90m |
| 5.2 Ten named profiles as Cedar policies | Test matrix: all profiles x scenarios | 90m |
| 5.3 Cedar enforcement on all endpoints | Member gets 403 on Pool Admin routes | 90m |
| 5.4 Field-level visibility enforcement | Restricted fields absent from JSON response | 60m |
| 5.5 Profile assignment UI | CentuRisk Admin assigns profiles via dropdown | 60m |
| 5.6 Three-user demo verification | Three browser windows, three roles, correct scoping | 30m |

##### Increment 6: Member Portal (2 weeks)

| Slice | User Sees | Effort | Parallel? |
|-------|-----------|--------|-----------|
| 6.1 Filtered asset list | Filter by construction type, occupancy, location | 60m | No |
| 6.2 Asset detail drill-down | Click row -> full detail with quality scores | 30m | After 6.1 |
| 6.3 TIV accumulation by geography | Bar chart: TIV by zip code | 90m | Yes with 6.5, 6.6 |
| 6.4 TIV by other dimensions | Same endpoint, different group_by | 30m | After 6.3 |
| 6.5 Map view (progressive enhancement) | Leaflet.js pins, fallback to table with lat/lng | 90m | Yes with 6.3, 6.6 |
| 6.6 Member quality dashboard | Portfolio composite, worst-first, actionable items | 60m | Yes with 6.3, 6.5 |
| 6.7 Empty state (new member guidance) | No assets -> onboarding guidance | 30m | After 6.1 |

**Usability validation checkpoint** after this increment with facilities managers.

##### Increment 7: Renewal Workflow (2 weeks)

| Slice | User Sees | Effort | Parallel? |
|-------|-----------|--------|-----------|
| 7.1 CentuRisk admin enters proposed valuations | Pre-population form per member | 90m | No |
| 7.2 Member sees renewal page | Proposed vs current values side-by-side | 60m | After 7.1 |
| 7.3 Member approves individual assets | Flows through SOV pipeline (source: renewal) | 60m | Yes with 7.4 |
| 7.4 Member flags for discussion | RenewalFlag lifecycle, visible to pool admin | 60m | Yes with 7.3 |
| 7.5 Pool admin flag queue | View flags, mark resolved | 60m | After 7.4 |
| 7.6 Bulk approval for clean items | No unresolved flags -> bulk approve | 60m | After 7.3, 7.5 |

#### TRACK B: Intelligence (parallel with Track A, starts after Inc 3/5)

##### Increment 8: Recommendations + Loss Events (1 week)

Can start after Inc 3 completes. Does NOT need Inc 5 (Cedar).

| Slice | User Sees | Effort |
|-------|-----------|--------|
| 8.1 Rule engine strategy + 3 starter rules | Tests: recommendations produced from asset state | 60m |
| 8.2 `<centurisk-recommendations>` list | Prioritized recommendations for member | 60m |
| 8.3 Asset-mapped recommendations | Recommendations on asset detail page | 30m |
| 8.4 Loss event intake form | `<centurisk-loss-event-form>` | 60m |
| 8.5 Loss events on asset detail | Stored events listed (not feeding recs yet) | 30m |

8.1-8.3 parallel with 8.4-8.5 (independent tracks).

##### Increment 9: NL Querying (2 weeks)

Requires Inc 5 + Spike 2 completion.

| Slice | User Sees | Effort |
|-------|-----------|--------|
| 9.1 Search index from assets | Standard fields indexed | 90m |
| 9.2 `<centurisk-search>` text input | Returns matching assets | 60m |
| 9.3 NL translation layer | "buildings over $5M" -> structured filter | 90m |
| 9.4 Low-confidence suggestions | Ranked alternatives for ambiguous queries | 60m |
| 9.5 Cedar field-level filter on search | Restricted fields not queryable | 60m |
| 9.6 Query telemetry + admin view | NLQueryEvent logging, unresolved rate | 60m |

#### TRACK C: Data Pipelines (parallel with Track A, starts after Inc 4)

##### Increment 11: Bulk Import (2-3 weeks)

Requires Inc 4 + Spike 3 completion.

7 slices following the 7-stage pipeline: Upload -> Parse -> ValidateSchema -> MapToExposureModel -> AssignAssetIds -> RunQualityScoring -> ProduceImportSummary. Each stage independently resumable.

| Slice | User Sees | Effort |
|-------|-----------|--------|
| 11.1 File upload stage | Drag-and-drop CSV/Excel, stored with checksum | 60m |
| 11.2 Parse stage | CSV/Excel -> rows with progress indicator | 90m |
| 11.3 Schema validation stage | Per-row validation results | 60m |
| 11.4 Map to exposure model + assign asset IDs | Natural key matching + new ID generation | 90m |
| 11.5 Quality scoring on imported data | Quality assessment per imported asset | 60m |
| 11.6 Import summary for admin review | Approve/reject with stats (asset count, quality, errors) | 90m |
| 11.7 Resumability verification | Fail at any stage, resume without re-running prior stages | 60m |

##### Increment 12: IO Adapters + SOV Generation (2 weeks)

Requires Inc 4 + Inc 5.

| Slice | User Sees | Effort |
|-------|-----------|--------|
| 12.1 `AppraisalIntakeV1` adapter | Submit appraisal, updates asset valuation | 90m |
| 12.2 SOV generation: Excel export | Configurable columns, pool-scoped | 90m |
| 12.3 SOV generation: PDF format | Formatted submission package | 60m |
| 12.4 Broker-specific templates | Save/load column configurations | 60m |
| 12.5 CAT export pre-flight validation | Readiness check with gap report | 60m |

12.1 and 12.2 can run simultaneously. 12.5 is independent.

#### Cross-Cutting

##### Increment 10: Notifications (1 week, extends over time)

Starts after Inc 4, extends after Inc 7.

| Slice | User Sees | Effort |
|-------|-----------|--------|
| 10.1 In-app notification system | Bell icon with unread count, notification panel | 90m |
| 10.2 Trigger wiring (quality, approvals, renewals) | Notifications appear for real events | 60m |
| 10.3 Acknowledgment tracking | Mark read, clear all | 30m |
| 10.4 Email digest job | Pool-configurable frequency, unacknowledged summary | 90m |

##### Increment 13: Custom Fields + Pool Config (1 week)

Extension points stubbed from Inc 2. This increment adds the admin UI.

| Slice | User Sees | Effort |
|-------|-----------|--------|
| 13.1 CentuRisk admin defines custom fields | Field name, type, required/recommended per pool | 90m |
| 13.2 Custom fields on asset forms/detail | Dynamic form rendering from field definitions | 60m |
| 13.3 Custom fields in quality scoring | Completeness and accuracy include custom fields | 60m |
| 13.4 Asset type composition | Type-specific field extensions visible per type | 60m |
| 13.5 Hierarchy depth + label config | Pool-level hierarchy customization | 60m |

---

### Phase 4: Hardening (Weeks 16-18)

#### Increment 14: Performance Validation (2 weeks)

| Slice | User Sees | Effort |
|-------|-----------|--------|
| 14.1 Synthetic data generator (1M assets, 2500 members) | Realistic test dataset | 90m |
| 14.2 Performance benchmark suite | All ADR targets validated | 90m |
| 14.3 Load testing (concurrent multi-role users) | System stable under load | 90m |
| 14.4 Circuit breakers + degradation testing | Progressive enhancement fallbacks verified | 60m |

---

## Parallelism Summary for Agent Swarms

When using multi-agent development, spawn agents per **feature slice**, not per architectural layer.

### Within-Increment Parallelism

| Increment | Parallel Groups |
|-----------|----------------|
| Inc 1 | {1.1, 1.2} -> 1.3 -> 1.4 -> {1.5, 1.6} -> 1.7 |
| Inc 2 | {2.1, 2.2} -> 2.3 -> {2.4, 2.5, 2.6} -> 2.7 -> 2.8 |
| Inc 3 | {3.1, 3.2, 3.3, 3.4} -> 3.5 -> {3.6, 3.7} |
| Inc 4 | 4.1 -> {4.2, 4.3} -> 4.4 -> 4.5 -> {4.6, 4.7} |
| Inc 5 | 5.1 -> 5.2 -> 5.3 -> {5.4, 5.5} -> 5.6 |
| Inc 6 | 6.1 -> 6.2 -> {6.3, 6.5, 6.6} |
| Inc 7 | 7.1 -> 7.2 -> {7.3, 7.4} -> 7.5 -> 7.6 |
| Inc 8 | {8.1-8.3} parallel with {8.4-8.5} |

### Cross-Increment Parallelism (after Inc 4)

```
Developer 1 (critical path): Inc 5 -> Inc 6 -> Inc 7 -> Inc 14
Developer 2 (Track B+C):     Inc 8 -> Inc 9 -> Inc 11
Developer 3 (Track C+cross): Inc 10 -> Inc 12 -> Inc 13
```

---

## Key External Crates

| Concern | Crate | Rationale |
|---------|-------|-----------|
| HTTP | `axum` 0.7+ | ADR stack requirement |
| Database | `rusqlite` (bundled) + `r2d2` pool | SQLite, embedded, WAL mode |
| Decimal | `rust_decimal` | Financial precision for Money fields |
| Date/Time | `time` | Pure Rust, no system dependency |
| UUID | `uuid` v7 | Time-ordered for index locality |
| Serialization | `serde` + `serde_json` | Universal |
| JWT | `jsonwebtoken` | IdP token validation |
| Migrations | `refinery` | Lightweight, works with rusqlite |
| Testing | `proptest` | Property-based for pure core |
| Tracing | `tracing` | Structured logging |
| Cedar | `cedar-policy` | ABAC policy engine (Inc 5) |

---

## Sizing Summary

| Increment | Size | Duration | On Critical Path? | Slices |
|-----------|------|----------|-------------------|--------|
| Spike 1 | M | 1 week | Yes | -- |
| Spike 2 | M | 1 week (background) | No (gates Inc 9) | -- |
| Spike 3 | M | 1 week (background) | No (gates Inc 11) | -- |
| Inc 1: Auth + Cedar Stub | L | 2 weeks | Yes | 7 |
| Inc 2: Asset Registry Core | L | 2 weeks | Yes | 8 |
| Inc 3: Data Quality Scoring | M | 1 week | Yes | 7 |
| Inc 4: SOV Pipeline + Approval | L | 2 weeks | Yes | 7 |
| Inc 5: Cedar Hardening | M | 1 week | Yes | 6 |
| Inc 6: Member Portal | L | 2 weeks | Yes | 7 |
| Inc 7: Renewal Workflow | L | 2 weeks | Yes | 6 |
| Inc 8: Recommendations + Loss Events | M | 1 week | No | 5 |
| Inc 9: NL Querying | L | 2 weeks | No | 6 |
| Inc 10: Notifications | M | 1 week | No | 4 |
| Inc 11: Bulk Import Pipeline | XL | 2-3 weeks | No | 7 |
| Inc 12: IO Adapters + SOV Gen | L | 2 weeks | No | 5 |
| Inc 13: Custom Fields + Pool Config | M | 1 week | No | 5 |
| Inc 14: Performance Validation | L | 2 weeks | Yes | 4 |
| **Total** | | **18-20 weeks (1 dev)** | | **84 slices** |

---

## Verification Plan

**Per-increment gate criteria** (each must pass before proceeding):
1. All `cargo test` pass (unit + integration)
2. Cross-tenant leakage test passes (from Inc 1 onward, never removed)
3. User-facing demo scenario works end-to-end (defined per increment above)
4. `PolicyGate` called on every data path (verified by audit log in AllowAllPolicy)

**End-to-end acceptance test** (after Inc 7):
1. CentuRisk Admin creates pool + member + 5 buildings
2. Quality scores computed, gaps visible
3. Member edits replacement cost -> SOV pipeline -> approval queue
4. Pool Admin approves -> value updates
5. Three users (admin/pool/member) see correctly scoped views
6. Renewal workflow: proposed values -> member approve/flag -> bulk approve
7. Recommendations visible for member portfolio

**Performance validation targets** (Inc 14):
- Single asset resolution p95 < 50ms at 1M assets
- 100-asset batch p95 < 500ms
- Search query p95 < 300ms with Cedar filtering
- SOV export < 5min for 1M assets
- Bulk import < 30min for 1M records

## Alternatives Considered

### Cedar Timing: Pull to Increment 2 vs Stub from Increment 1

**Option A (Pull Cedar to Inc 2):** Full Cedar integration before any data paths are built. Rejected because it delays user-facing value -- you can't demo asset CRUD until both auth and Cedar are complete, coupling two complex subsystems.

**Option B (Stub from Inc 1, chosen):** `PolicyGate` trait with `AllowAllPolicy` from day 1. Every handler calls it. Cedar replaces the stub in Inc 5. This preserves the delivery sequence's value-first property while ensuring no data path bypasses authorization by construction. `TenantContext` provides real security (pool/member isolation) throughout.

### Expression Language: External Crate vs Custom Parser

**Option A (CEL via `cel-interpreter`):** Google's expression language, designed for this use case. Rejected because the Rust crate is early-stage and adds dependency risk for a grammar we can specify in ~150 LOC.

**Option B (Rhai):** Full scripting language. Rejected as overkill for predicate evaluation and introduces a security surface.

**Option C (Custom minimal parser, chosen):** Hand-written recursive-descent parser for a small predicate grammar. Covers all ADR examples. ~150 LOC. No external dependency. Extensible without breaking the AST.

### Spike Timing: All Sequential vs Parallel

**All sequential (1 gates 2 gates 3):** Rejected. Spikes 2 and 3 don't gate early increments. Running them sequentially delays the start of implementation by 3 weeks instead of 1.

**Spike 1 blocking, Spikes 2 & 3 background (chosen):** Only Spike 1 gates Inc 2 (the temporal model is foundational). Spikes 2 and 3 run during Inc 1-4 and must complete before their respective consumers (Inc 9 and Inc 11).

## Consequences

**Positive:**
- Every increment produces demonstrable, user-facing value
- P0 blockers resolved structurally (PolicyGate trait, FieldValue union) rather than by hoping for later cleanup
- Three parallel tracks after Inc 4 enable 2-3x developer parallelism
- 84 slices at 30-90 min each provide fine-grained progress tracking
- Crate boundaries enable parallel development without merge conflicts

**Negative:**
- The PolicyGate stub means Increments 2-4 run with permissive auth -- `TenantContext` provides pool/member isolation but not role-based or field-level enforcement until Inc 5
- Custom expression language adds maintenance burden vs. adopting a standard (CEL)
- 10 crates in the workspace adds compilation and dependency management overhead
- Spike 1 is a hard gate -- if it fails, the entire temporal model must be redesigned before proceeding

**New constraints:**
- Every handler must call `PolicyGate` from Inc 1 onward (enforced by code review)
- Cross-tenant leakage test is permanent CI fixture
- Spike 1 must produce affirmative result before Inc 2 begins
- Custom field extension points must be stubbed from Inc 2 (per design review P2)
