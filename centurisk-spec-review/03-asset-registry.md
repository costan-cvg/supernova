# 03. Asset Registry (Resolved)

The Asset Registry is the foundational data model for the system. Five decisions were resolved covering hierarchy structure, temporal model, lifecycle interaction with downstream consumers, custom fields, and user tiers.

## 1. Hierarchy: Materialized Path, Maximum 5 Levels

**Question:** What is the realistic range of hierarchy depth, and how can we make queries hierarchy-independent?

**Answer:** 5 levels is a realistic default depth, but this should be configurable per pool. The hierarchy is essentially static once established, only changing during major events like corrections or restructuring.

**Decision:** Use materialized path pattern. Each node stores its full ancestry as a delimited string (e.g., `/pool-123/member-456/campus-789/building-012/unit-345`). Queries for descendants become prefix matches. Aggregation at any level is a GROUP BY on a prefix substring. The path is recomputed when a node moves, but moves are rare administrative operations.

**Spec Implications:**

- Hierarchy depth is configurable per pool, defaulting to 5 levels. Labels at each level are also configurable per pool.
- The core domain logic never traverses the tree. It operates on flat asset records that carry their lineage as data. The hierarchy is a property of each record, not a recursive structure the query engine navigates.
- Whether a pool uses 3 levels or 5, the queries are structurally identical — only the path length changes.
- Access control scoping is a prefix filter on the materialized path, composing cleanly with the relationship-based permission model from Cross-Cutting Decision 3.

## 2. Temporal Model: Field-Level Mutations with Configurable Resolution

**Question:** How does the system resolve asset state when exports, quality scores, and views need to answer "what does this asset look like as of date X?"

**Answer (evolved through three follow-ups):**

- Individual field changes have their own effective dates within a policy period. A building added mid-year on June 15 has that as its effective date. A replacement cost update in March has a different effective date than a construction class correction in September.
- The temporal resolution strategy is not a system-wide choice — it is a per-pool configurable rule. Some pools use record-level resolution, others use field-by-field, and rules may be conditional on asset value or other attributes.
- Phase 1 ships with Centurisk-configured rules per pool. The rule engine exists in the core, but no pool-facing configuration UI is built in Phase 1.

**Decision:** The storage model captures field-level mutations with per-field effective dates (the most granular option). A pluggable resolution strategy, configured per pool by Centurisk, determines how to compose "state of this asset as of date X" from those mutations. Record-level snapshot resolution can always be derived from field-level data, but not the reverse.

**Spec Implications:**

- Every field mutation is a timestamped fact with an effective date, stored in the exposure core.
- Current state is a projection over those facts, resolved by the pool's configured resolution strategy up to a given point in time.
- The resolution strategy is domain core logic (pure, deterministic) — not infrastructure. It evaluates configurable rules without knowing why a particular pool chose that rule.
- The architecture supports adding a pool-administrator-facing rule configuration UI in a future phase without changing the core.

## 3. Asset Lifecycle and Downstream Consumers

**Question:** What do downstream consumers (CAT exports, SOV generation, quality model) see when an asset has pending changes?

**Answer:** The effective date of the policy impacts what appears. The last approved value for the effective date appears in exports. A separate report can show the impact of pending changes based on effective date. The quality model should show last approved values based on an effective date (defaulting to current date), and also be able to show the score based on pending changes with a configurable effective date.

**Decision:** Asset state resolves along two axes: temporal (as of what effective date?) and approval state (approved-only or approved-plus-pending?). This produces four view modes:

- Approved values as of current date (the default view).
- Approved values as of a specific effective date (policy-period lookback).
- Approved + pending values as of current date (impact preview).
- Approved + pending values as of a specific effective date (impact analysis for a future or past date).

**Spec Implications:**

- Exports (CAT model, SOV generation) always resolve to approved values at a specific effective date.
- The quality model scores all four view modes, defaulting to approved-as-of-today.
- A separate impact report shows what pending changes would do to scores and export data for any effective date.

## 4. Custom Fields: Centurisk Configures, Pool Admin Modifies

**Question:** Who defines the initial set of custom fields during pool onboarding?

**Answer:** Initial setup is on Centurisk. The pool administrator can modify after the fact. Changes like these should have an auditable change history for Centurisk system admins.

**Decision:** Custom fields are defined by Centurisk during pool onboarding and modifiable by the pool administrator afterward. All changes to custom field configuration carry an auditable change history visible to Centurisk system admins — who changed what, when, and from what prior state.

## 5. New User Tier: Centurisk System Admin

**Discovery:** The custom fields audit requirement introduced a third user tier not present in the original document: the Centurisk system admin. This role operates above pool administrators, with visibility across pools and access to configuration audit trails.

**Decision:** The access model has three tiers: member user, pool administrator, and Centurisk system admin. The Centurisk system admin role needs to be explicitly defined in the role-based access model with cross-pool visibility and configuration audit access.
