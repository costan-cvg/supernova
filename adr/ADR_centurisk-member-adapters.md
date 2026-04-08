# ADR: CentuRisk Member-Facing Adapters

## Status
Proposed

## Context

CentuRisk's member experience encompasses exposure self-service, renewal workflows, coverage views, data quality dashboards, and loss prevention views. These capabilities span multiple domain boundaries — the exposure core, quality model, recommendation engine, and policy data — but must present a coherent, member-scoped experience.

Several constraints shape this decision:

1. **UX must iterate independently of domain logic.** Member-facing experiences will evolve based on user feedback, pool-specific needs, and CentuRisk product direction. Changes to how data is displayed, filtered, or navigated should not require changes to the exposure core, quality model, or recommendation engine.
2. **All adapters render scoped views of the same underlying data.** A member sees only their own assets, quality scores, and recommendations. A pool administrator sees across members. The data is the same; the scope and presentation differ.
3. **Renewal is the highest-complexity member workflow.** It involves pre-populated valuations, member review (approve/modify/flag), flag resolution tracking, and bulk approval — all of which interact with the SOV pipeline and approval workflow.
4. **Phase 1 coverage views are read-only.** Members view coverage details and generate certificates but do not modify coverage data through the portal.
5. **Data quality and recommendation views are consumption adapters.** They render output from the quality model and recommendation engine, respectively, without modifying the underlying models.

## Decision

### Adapter Architecture: Member Experience as Adapters Over Exposure Core

The member experience is implemented as a set of **adapters** over the exposure core and related domain services. Each adapter renders a specific view of underlying data. The boundary is:

```
Exposure Core → Member/Pool Adapters
```

The core produces exposure views, quality scores, and recommendations. Adapters render them with appropriate scope and presentation. Changing the UX — adding a new visualization, restructuring navigation, altering filter behavior — does not require changing domain logic. Changing domain logic — adding a new quality dimension, modifying recommendation rules — does not require adapter changes as long as the output contracts are preserved.

This separation means:
- Frontend teams can iterate on member experience without backend coordination.
- Domain logic can be tested and validated independently of presentation.
- New adapters (mobile, API, third-party integrations) can be added without modifying the core.

### Exposure Self-Service

Exposure self-service provides interactive portfolio views for members to understand and manage their exposures. The adapter renders data from the exposure core through these views:

**Map View.** Geospatial overlay of assets with location data. Members see their portfolio geographically, enabling visual identification of concentration risk and coverage gaps. The map is a consumption view — it reads asset location data and renders it spatially.

**Tabular Asset List.** Filterable, sortable list of all assets in the member's portfolio. Supports filtering by any asset dimension (construction type, occupancy, location, coverage status). Supports drill-down from the portfolio view to an individual asset's detail page.

**TIV Accumulation Analysis.** Total Insured Value accumulation broken down by configurable dimensions:
- By geography (zip code, county, state, custom region)
- By construction type
- By occupancy class
- By custom dimensions defined at the pool level

This analysis helps members understand their risk concentration and supports renewal planning. The adapter queries the exposure core for asset data and performs aggregation for the requested dimensions.

### Renewal Experience

The renewal experience is the most complex member adapter, orchestrating interaction between members, CentuRisk-entered valuations, and the SOV pipeline/approval workflow.

**Pre-population with Proposed Valuations.**
CoreLogic (Marshall & Swift) data is manually entered by CentuRisk administrators as valuations in the asset registry. These valuations follow the normal valuation intake path and become the proposed replacement cost values members see during renewal. No external data feed or automated computation is required — the input is a CentuRisk admin action through the existing valuation intake.

**Member Response Options.** For each asset/valuation, members can:

| Action | System Behavior |
|---|---|
| **Approve** | Accept proposed values as-is. Asset enters the SOV pipeline with `source: renewal`. |
| **Modify and submit** | Member adjusts values and submits. Changes enter the SOV pipeline for validation, diffing, and approval routing. |
| **Flag for discussion** | Creates a queue item for the pool administrator. Does not submit changes. |

**Flag for Discussion.**
A flag creates a queue item for the pool administrator with:

