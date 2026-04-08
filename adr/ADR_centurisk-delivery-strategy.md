# ADR: CentuRisk Delivery Strategy and Phase Roadmap

## Status
Proposed

## Context

CentuRisk RMIS must be delivered incrementally. The spec defines a comprehensive system spanning exposure management, data quality, recommendations, SOV workflows, member self-service, NL querying, ABAC access control, bulk import, and multiple adapter layers. Attempting to build all of this before delivering value would violate the value-first development philosophy and introduce unacceptable risk.

The spec explicitly defers eight capabilities to later phases. These deferrals are intentional exclusions, not oversights. Each deferred item has a defined boundary contract so the Phase 1 architecture does not preclude future integration.

Additionally, three areas are identified as high-risk technical unknowns requiring spikes before committing to implementation approaches:
- Field-level temporal resolution at 1M asset scale
- Search index with field-level Cedar access control at scale
- Step function bulk import pipeline at target volume

The member portal UX is designed from inferred behavior, not validated with actual facilities managers. This is a known risk that must be mitigated before investing heavily in member-facing features.

Phase 1 scope: everything in the specification except the explicitly deferred items listed below.

## Decision

### Phase 1 Scope (Current Release)

Phase 1 delivers the complete CentuRisk RMIS as specified, including:

**Exposure Core:**
- Asset registry with system-generated stable IDs
- Configurable hierarchy (materialized path, up to 5 levels)
- Field-level temporal model with per-field effective dates
- Configurable temporal resolution strategies (per pool, CentuRisk-configured)
- Four view modes: approved/provisional x current/as-of-date
- Custom fields (CentuRisk defines, pool admin modifies, auditable history)
- Asset types via composition model (buildings, contents, vehicles, fine arts)
- Lifecycle states (Draft, Active, Pending Change, Archived)

**Data Quality Model:**
- Three-dimension scoring: completeness, accuracy, recency
- Field-scoped recency (not asset-scoped)
- CentuRisk-authored scoring rules, customizable per pool
- Pool-administrator-configurable notification thresholds
- QualityEvent emission at boundary crossing

**Recommendation Engine:**
- Rule-based engine (ML-ready output contract)
- CentuRisk-authored rules, opaque to users
- Stable output: `Recommendation { asset_id?, category, priority, action, rationale, expected_quality_impact }`
- expected_quality_impact as qualitative indicator (high/moderate/low)
- Loss event intake and storage (not yet feeding recommendations)

**SOV Pipeline and Approval Workflow:**
- Deterministic processing pipeline producing `SOVProcessingResult`
- Source discriminator (renewal, onboarding, inline edit, bulk import)
- Approval routing: new assets and valuation changes always require approval; other changes follow user's auto-approve setting
- Valuation-specific approval permissions

**Member-Facing Adapters:**
- Exposure self-service: portfolio views, maps (enhanced), tabular lists, drill-down, TIV analysis
- Renewal experience: pre-populated values (CoreLogic via CentuRisk admin entry), approve/modify/flag workflow, bulk approval for clean items
- Coverage views: read-only, field-level differentials across policy periods, Certificate of Insurance generation
- Data quality dashboard: composite scores, per-asset breakdowns, gap analysis
- Loss prevention views: prioritized recommendations per member

**Input/Output Adapters:**
- Onboarding: interactive guided flow + batch import pipeline
- Appraisal intake: `AppraisalIntakeV1` (single CentuRisk-defined format)
- SOV generation: configurable Excel/PDF formats, broker-specific templates
- CAT export: data readiness with pre-flight validation (no format-specific adapters)

**Cross-Cutting:**
- NL querying: search index with NL translation layer, custom field indexing, low-confidence suggestion with telemetry
- ABAC with Cedar Policy: four user categories, named profiles, field-level visibility enforced at search index
- Notifications: in-app primary, email digest, pool-configurable frequency
- Progressive enhancement: core works without external services

**Infrastructure:**
- Two-path authentication (federated for CentuRisk, hosted directory for pools)
- Logical multi-tenancy with changeable isolation strategy
- Member-scoped encryption with pool key grants
- Bulk import step function pipeline (CSV/Excel, seven stages, resumable)
- Performance testing at target scale (1M assets, 2,500 members)

### Explicitly Deferred Items with Boundary Contracts

Each deferred item defines its boundary contract so Phase 1 architecture accommodates future integration without rework.

---

