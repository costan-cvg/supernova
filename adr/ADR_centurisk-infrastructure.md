# ADR: CentuRisk Infrastructure Decisions

## Status
Proposed

## Context

CentuRisk RMIS is a platform for public sector risk pools to manage exposure data, data quality, and member engagement. Six infrastructure decisions were absent from the original value proposition but are necessary to build the system. These decisions cut across every module and must be resolved before development begins because they constrain implementation choices at every layer.

The system must support:
- Multiple user populations with different identity sources (CentuRisk staff, pool administrators, members)
- Strict data isolation between pools and members, with data portability when members change pools
- Notification delivery that respects the decision-support (not engagement-engine) system role
- Graceful degradation when external services are unavailable
- Scale targets of 1M assets and 2,500 members per pool, with tens of millions of mutation records
- Bulk historical data import during pool onboarding, potentially spanning 60+ years of records

These decisions interact with every boundary contract defined in the system and must compose cleanly with the ABAC/Cedar policy engine, the field-level temporal model, and the adapter architecture.

## Decision

### 1. Authentication: Two-Path Model with Identity Adapter

**Approach:** Authentication uses two paths unified behind a single identity adapter contract. The core receives an `AuthenticatedUser` regardless of identity source.

**CentuRisk Internal Users** authenticate via federated identity from day one. CentuRisk operates its own identity provider; the system federates against it.

**Pool Administrators and Members** authenticate via a hosted directory implemented using Okta, Auth0, or AWS Cognito. Federation for individual pools is added on demand when a pool requires it (e.g., a large municipality with its own IdP).

**Identity Adapter Contract:**

```
AuthenticatedUser {
  user_id: UUID,                    // System-generated, stable
  identity_source: enum {           // Which IdP authenticated this user
    CenturiskFederated,
    HostedDirectory,
    PoolFederated { pool_id }
  },
  user_category: enum {             // Maps to ABAC user model
    CenturiskUser,
    PoolAdministrator,
    MemberUser,
    ViewOnlyUser
  },
  pool_scope: PoolId?,              // null for cross-pool CentuRisk users
  member_scope: MemberId?,          // null for pool-level and CentuRisk users
  hierarchy_path: String?,          // Materialized path for scoped access
  cedar_profile_ids: [ProfileId],   // Named profiles assigned to this user
  session_metadata: {
    authenticated_at: Timestamp,
    ip_address: String,
    user_agent: String
  }
}
```

**Key properties:**
- The exposure core never sees authentication details. It receives `AuthenticatedUser` and operates on it.
- Cedar policy evaluation uses `AuthenticatedUser` attributes to make access decisions.
- Adding a new identity source (pool federation, future broker IdP) means adding an adapter, not changing the core.
- Session metadata supports audit trail requirements without coupling authentication to audit logging.

### 2. Multi-Tenancy: Logical Isolation with Changeable Implementation

**Approach:** The domain core is agnostic to the isolation mechanism. Row-level security, separate schemas, and separate databases are all valid implementations and can be swapped without changing domain logic.

**Isolation Boundary Contract:**

```
TenantContext {
  pool_id: PoolId,
  member_id: MemberId?,              // null for pool-wide operations
  isolation_scope: enum {
    PoolWide,                         // Pool admin viewing all members
    MemberScoped { member_id },       // Member viewing own data
    CrossPool { pool_ids: [PoolId] }  // CentuRisk admin cross-pool view
  }
}
```

**Encryption and Key Management:**

```
EncryptionKeyGrant {
  key_id: UUID,
  member_id: MemberId,               // Keys are member-scoped
  granted_to_pool_id: PoolId,        // Current pool with access
  granted_at: Timestamp,
  revoked_at: Timestamp?,            // null = active grant
  grant_reason: enum {
    MemberOnboarding,
    PoolTransition,
    AdministrativeOverride
  }
}
```

**Key management rules:**
- Encryption keys are generated per member, not per pool.
- The current pool receives a key access grant tied to the pool relationship.
- When a member moves pools: (a) the new pool receives a key grant, (b) the old pool's grant is revoked with a `revoked_at` timestamp, (c) the old pool retains read access to historical data produced during its tenure via a time-bounded grant if required by regulation.
- Key rotation follows member lifecycle events, not calendar schedules.
- Data portability is an operational consequence: because keys follow the member, moving a member between pools is a grant operation, not a data migration.