```
RenewalFlag {
  asset_id:        UUID          -- the asset being flagged
  field_reference:  String | null -- specific field, if applicable
  member_note:     String        -- free-text explanation from the member
  created_at:      Timestamp
  state:           Enum { open, resolved }
  resolved_at:     Timestamp | null
  resolved_by:     UUID | null   -- the admin who resolved it
}
```

Discussion happens outside the system (phone call, email). The system tracks only the flag lifecycle — open or resolved. Resolution occurs when the administrator marks the flag done, not when a reply is sent. There is no in-system messaging or threading.

**Bulk Approval.**
Members can bulk-approve "clean" items — assets with no unresolved flags. The definition of "clean" is:
- No flags at all, OR
- All flags have been resolved

Items with any open (unresolved) flag require individual review and cannot be included in a bulk approval action. This prevents members from accidentally skipping items that need attention.

### Coverage Views (Read-Only in Phase 1)

Coverage views are consumption-only in Phase 1. Members can:

- **View coverage details.** See current coverage terms, limits, deductibles, and conditions for each asset or policy.
- **Generate Certificates of Insurance on demand.** Produce a certificate for a specific asset or coverage line, formatted for the member's use (e.g., providing to a lender or regulatory body).
- **See field-level coverage differentials across policy periods.** Members see exactly which fields changed on which assets between period A and period B. This is not a summary ("Asset X changed") but a field-level comparison:

```
CoverageDifferential {
  asset_id:        UUID
  field_name:      String
  period_a_value:  Any           -- value at period A's effective date
  period_b_value:  Any           -- value at period B's effective date
  change_type:     Enum { added, removed, modified, unchanged }
}
```

This composes naturally with the exposure core's field-level mutation store — the differential is a query comparing resolved field values at each period's effective date and surfacing the deltas.

### Data Quality Dashboard

The data quality dashboard renders the quality model's output for a member's portfolio. It does not compute scores — it consumes them from the quality model and presents them through these views:

- **Composite scores.** Overall data quality score for the member's portfolio, providing a single indicator of exposure data health.
- **Per-asset breakdowns.** Quality scores for individual assets, allowing members to identify which assets have the poorest data quality.
- **Gap identification by dimension.** Specific gaps organized by quality dimension (completeness, accuracy, timeliness, consistency). Members see which dimensions are weakest and which specific fields are missing or stale.
- **Highest-impact actions.** The dashboard highlights which actions would produce the largest quality improvement — connecting to the recommendation engine's output to show prioritized next steps.

### Loss Prevention Views

Loss prevention views render the recommendation engine's output for a member's specific exposure profile. The adapter consumes `Recommendation` objects and presents them as:

- **Prioritized recommendation list.** Recommendations sorted by priority (high/moderate/low), showing action, rationale, and expected quality impact for each.
- **Asset-mapped recommendations.** Recommendations linked to specific assets are viewable from the asset detail page, giving members context for why a recommendation applies to a particular property.
- **Portfolio-level recommendations.** Recommendations with no `asset_id` (portfolio-level) are presented separately as systemic improvements the member can make across their portfolio.

## Alternatives Considered

### Embed Domain Logic in the Member Portal

Rejected. Embedding quality scoring, recommendation generation, or approval routing in the member-facing layer would couple UX iteration to domain logic changes. Every frontend change risks breaking business rules; every business rule change risks breaking the UI. The adapter pattern keeps these concerns cleanly separated.

### Build a Full In-System Messaging Feature for Renewal Flags

Rejected. CentuRisk confirmed that discussion happens outside the system (phone, email). Building a messaging or threading feature would add significant complexity for a workflow that is inherently interpersonal. The flag-with-note model is sufficient — it captures the member's concern, creates visibility for the administrator, and tracks resolution state.

### Summary-Level Coverage Differentials (Asset Changed vs. Field Changed)

Rejected. CentuRisk specified field-level differentials. Summary-level differentials ("Asset X changed") do not give members enough information to understand what changed during a renewal or policy period transition. Field-level comparison is more informative and composes naturally with the existing field-level mutation store.

### Compute TIV Accumulation in the Frontend

Rejected. TIV accumulation analysis across geography, construction type, and custom dimensions involves aggregation over potentially thousands of assets. This is a backend query responsibility — the adapter requests the aggregation from the exposure core, which can use indexed queries and caching. The frontend renders the result but does not perform the computation.