#### 1. Valuation Estimator (Target: Phase 3, may pull to Phase 1)

**What it does:** Estimates replacement cost from asset attributes (construction type, square footage, occupancy, location, age). Replaces manual CoreLogic data entry during renewal pre-population.

**Why deferred:** Requires actuarial modeling expertise and validated estimation algorithms. The manual process (CentuRisk admin enters CoreLogic values) is sufficient for Phase 1.

**Boundary contract for future integration:**

```
ValuationEstimate {
  asset_id: AssetId,
  estimated_replacement_cost: Money,
  confidence: enum { High, Moderate, Low },
  estimation_method: String,          // e.g., "Marshall & Swift Residential V3"
  input_attributes_used: [String],    // Which asset fields fed the estimate
  input_attributes_missing: [String], // Fields that would improve confidence
  estimated_at: Timestamp,
  valid_until: Timestamp              // Estimates expire
}
```

**Integration point:** The Valuation Estimator produces `ValuationEstimate` records that feed into the existing valuation intake path. The exposure core already stores valuations with provenance; the estimator becomes another source alongside manual CentuRisk admin entry. The `source` discriminator on `SOVProcessingResult` would record `ValuationEstimator { estimate_id }`.

**Phase 1 accommodation:** The asset model stores `valuation_source: enum { ManualEntry, AppraisalIntake, ValuationEstimator }` from day one. Phase 1 only populates `ManualEntry` and `AppraisalIntake`. The enum is extensible without schema change.

---

#### 2. Policy and Coverage Management (Target: Phase 4)

**What it does:** Links exposure data to policies, manages endorsement workflows, calculates premiums based on exposure changes, and provides coverage-aware views.

**Why deferred:** Requires integration with policy administration systems and actuarial premium calculation logic. Phase 1 provides read-only coverage views sufficient for member self-service.

**Boundary contract for future integration:**

```
PolicyLink {
  asset_id: AssetId,
  policy_id: PolicyId,
  coverage_type: String,              // e.g., "Property", "Liability", "Auto"
  effective_date: Date,
  expiration_date: Date,
  coverage_limit: Money,
  deductible: Money,
  premium_allocation: Money?,         // Null until premium calc is built
  link_status: enum { Active, Pending, Expired }
}

EndorsementRequest {
  endorsement_id: UUID,
  policy_id: PolicyId,
  requested_by: UserId,
  changes: [AssetChange],            // References SOVProcessingResult diffs
  premium_impact: Money?,            // Null until premium calc available
  status: enum { Draft, Submitted, UnderReview, Approved, Rejected },
  submitted_at: Timestamp?,
  resolved_at: Timestamp?
}

PremiumCalculation {
  policy_id: PolicyId,
  calculation_id: UUID,
  effective_date: Date,
  factors: [PremiumFactor],
  base_premium: Money,
  adjusted_premium: Money,
  calculated_at: Timestamp
}
```

**Integration point:** The asset model's "segment linkage points" are where `PolicyLink` attaches. Phase 1 assets have a `segment_id: String?` field that stores an opaque identifier for the policy segment. Phase 4 replaces this with a full `PolicyLink` relationship. Coverage differential views in Phase 1 operate on temporal field comparisons; Phase 4 adds policy-period-aware comparisons.

**Phase 1 accommodation:** Coverage views are read-only. The `segment_id` field exists on assets but is not enforced or validated. Premium impact values are entered manually by pool administrators via an input adapter, not computed.

---

#### 3. Full Claims Lifecycle (Target: Phase 5)

**What it does:** Reserve accounting, adjuster assignment and workflows, claim adjudication, payment tracking, and loss-to-exposure attribution.

**Why deferred:** Claims management is a complex domain with regulatory requirements that vary by jurisdiction. Phase 1 includes only loss event intake to begin accumulating historical data.

**Boundary contract for future integration:**

```
Claim {
  claim_id: UUID,
  loss_event_id: LossEventId,        // Links to Phase 1 loss event intake
  asset_id: AssetId,
  policy_id: PolicyId?,               // Links to Phase 4 policy management
  claimant: {
    member_id: MemberId,
    contact_info: ContactInfo
  },
  status: enum {
    Open, UnderInvestigation, Reserved,
    InAdjudication, Settled, Closed, Reopened
  },
  reserves: [Reserve],
  payments: [Payment],
  adjuster_id: UserId?,
  opened_at: Timestamp,
  closed_at: Timestamp?
}

Reserve {
  reserve_id: UUID,
  claim_id: UUID,
  reserve_type: enum { CaseReserve, IBNRReserve },
  amount: Money,
  set_by: UserId,
  set_at: Timestamp,
  reason: String
}

Payment {
  payment_id: UUID,
  claim_id: UUID,
  amount: Money,
  payee: String,
  payment_type: enum { Indemnity, Expense, Recovery },
  paid_at: Timestamp,
  approved_by: UserId
}
```