**Implementation strategy flexibility:**
- Phase 1 may start with row-level security for simplicity.
- The architecture must allow migration to separate schemas or databases without changing domain logic.
- The `TenantContext` is passed into every repository/store operation. The store implementation decides how to enforce isolation.
- All queries include tenant filtering; no query path bypasses isolation except explicit `CrossPool` scope held only by CentuRisk users.

### 3. Notifications: In-App Primary, Email Digest Secondary

**Approach:** In-app notifications are the primary channel. Email is a digest of unacknowledged in-app notifications, not an independent notification path.

**Notification Contract:**

```
Notification {
  notification_id: UUID,
  recipient_user_id: UUID,
  pool_id: PoolId,
  source: enum {
    QualityEvent { quality_event_id },
    Recommendation { recommendation_id },
    ApprovalRequest { approval_id },
    RenewalAction { renewal_id },
    FlagUpdate { flag_id },
    SystemAnnouncement { announcement_id },
    ImportComplete { import_job_id }
  },
  priority: enum { Low, Normal, High, Urgent },
  title: String,
  body: String,
  action_url: String?,               // Deep link to relevant view
  state: NotificationState,
  created_at: Timestamp,
  delivered_at: Timestamp?,           // When rendered in-app
  acknowledged_at: Timestamp?         // When user explicitly dismissed/acted
}

NotificationState: enum {
  Created,      // Generated but not yet delivered to user's session
  Delivered,    // Rendered in the user's in-app notification feed
  Acknowledged  // User dismissed or took action
}
```

**Email Digest:**

```
DigestConfiguration {
  pool_id: PoolId,
  frequency: enum { Daily, TwiceWeekly, Weekly, BiWeekly },
  delivery_time: TimeOfDay,           // When the digest job runs
  delivery_timezone: Timezone,
  enabled: bool                       // Pool admin can disable
}
```

**Digest behavior:**
- A scheduled job queries for `Created` or `Delivered` (but not `Acknowledged`) notifications per user, grouped by pool.
- The digest email summarizes unacknowledged notifications with deep links back to the in-app views.
- Digest frequency is a pool administrator setting, configurable per pool.
- Individual users cannot override pool-level digest frequency in Phase 1.
- The digest job is idempotent: running it twice for the same period produces the same email content.

**Notification sources and their triggers:**
| Source | Trigger Condition |
|--------|------------------|
| QualityEvent | Quality score crosses a pool-configured threshold |
| Recommendation | New recommendation generated for member's assets |
| ApprovalRequest | Change submitted requiring approval; approval granted/denied |
| RenewalAction | Renewal period opens; pre-populated values available for review |
| FlagUpdate | Flag resolved by pool admin; flag created by member |
| SystemAnnouncement | CentuRisk or pool admin broadcasts to users |
| ImportComplete | Bulk import job reaches terminal state (success or error) |

### 4. Progressive Enhancement: Core Without External Dependencies

**Approach:** The core experience is fully functional without external services. External services enhance the experience when available but are never in the critical path.

**Enhancement Tiers:**

| Tier | Capability | External Dependency | Fallback |
|------|-----------|---------------------|----------|
| **Core (always works)** | Asset data views, tables, filtering | None | N/A |
| **Core** | Quality scores, breakdowns | None | N/A |
| **Core** | Recommendations, prioritized list | None | N/A |
| **Core** | SOV generation, export | None | N/A |
| **Core** | NL querying (search index) | None | N/A |
| **Core** | Approval workflows, flags | None | N/A |
| **Core** | Coverage differentials | None | N/A |
| **Enhanced** | Map views, geospatial overlays | Map tile provider | Tabular view with address/coordinates |
| **Enhanced** | Hazard layer overlays | Hazard data provider | Data available without visualization |
| **Enhanced** | Address geocoding | Geocoding service | Manual lat/lng entry, address as text |
| **Enhanced** | Email digest delivery | Email service (SES/SendGrid) | In-app notifications still functional |