### Editable Coverage Views in Phase 1

Rejected. Coverage data originates from pool-level policy administration, not member self-service. Allowing members to edit coverage data introduces complex validation, approval, and synchronization concerns that are not needed in Phase 1. Read-only views with on-demand certificate generation deliver immediate value without the complexity.

## Consequences

**Positive:**
- The adapter architecture allows the member experience to evolve rapidly based on user feedback without risking domain logic stability.
- Each adapter is independently deployable — improvements to the data quality dashboard do not require changes to the renewal workflow.
- The same underlying data serves member views, pool admin views, and future API consumers through different adapters with different scope.
- Renewal flags with open/resolved states provide enough workflow structure without over-engineering an in-system messaging feature.
- Field-level coverage differentials give members precise visibility into what changed across policy periods.

**Negative / Trade-offs:**
- The adapter pattern requires that all domain contracts (quality scores, recommendations, exposure views) remain stable. Breaking changes to domain output contracts cascade to all adapters.
- Bulk approval logic ("clean" = no unresolved flags) is simple but may need refinement if members want more nuanced criteria (e.g., approve items below a valuation threshold).
- CoreLogic/Marshall & Swift data entry is manual — CentuRisk admin labor is required for every renewal cycle. This is an operational cost that scales with the number of members and assets.
- Coverage views being read-only in Phase 1 means members cannot correct coverage data errors through self-service — they must contact their pool administrator.

**New constraints:**
- The exposure core must support efficient TIV accumulation queries by arbitrary dimensions. This likely requires indexed aggregation paths, not just row-level queries.
- The field-level mutation store must support period-based comparison queries for coverage differentials.
- The renewal workflow depends on the SOV pipeline and approval workflow being operational — it is not standalone.
- The `RenewalFlag` lifecycle (open/resolved) must be integrated into the bulk approval eligibility check — the adapter must query flag state before allowing bulk actions.

## Implementation Plan

1. **Build the tabular asset list with filtering and drill-down.** Implement the simplest exposure self-service view — a filterable, sortable table of assets in a member's portfolio with drill-down to individual asset detail. This is the foundational adapter view that validates the adapter-over-core pattern. Testable by seeding asset data and verifying the member sees only their scoped portfolio.

2. **Add the map view as a second exposure visualization.** Layer geospatial rendering on top of the same asset data used by the tabular view. Members see their assets on a map. This validates that multiple adapters can render the same core data in different formats. Testable with assets that have location data.

3. **Build the TIV accumulation analysis.** Implement backend aggregation queries for TIV by geography, construction type, and custom dimensions. Render the results in the exposure self-service adapter. Testable by seeding a known portfolio and asserting correct accumulation totals.

4. **Build the renewal workflow: propose, approve, modify, flag.** Implement the renewal adapter that displays CentuRisk-entered proposed valuations, supports member approve/modify/flag actions, and submits approved/modified values to the SOV pipeline. Implement the `RenewalFlag` model with open/resolved states. This is the highest-value member interaction and the most complex adapter. Testable end-to-end: admin enters valuation, member sees proposed value, member approves or flags, result flows to pipeline or admin queue.

5. **Add bulk approval for clean renewal items.** Implement the bulk approval action that selects all items with no unresolved flags and submits them through the SOV pipeline. Testable by creating a mix of clean and flagged items and verifying only clean items are bulk-approved.

6. **Build read-only coverage views with certificate generation.** Implement coverage detail views and on-demand Certificate of Insurance generation. This is a consumption adapter — it reads coverage data and renders it. Testable by seeding coverage data and generating a certificate.

7. **Build field-level coverage differentials across policy periods.** Implement the period comparison query that compares resolved field values at two effective dates and surfaces deltas. Render as a field-level diff view. Testable by seeding two periods with known field changes and asserting the differential output.

8. **Build the data quality dashboard.** Implement the adapter that consumes quality model output and renders composite scores, per-asset breakdowns, dimension-level gaps, and highest-impact actions. Testable by seeding known quality scores and verifying the dashboard renders them correctly with proper prioritization.

9. **Build loss prevention views.** Implement the adapter that consumes recommendation engine output and renders prioritized recommendations mapped to the member's exposure profile. Testable by seeding recommendations and verifying they render with correct priority ordering and asset mapping.