**Integration point:** Phase 1's `LossEvent { asset_id, event_type, date, severity_estimate, description }` is the seed record. Phase 5's `Claim` references `loss_event_id`, linking the claim back to the original intake. The recommendation engine's future ML version consumes claim outcomes to improve loss prevention recommendations.

**Phase 1 accommodation:** The `LossEvent` contract is defined and intake is functional. Loss events are stored with full provenance. The intake form collects sufficient data for retrospective claim creation when Phase 5 is built. No claim-specific fields are added to the asset model in Phase 1.

---

#### 4. Scheduled and Formatted Reporting (Target: Phase 5)

**What it does:** Full reporting engine with scheduled delivery, cross-module reports, formatted output (PDF, Excel), distribution lists, and report templates.

**Why deferred:** NL querying and built-in exposure views serve the most common ad hoc reporting needs. The full reporting engine adds scheduled delivery and cross-module aggregation that are not required for Phase 1 operations.

**Boundary contract for future integration:**

```
ReportDefinition {
  report_id: UUID,
  name: String,
  description: String,
  query: StructuredQuery,             // Same query contract as NL layer output
  output_format: enum { PDF, Excel, CSV },
  template_id: TemplateId?,
  filters: [ReportFilter],
  grouping: [GroupByDimension],
  schedule: ReportSchedule?,
  distribution: [RecipientConfig],
  created_by: UserId,
  pool_id: PoolId
}

ReportSchedule {
  frequency: enum { Daily, Weekly, Monthly, Quarterly, Annual, Custom },
  custom_cron: String?,               // For Custom frequency
  delivery_time: TimeOfDay,
  timezone: Timezone,
  enabled: bool
}

ReportExecution {
  execution_id: UUID,
  report_id: UUID,
  triggered_by: enum { Schedule, Manual { user_id: UserId } },
  status: enum { Queued, Running, Completed, Failed },
  started_at: Timestamp?,
  completed_at: Timestamp?,
  output_location: String?,           // File path or download URL
  row_count: u64?
}
```

**Integration point:** The NL query layer's `StructuredQuery` output is the same contract the reporting engine uses. Reports are saved queries with schedule and formatting metadata. The search index, Cedar policy evaluation, and temporal resolution all apply identically to report execution as they do to interactive queries.

**Phase 1 accommodation:** NL querying produces `StructuredQuery` objects that are logged and could be saved. Phase 1 does not persist saved queries as reports, but the contract is stable for Phase 5 to build on. SOV generation and export adapters handle the most critical formatted output needs.

---

#### 5. Premium "What-If" Modeling (Target: unscheduled)

**What it does:** Interactive modeling of how exposure changes affect premiums. Users adjust asset values, coverage, or risk factors and see estimated premium impact in real time.

**Boundary contract for future integration:**

```
PremiumFactorSource {
  version: String,                    // Versioned adapter
  pool_id: PoolId,
  factors: [PremiumFactor]
}

PremiumFactor {
  factor_id: UUID,
  name: String,                       // e.g., "Construction Class Modifier"
  dimension: String,                  // What asset attribute this applies to
  values: Map<String, f64>,           // Attribute value -> multiplier
  effective_date: Date,
  source: enum { StaticConfig, ActuarialModel, Historical }
}

WhatIfScenario {
  scenario_id: UUID,
  base_state: TemporalQuery,          // Starting point asset state
  modifications: [AssetModification],  // Proposed changes
  premium_impact: PremiumImpactResult?
}

PremiumImpactResult {
  scenario_id: UUID,
  current_premium: Money,
  projected_premium: Money,
  delta: Money,
  delta_percentage: f64,
  contributing_factors: [FactorContribution]
}
```

**Phase 1 accommodation:** `PremiumFactorSource` is defined as a versioned adapter. In Phase 1, it returns static configuration (manually entered premium factor tables). The adapter interface exists; only the implementation is trivial. Premium impact values displayed in the member portal are manually entered by pool administrators, not computed.