**Implementation rules:**
- Every feature that depends on an external service must define its fallback behavior explicitly.
- The UI loads core content first, then progressively enhances with external data.
- External service calls are behind circuit breakers with configurable timeouts.
- External service availability is observable: the system logs degradation events following the same structured event pattern as quality events.
- Client bandwidth detection informs which enhancement tier to attempt (progressive enhancement in the original meaning: works on low-bandwidth connections, richer on high-bandwidth).

**External Service Adapter Contract:**

```
ExternalServiceResult<T> {
  status: enum {
    Available { data: T, fetched_at: Timestamp },
    Degraded { partial_data: T?, error: String },
    Unavailable { fallback_used: bool, error: String }
  },
  service_id: String,
  latency_ms: u64
}
```

### 5. Performance Targets: Phase 1 Scale Requirements

**Target scale per pool:**
- 1,000,000 assets
- 2,500 members
- Tens of millions of field-level mutation records (estimated: 1M assets x 30 tracked fields x average 3 mutations = 90M mutation records upper bound)

**Performance-critical operations and benchmarks:**

| Operation | Target | Notes |
|-----------|--------|-------|
| Materialized path prefix query (descendants of a node) | < 200ms at 1M assets | GROUP BY on prefix substring |
| Temporal state resolution (as-of-date for single asset) | < 50ms | Field-level mutation composition |
| Temporal state resolution (as-of-date for asset list, 100 assets) | < 500ms | Batch resolution |
| Quality score computation (single asset) | < 100ms | Three-dimension evaluation |
| Quality score computation (batch re-score, 10K assets) | < 30s | Background job, not interactive |
| Search index query (NL translated) | < 300ms | Including Cedar field-level filtering |
| SOV export generation (full pool, 1M assets) | < 5 min | Background job with progress tracking |
| Coverage differential view (1K assets, 2 periods) | < 2s | Field-level comparison |
| Bulk import stage (parse 100K records) | < 60s per batch | Step function stage |

**Performance testing requirements:**
- Performance testing at target scale is a Phase 1 requirement, not a post-launch activity.
- Synthetic data generation must produce realistic mutation distributions (not uniform random).
- The field-level temporal model at scale is the highest-risk performance concern and requires a dedicated technical spike.
- Search index performance with Cedar field-level filtering at scale is the second-highest risk.
- Performance benchmarks are automated and run in CI against a representative dataset.

### 6. Bulk Import Pipeline: Step Function Flow

**Approach:** Greenfield step function (state machine) accepting CSV or Excel input, with each stage independently resumable.

**Pipeline Stages:**

```
BulkImportJob {
  job_id: UUID,
  pool_id: PoolId,
  initiated_by: UserId,
  source_file: {
    filename: String,
    format: enum { CSV, Excel },
    size_bytes: u64,
    uploaded_at: Timestamp,
    checksum: String                  // SHA-256 for integrity verification
  },
  current_stage: ImportStage,
  stage_history: [StageExecution],
  created_at: Timestamp,
  completed_at: Timestamp?
}

ImportStage: enum {
  Upload,                             // File received and stored
  Parse,                              // File parsed into raw records
  ValidateSchema,                     // Records validated against SOV schema
  MapToExposureModel,                 // Raw fields mapped to asset model
  AssignAssetIds,                     // System IDs assigned or matched to existing
  RunQualityScoring,                  // Quality model evaluates imported data
  ProduceImportSummary                // Summary generated for human review
}

StageExecution {
  stage: ImportStage,
  status: enum { Pending, InProgress, Completed, Failed, Skipped },
  started_at: Timestamp?,
  completed_at: Timestamp?,
  records_processed: u64,
  records_succeeded: u64,
  records_failed: u64,
  errors: [ImportError],
  batch_progress: {                   // For large imports processed in chunks
    total_batches: u64,
    completed_batches: u64,
    current_batch_id: String?
  }
}

ImportError {
  row_number: u64?,
  field_name: String?,
  error_type: enum {
    SchemaViolation,
    TypeMismatch,
    RequiredFieldMissing,
    ReferentialIntegrityViolation,
    DuplicateDetected,
    ValueOutOfRange
  },
  message: String,
  severity: enum { Error, Warning }   // Warnings don't block progression
}
```

**Import Summary for Review:**