---

#### 6. Pool Health Analytics (Target: unscheduled)

**What it does:** Aggregate exposure analysis across the pool: concentration risk by geography/construction type/occupancy, portfolio diversification metrics, loss ratio trending, and financial projections.

**Boundary contract for future integration:**

```
PoolHealthSnapshot {
  pool_id: PoolId,
  as_of_date: Date,
  total_insured_value: Money,
  asset_count: u64,
  member_count: u64,
  concentration_risks: [ConcentrationRisk],
  diversification_score: f64?,
  quality_distribution: {
    high: u64,                        // Assets above high-quality threshold
    medium: u64,
    low: u64
  }
}

ConcentrationRisk {
  dimension: String,                  // e.g., "geography:zip_code", "construction_class"
  top_concentrations: [{
    value: String,                    // e.g., "90210", "Frame"
    tiv_amount: Money,
    tiv_percentage: f64,
    asset_count: u64
  }],
  herfindahl_index: f64?             // Concentration metric
}
```

**Phase 1 accommodation:** The exposure core already stores all data needed for pool health analytics. TIV accumulation analysis is available through member-facing exposure self-service views. Pool Health Analytics adds pool-wide aggregation and concentration modeling on top of data that already exists. No schema changes are needed.

---

#### 7. Full Communication Adapter (Target: unscheduled)

**What it does:** Structured campaigns (targeted messaging to member segments), task workflows (assign follow-up tasks to members with deadlines and tracking), and survey distribution (collect structured feedback from members).

**Boundary contract for future integration:**

```
Campaign {
  campaign_id: UUID,
  pool_id: PoolId,
  name: String,
  target_audience: CedarPolicy,      // Reuses ABAC for audience selection
  message_template: MessageTemplate,
  delivery_channels: [enum { InApp, Email, Both }],
  schedule: CampaignSchedule,
  status: enum { Draft, Scheduled, Active, Completed, Cancelled }
}

TaskAssignment {
  task_id: UUID,
  assigned_to: UserId,
  assigned_by: UserId,
  title: String,
  description: String,
  related_entity: {                   // What this task is about
    entity_type: enum { Asset, Member, Recommendation, QualityGap },
    entity_id: UUID
  },
  due_date: Date?,
  status: enum { Open, InProgress, Completed, Overdue, Cancelled },
  completed_at: Timestamp?
}
```

**Phase 1 accommodation:** The notification system handles lightweight one-way communication. Campaigns, tasks, and surveys build on the same `Notification` delivery infrastructure but add structured workflows and audience targeting. The Cedar policy engine already supports the audience selection mechanism campaigns would use.

---

#### 8. Member Team and Delegation Model (Target: unscheduled)

**What it does:** Multiple users within a single member organization, with delegation of responsibilities (e.g., a facilities director delegates building-level data management to campus managers).

**Boundary contract for future integration:**

```
MemberTeam {
  member_id: MemberId,
  team_members: [TeamMember]
}

TeamMember {
  user_id: UserId,
  role_within_member: String,         // e.g., "Facilities Director", "Campus Manager"
  delegated_scope: String,            // Materialized path prefix for delegation
  cedar_profile_ids: [ProfileId],     // ABAC profiles for this team member
  delegated_by: UserId?,
  delegated_at: Timestamp?
}
```

**Phase 1 accommodation:** The ABAC model with Cedar policies already supports arbitrarily granular scoping. A team member with delegated access to a campus is expressible as a Cedar policy scoped to that campus's materialized path prefix. The `AuthenticatedUser` contract includes `hierarchy_path` for scoped access. Adding team management is a UI and workflow concern; the authorization model accommodates it without schema changes. Phase 1 supports one user per member organization; adding more users later is an additive change.

---

### Technical Spikes (Before Committing to Full Implementation)

Three areas require dedicated investigation before the team commits to implementation approaches. Each spike produces a written finding with performance data and a go/no-go recommendation.

**Spike 1: Field-Level Temporal Resolution at 1M Asset Scale**
- **Question:** Can the field-level mutation store resolve "state of asset X as of date Y" within performance targets (50ms single, 500ms batch of 100) when the pool has 1M assets and tens of millions of mutations?
- **Method:** Generate synthetic mutation data at target scale. Benchmark resolution with multiple strategies (sequential scan, indexed lookups, materialized views, pre-computed snapshots). Test both record-level and field-level resolution strategies.
- **Output:** Performance data per strategy, recommended approach, and any architectural constraints discovered.
- **Risk if skipped:** The temporal model is foundational. If it cannot perform at scale, the entire asset state resolution layer must be redesigned.

**Spike 2: Search Index with Field-Level Cedar Access Control**
- **Question:** Can the search index evaluate Cedar field-level visibility policies within the 300ms target at 1M assets with complex policy sets?
- **Method:** Deploy search index (e.g., OpenSearch, Meilisearch, or PostgreSQL full-text) with 1M assets. Apply representative Cedar policies with field-level restrictions. Benchmark query latency with varying policy complexity.
- **Output:** Recommended search index technology, Cedar policy evaluation strategy (pre-filter vs. post-filter vs. index-level enforcement), and performance data.
- **Risk if skipped:** Field-level access control at the search layer is a novel requirement. Standard search indexes do not natively support per-field visibility. The integration approach must be validated before building the NL query layer on top.

**Spike 3: Step Function Bulk Import at Target Volume**
- **Question:** Can the seven-stage bulk import pipeline process 1M records within acceptable time bounds with correct resumability behavior?
- **Method:** Generate a 1M-record CSV with realistic data quality issues. Run through all seven stages. Simulate failures at each stage and verify resume-from-failure behavior. Measure throughput at various batch sizes.
- **Output:** Recommended batch size, expected total import time for 1M records, and any stage-specific bottlenecks.
- **Risk if skipped:** Pool onboarding with historical data is a Phase 1 requirement. If import performance is unacceptable, onboarding timelines are at risk.

### Usability Validation

The member portal UX is based on inferred behavior from observed usage of other tools, not validated with actual member facilities managers. Before finalizing the member portal experience:

- Conduct usability sessions with 3-5 facilities managers from pilot pool candidates.
- Focus on: renewal workflow (approve/modify/flag), exposure self-service navigation, data quality dashboard comprehension, and Certificate of Insurance generation.
- The adapter architecture means UX changes do not require domain logic changes. Findings from usability testing can be incorporated without rearchitecting.
- Usability validation should happen after the first working vertical slice of the member portal is available (Increment 6 in the delivery sequence below) and before investing in polish and edge cases.

### Phase 1 Incremental Delivery Sequence

Each increment delivers user-facing value end-to-end. Increments build on previous ones. Each is independently demonstrable and testable.

---

**Increment 0: Technical Spikes (1-2 weeks)**

Execute the three technical spikes in parallel. Results inform implementation decisions for all subsequent increments.

Deliverables:
- Spike 1 report: Temporal resolution strategy recommendation with benchmarks
- Spike 2 report: Search index technology and Cedar integration approach with benchmarks
- Spike 3 report: Bulk import batch size and throughput data

Gate: Spikes must produce acceptable results before proceeding. If any spike reveals a fundamental problem, the affected module's approach is redesigned before building on it.

---

**Increment 1: Authentication and Tenant Isolation (1-2 weeks)**

A CentuRisk admin and a pool admin can log in through their respective identity paths. Tenant isolation is enforced. This is the foundation everything else builds on.

Deliverables:
- Hosted directory authentication (pool admin login)
- Federated identity authentication (CentuRisk admin login)
- `AuthenticatedUser` contract flowing through to Cedar policy evaluation
- `TenantContext` enforced on all repository operations
- Cross-tenant data leakage prevention test (permanent CI fixture)

User-facing value: Two users can log into the system and see different, correctly scoped empty dashboards.

---

**Increment 2: Asset Registry Core with Single Asset CRUD (1-2 weeks)**

A CentuRisk admin creates a pool, creates a member within the pool, and adds a single asset through the UI. The asset has a stable ID, sits in a hierarchy, and stores field-level mutations with effective dates.

Deliverables:
- Pool and member creation (CentuRisk admin)
- Asset creation with system-generated ID
- Materialized path hierarchy (pool -> member -> asset)
- Field-level mutation store with effective dates
- Temporal state resolution (approved values at current date)
- Single asset view with all fields displayed

User-facing value: A CentuRisk admin can create a pool, add a member, add a building, and see the building's details.

---

**Increment 3: Data Quality Scoring on a Single Asset (1 week)**

The data quality model scores the asset created in Increment 2. The CentuRisk admin sees completeness, accuracy, and recency scores. Missing fields are highlighted.