```
ImportSummary {
  job_id: UUID,
  total_records: u64,
  new_assets: u64,
  updated_assets: u64,               // Matched to existing by ID or rule
  skipped_records: u64,              // Duplicates or irrecoverable errors
  quality_summary: {
    average_completeness: f64,
    average_accuracy: f64,
    assets_below_threshold: u64
  },
  errors_by_stage: Map<ImportStage, u64>,
  warnings: [ImportError],
  requires_review: bool,              // true if any errors or warnings exist
  approved_by: UserId?,               // Set when admin approves import
  approved_at: Timestamp?
}
```

**Resumability rules:**
- Each stage persists its output before marking as `Completed`.
- A failed stage can be retried from its last completed batch without re-running prior stages.
- Stage outputs are immutable once completed; re-running a completed stage requires explicit reset.
- The administrator can review errors at any failed stage, correct the source data, and resume.
- At 1M records, stages process in configurable batch sizes (default: 10,000 records per batch).

**Pipeline destination contract:** The bulk import pipeline feeds the same `SOVProcessingResult` contract that all other input adapters use. The exposure core does not know whether data arrived via interactive onboarding, inline edit, renewal, or bulk import. The `source` discriminator on `SOVProcessingResult` records `BulkImport { job_id }` for auditability.

## Alternatives Considered

### Authentication
- **Single hosted directory for all users:** Rejected because CentuRisk staff already have a corporate IdP; forcing them through a hosted directory adds friction and removes SSO benefits. The two-path model is more complex to implement but correctly reflects organizational reality.
- **Full federation from day one for pools:** Rejected as premature. Most pools will not have an IdP ready to federate. The hosted directory is the pragmatic starting point; federation is added per demand.
- **Build custom auth:** Rejected. Identity is a solved problem. Using Okta/Auth0/Cognito avoids reinventing password management, MFA, session handling, and token lifecycle.

### Multi-Tenancy
- **Separate database per pool:** Provides the strongest isolation but complicates cross-pool operations for CentuRisk admins and increases operational overhead. Not rejected permanently, but not the starting point.
- **Single shared schema with no isolation mechanism:** Too risky. A single misconfigured query could expose data across pools. Logical isolation with explicit tenant context is the minimum acceptable baseline.
- **Pool-scoped encryption keys:** Rejected because it couples data to pools, violating the data portability principle. Member-scoped keys with pool grants preserve portability.

### Notifications
- **Email as primary channel:** Rejected. The system is decision-support, not an engagement engine. Members log in when they have business to conduct; notifications should be waiting for them, not pushing them to act.
- **Real-time push notifications (WebSocket/SSE):** Deferred. In-app notifications rendered on page load are sufficient for Phase 1. Real-time push adds infrastructure complexity without clear value for a decision-support tool.
- **Per-user digest configuration:** Deferred. Pool-level configuration is sufficient for Phase 1 and simpler to implement and administer.

### Performance
- **Lower scale targets with "scale later" approach:** Rejected. The field-level temporal model and search index with Cedar filtering are architecturally constrained by scale. Discovering performance problems at 1M assets after building for 10K assets would require significant rework. Performance testing at target scale must happen during development.

### Bulk Import
- **Real-time streaming import:** Rejected for Phase 1. The one-time historical import use case does not require streaming. Step function flow with batch processing is simpler, more debuggable, and sufficient for the use case.
- **External ETL tool (Airflow, dbt):** Rejected. The import pipeline needs deep integration with the exposure model, quality scoring, and asset ID assignment. An external ETL tool would require maintaining the exposure model logic in two places.

## Consequences

**Positive outcomes:**
- The identity adapter contract decouples authentication from domain logic permanently. Adding new identity sources (broker federation, API keys for integrations) requires only a new adapter.
- Member-scoped encryption with pool grants implements the data portability principle at the infrastructure level. Pool transitions become grant operations, not data migrations.
- In-app-primary notifications align with the decision-support system role. No infrastructure investment in push delivery that would go unused.
- Progressive enhancement means the system is deployable in environments with limited external service access. Core functionality never degrades.
- Performance testing at scale during development prevents late-stage architectural discoveries.
- The step function bulk import is independently testable per stage. Each stage can be developed, tested, and performance-benchmarked in isolation.

**Negative outcomes and trade-offs:**
- Two authentication paths require maintaining two IdP integrations from day one (federated + hosted directory).
- Logical isolation with changeable strategy means the initial implementation choice (likely row-level security) may need to be replaced under load. The architecture accommodates this, but the migration itself is operational work.
- In-app-primary notification means users who rarely log in may miss time-sensitive items until the email digest fires. Digest frequency configuration mitigates but does not eliminate this.
- Performance testing at target scale requires significant synthetic data infrastructure that must be built and maintained alongside the application.
- The bulk import pipeline is a substantial engineering effort (seven stages, resumability, batch processing) that serves primarily the onboarding use case.

**New constraints introduced:**
- Every repository/store operation must accept `TenantContext`. No query path may bypass tenant isolation.
- Every external service integration must implement the `ExternalServiceResult<T>` contract with fallback behavior.
- The email service is itself an external dependency subject to the progressive enhancement model: if email delivery fails, in-app notifications still function.
- Bulk import at target scale (1M records) must be performance-tested during development, requiring synthetic data generation tooling.
- The Cedar policy engine must be evaluable at the performance targets specified: field-level filtering on search queries at 1M assets must complete within 300ms.

## Implementation Plan

1. **Identity adapter with hosted directory** -- Implement the `AuthenticatedUser` contract and the hosted directory adapter (Okta, Auth0, or Cognito). A single CentuRisk admin and a single pool admin can authenticate. The exposure core receives `AuthenticatedUser` and makes access decisions. Validates the adapter boundary works end-to-end.

2. **CentuRisk federated identity path** -- Add the federated identity adapter for CentuRisk staff. Both authentication paths produce the same `AuthenticatedUser` contract. Validates that the two-path model works correctly and that Cedar policy evaluation is identity-source-agnostic.

3. **Tenant isolation layer** -- Implement `TenantContext` injection into repository operations. Start with row-level security. Write a verification test that proves cross-tenant data leakage is impossible for every query path. This test is permanent and runs in CI.

4. **Member-scoped encryption and key grants** -- Implement `EncryptionKeyGrant` lifecycle: key generation on member creation, grant to pool on onboarding, revocation on pool transition, grant to new pool. Test the pool transition flow end-to-end with data access verification before and after transition.

5. **In-app notification system** -- Implement the `Notification` contract with state tracking (Created/Delivered/Acknowledged). Wire the first notification source: `QualityEvent` threshold crossing. A pool admin sees notifications in-app when quality scores cross configured thresholds.

6. **Email digest job** -- Implement `DigestConfiguration` and the scheduled digest job. The job queries unacknowledged notifications, generates a summary email, and delivers via email service adapter. Verify idempotency: running the job twice for the same period produces identical output.

7. **Progressive enhancement framework** -- Implement the `ExternalServiceResult<T>` contract and circuit breaker pattern. Wire the first enhanced feature: map view with fallback to tabular view. External service unavailability triggers a `Degraded` or `Unavailable` result; the UI renders the fallback.

8. **Synthetic data generation for performance testing** -- Build tooling to generate realistic data at target scale: 1M assets, 2,500 members, tens of millions of mutations with realistic distribution. This is infrastructure for all subsequent performance work.

9. **Performance benchmarks at scale** -- Benchmark the critical operations listed in the performance targets table against the synthetic dataset. Identify operations that fail to meet targets. This spike directly informs architecture decisions for temporal resolution and search indexing.

10. **Bulk import pipeline: Upload through ValidateSchema** -- Implement the first three stages of the step function: file upload, parse (CSV/Excel), and schema validation. Each stage persists output and is independently resumable. Test with a 100K-record CSV.

11. **Bulk import pipeline: MapToExposureModel through ProduceImportSummary** -- Implement the remaining stages: field mapping, asset ID assignment/matching, quality scoring, and summary generation. The pipeline feeds the standard `SOVProcessingResult` contract. Test end-to-end with a 100K-record import and verify the summary is accurate.

12. **Bulk import at target scale** -- Run the full pipeline against a 1M-record dataset. Verify batch processing, resumability after simulated failures, and performance within the specified targets. This is the performance validation gate for the import pipeline.