Deliverables:
- Three-dimension quality scoring (completeness, accuracy, recency)
- Field-scoped recency tracking
- Quality score display on asset detail view
- QualityEvent emission when scores cross thresholds
- CentuRisk admin configures scoring rules for the pool

User-facing value: After adding a building with incomplete data, the admin sees a quality score and knows exactly which fields to fill in.

---

**Increment 4: SOV Pipeline and Approval Workflow (1-2 weeks)**

An edit to an asset flows through the SOV pipeline, produces a diff, and enters the approval workflow. A pool admin reviews and approves or rejects the change.

Deliverables:
- `SOVProcessingResult` production from inline edit
- Diff summary showing field-level changes
- Quality assessment of proposed changes
- Approval routing (auto-approve for authorized users, manual approval otherwise)
- Valuation changes always require approval
- Four view modes (approved/provisional x current/as-of-date)

User-facing value: A member edits a building's replacement cost. The pool admin sees the proposed change with a diff, reviews it, and approves. The building's value updates.

---

**Increment 5: ABAC Access Control with Cedar (1-2 weeks)**

Field-level access control is enforced. A member user logs in and sees only their own assets. A view-only user sees a restricted field set. The pool admin sees everything in their pool.

Deliverables:
- Cedar policy engine integrated with all data access paths
- Named profiles for common access patterns (pool admin, member user, view-only)
- Field-level visibility enforcement (hidden fields are not queryable or visible)
- CentuRisk admin creates and assigns profiles
- Verification that all four user categories see correctly scoped data

User-facing value: Three users log in and each sees a different, correctly scoped view of the same pool's data.

---

**Increment 6: Member Portal - Exposure Self-Service (1-2 weeks)**

A member logs in and sees their portfolio. They can view assets in a table, drill down to individual assets, and see TIV accumulation by configurable dimensions.

Deliverables:
- Member-facing portfolio view (tabular, filterable)
- Asset detail drill-down
- TIV accumulation analysis (by geography, construction type, custom dimensions)
- Map view with geospatial overlay (progressive enhancement: fallback to table)
- Data quality dashboard for member's assets

User-facing value: A member logs in and understands their exposure portfolio: what they own, what it is worth, and where their data quality gaps are.

**Usability validation checkpoint:** Conduct facilities manager usability sessions after this increment.

---

**Increment 7: Renewal Workflow (1-2 weeks)**

The renewal experience is functional. Pre-populated values appear. Members can approve, modify, or flag. Pool admins manage the flag queue. Bulk approval works for clean items.

Deliverables:
- Renewal pre-population with proposed values (manually entered by CentuRisk admin)
- Member approve/modify/flag workflow
- Flag for Discussion: queue item with member note, open/resolved state
- Bulk approval for items with no unresolved flags
- Renewal source tracked in `SOVProcessingResult`

User-facing value: A member receives their renewal SOV with proposed values, reviews each building, approves most in bulk, flags two for discussion, and modifies one. The pool admin sees the flags and resolves them.

---

**Increment 8: Recommendation Engine and Loss Event Intake (1 week)**

The rule-based recommendation engine produces suggestions. Members see prioritized recommendations. Loss events can be reported.

Deliverables:
- Rule-based recommendation engine with CentuRisk-authored rules
- `Recommendation` output rendered in member loss prevention views
- Loss event intake form (`LossEvent` contract)
- Loss events stored with full provenance (not yet feeding recommendations)

User-facing value: A member sees recommendations like "Building X lacks sprinkler documentation -- adding this could improve your data quality score" and can report a loss event.

---

**Increment 9: NL Querying (1-2 weeks)**

Users can ask questions about their portfolio in natural language. The search index respects Cedar field-level visibility.

Deliverables:
- Search index populated with asset data and custom fields
- NL-to-query-syntax translation layer
- Cedar field-level filtering enforced at search index
- Low-confidence handling with alternative suggestions
- Query telemetry (unresolved queries, suggestion acceptance rates)

User-facing value: A member types "show me all buildings over $5M replacement cost with no appraisal in the last 2 years" and gets results filtered to their authorized scope.

---

**Increment 10: Notifications and Email Digest (1 week)**

In-app notifications fire for quality events, approval requests, and renewal actions. Unacknowledged notifications are summarized in email digests.

Deliverables:
- In-app notification system with state tracking
- Notification triggers for quality events, approvals, renewals, flags
- Email digest scheduled job
- Pool-configurable digest frequency
- Notification acknowledgment tracking

User-facing value: A pool admin logs in and sees a notification badge showing 3 pending approvals and 2 quality alerts. A member who hasn't logged in for a week receives an email summarizing what's waiting.

---

**Increment 11: Bulk Import Pipeline (2-3 weeks)**

The full bulk import pipeline is functional. A pool can onboard with historical data from CSV/Excel.

Deliverables:
- Seven-stage step function pipeline (upload through summary)
- Each stage independently resumable
- Batch processing for large files
- Import summary with quality assessment for admin review
- Admin approval of import results
- Pipeline produces standard `SOVProcessingResult` records

User-facing value: A new pool uploads 10 years of historical asset data as a CSV. The import processes in stages, the admin reviews the summary (12,000 assets, 94% completeness, 47 errors to review), corrects issues, and approves the import.

---

**Increment 12: Input/Output Adapters and SOV Generation (1-2 weeks)**

Appraisal intake, SOV generation, and CAT export readiness are functional.

Deliverables:
- `AppraisalIntakeV1` adapter: structured appraisal results flow into asset valuations
- SOV generation: Excel and PDF with configurable columns
- Broker-specific template system for SOV formats
- Pre-flight validation for data completeness
- Coverage views with field-level differentials across policy periods
- Certificate of Insurance generation

User-facing value: A pool admin generates a submission-ready SOV in the broker's required format, validates completeness, and exports it. A member generates a Certificate of Insurance for a specific property.

---

**Increment 13: Custom Fields, Asset Types, and Pool Configuration (1 week)**

The full configurability layer is complete. CentuRisk admins define custom fields and configure pools. Pool admins modify custom fields.

Deliverables:
- Custom field definition (CentuRisk admin)
- Custom field modification (pool admin, with auditable history)
- Asset type composition model (type-specific extensions)
- Temporal resolution strategy configuration per pool
- Hierarchy depth and label configuration per pool

User-facing value: CentuRisk onboards a new pool type (e.g., a water district) that needs custom fields for pipe diameter, material, and installation date. The fields are added without code changes, quality scoring includes them, and they appear in the search index.

---

**Increment 14: Performance Validation and Hardening (1-2 weeks)**

The system is validated at target scale. Performance benchmarks pass. Edge cases and error handling are hardened.

Deliverables:
- Synthetic data generation at 1M assets, 2,500 members
- All performance benchmarks from the infrastructure ADR pass
- Load testing for concurrent users (pool admin + members + CentuRisk admin)
- Error handling for all degradation scenarios (external service unavailability)
- Circuit breakers configured and tested
- Audit trail completeness verification

User-facing value: The system performs acceptably when a pool with 1M assets and 2,500 members uses it in production.

---

### Delivery Timeline Summary

| Increment | Description | Duration | Cumulative |
|-----------|-------------|----------|------------|
| 0 | Technical Spikes | 1-2 weeks | 1-2 weeks |
| 1 | Authentication and Tenant Isolation | 1-2 weeks | 2-4 weeks |
| 2 | Asset Registry Core | 1-2 weeks | 3-6 weeks |
| 3 | Data Quality Scoring | 1 week | 4-7 weeks |
| 4 | SOV Pipeline and Approval | 1-2 weeks | 5-9 weeks |
| 5 | ABAC Access Control | 1-2 weeks | 6-11 weeks |
| 6 | Member Portal - Exposure Self-Service | 1-2 weeks | 7-13 weeks |
| 7 | Renewal Workflow | 1-2 weeks | 8-15 weeks |
| 8 | Recommendations and Loss Events | 1 week | 9-16 weeks |
| 9 | NL Querying | 1-2 weeks | 10-18 weeks |
| 10 | Notifications and Email Digest | 1 week | 11-19 weeks |
| 11 | Bulk Import Pipeline | 2-3 weeks | 13-22 weeks |
| 12 | IO Adapters and SOV Generation | 1-2 weeks | 14-24 weeks |
| 13 | Custom Fields and Pool Configuration | 1 week | 15-25 weeks |
| 14 | Performance Validation | 1-2 weeks | 16-27 weeks |

**Total estimated range: 16-27 weeks** depending on team size and spike outcomes.

### Future Phase Summary

| Phase | Capabilities | Depends On |
|-------|-------------|------------|
| Phase 1 | Everything above | -- |
| Phase 3 | Valuation Estimator | Phase 1 asset model, valuation intake |
| Phase 4 | Policy and Coverage Management | Phase 1 asset model, Phase 3 valuations |
| Phase 5 | Full Claims Lifecycle | Phase 1 loss event intake, Phase 4 policies |
| Phase 5 | Scheduled and Formatted Reporting | Phase 1 NL querying, search index |
| Unscheduled | Premium "What-If" Modeling | Phase 4 policy management |
| Unscheduled | Pool Health Analytics | Phase 1 exposure core |
| Unscheduled | Full Communication Adapter | Phase 1 notification system |
| Unscheduled | Member Team and Delegation | Phase 1 ABAC model |

## Alternatives Considered

### Delivery Approach
- **Horizontal layer-by-layer delivery (all models, then all services, then all UI):** Rejected. This delays user-facing value to the end. The risk pool flywheel -- the system's core value proposition -- cannot be demonstrated until every layer is complete. Vertical slices deliver demonstrable value at every increment.
- **Module-by-module delivery (complete asset registry, then complete quality model, etc.):** Rejected. Modules are interdependent. The quality model needs assets. The SOV pipeline needs quality scores and approval routing. Member views need access control. Vertical slices cut through all modules to deliver one working feature at a time.
- **Start with member portal (outside-in):** Considered but adjusted. The member portal depends on authentication, tenant isolation, and asset data. Starting from the user-facing layer is ideal but the infrastructure must exist first. Increments 1-5 build the minimum infrastructure needed to show the first member-facing view in Increment 6.

### Phase 1 Scope
- **Smaller Phase 1 (exclude NL querying and recommendations):** Rejected. NL querying and recommendations are core to the value proposition of making the risk pool flywheel visible. Without them, the system is a data entry tool, not a decision-support platform.
- **Larger Phase 1 (include valuation estimator):** Deferred. The manual CoreLogic entry process is sufficient for Phase 1. The valuation estimator adds value but requires actuarial modeling expertise that may not be available. It can be pulled into Phase 1 if capacity allows without blocking the release.

### Technical Spike Timing
- **Spikes during implementation:** Rejected. Discovering that field-level temporal resolution does not perform at scale after building 6 increments on top of it would require rework. Spikes must happen before committing to the approach.
- **No spikes (trust the design):** Rejected. The three spike areas are novel technical challenges without industry-standard solutions. Performance characteristics at target scale are unpredictable without measurement.

## Consequences

**Positive outcomes:**
- Every increment produces demonstrable value. Stakeholders can provide feedback on working software throughout development.
- Technical risks are identified and mitigated in Increment 0, before significant implementation investment.
- Usability validation happens at Increment 6, when there is a working member portal to test with, but before investing in polish and edge cases.
- All deferred items have boundary contracts defined in Phase 1. No Phase 1 architectural decision precludes future integration.
- The incremental sequence allows scope adjustment: if time is constrained, lower-priority increments (13, parts of 12) can be deferred without affecting the core experience.
- Each increment is a potential release point. If business needs require an early launch, the system is usable (though incomplete) after Increment 7.

**Negative outcomes and trade-offs:**
- Vertical slices mean some modules are partially built across multiple increments. The asset registry, for example, gets CRUD in Increment 2, quality scoring in Increment 3, approval routing in Increment 4, and custom fields in Increment 13. This requires careful interface design to avoid rework.
- Technical spikes add 1-2 weeks before any feature delivery. This is an investment in risk reduction, not immediate value.
- The 16-27 week range is wide. Team size, spike outcomes, and usability validation findings all affect the actual timeline.
- Deferred items with "unscheduled" target phases have no committed delivery date. Stakeholders expecting Pool Health Analytics or Premium What-If Modeling must understand these are not in any current phase.

**New constraints introduced:**
- Each increment must be independently deployable and testable. This requires CI/CD infrastructure from Increment 1.
- The boundary contracts for deferred items are commitments. Changing the `LossEvent` contract in Phase 1 would affect the Phase 5 claims lifecycle integration. Contract versioning must be taken seriously.
- Usability validation at Increment 6 may produce findings that require rework of Increments 6-7. The schedule must accommodate iteration.
- Performance validation in Increment 14 is a hard gate. If benchmarks fail, the team must resolve performance issues before Phase 1 can ship.
