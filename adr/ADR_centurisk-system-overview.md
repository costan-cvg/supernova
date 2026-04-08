# ADR: CentuRisk System Overview

## Status
Proposed

## Context

CentuRisk is a Risk Management Information System (RMIS) for public sector risk pools. Risk pools are cooperatives of public entities (cities, counties, school districts) that share risk. The system must manage exposure data, enforce data quality, support member transitions between pools, and present decision-support information to pool administrators and members.

Several foundational questions drive the architecture:

1. **What is the system's role?** Members log in for policy changes, claims, value trending, and premium impact forecasts. The system must present accurate state on demand, but the pool administrator -- not the system -- drives engagement and action.
2. **Who owns the data?** Members own their data. Pools receive time-scoped access grants. Members may migrate between pools, and the system must not create inadvertent stickiness by coupling data to a pool.
3. **How is trust maintained?** Every mutation is logged with full provenance in an immutable audit trail. Pools receiving a migrating member can verify data integrity across the entire history.
4. **Has the member experience been validated?** No. The member UX is inferred from observed behavior with other tools, not validated with facilities managers. This is a known risk mitigated by the adapter architecture: member-facing views iterate independently of domain logic.

The architecture must support these constraints while remaining testable, extensible, and portable.

## Decision

### System Role: Decision-Support, Not Engagement Engine

The system is a decision-support tool. It generates information used to make decisions and inform actions. It does not push members to act.

- **Phase 1 scope**: Present current, accurate state whenever a member logs in. No proactive engagement features (email campaigns, push notifications).
- **Premium impact forecasts**: Externally provided by the pool administrator, not computed by the system. Premium impact is an input adapter -- the administrator enters values, the system validates and persists them, the member portal renders them. No actuarial engine is needed until What-If modeling is built in a later phase.
- **Member portal purpose**: Policy changes, claims, portfolio value trending, premium impact display.

### The Risk Pool Flywheel

The system makes the following virtuous cycle visible and actionable:

```
Better Data --> Better Prevention --> Reduced Losses --> Stable Rates --> Diverse Membership --> More Data
                                                                                    |
                                                                                    v
                                                                           (cycle repeats)
```

Every module in the system serves this flywheel. The asset registry captures the data. The quality model measures it. Recommendations improve it. SOV exports operationalize it. The member portal makes it legible to every participant.

### Data Ownership: Member Owns Data, Not the Pool

Member data exists independently of any pool relationship. Pools are granted visibility through explicit, time-scoped, revocable access grants.

**Key constraints:**

- The permission model is relationship-based access, not simple pool-scoped hierarchy.
- A member's data is never structurally coupled to a single pool, even if day-to-day operation assumes one pool at a time.
- Data portability is a design principle, not a frequently exercised workflow. No polished handoff UI is needed in Phase 1, but an administrator can reassign the access grant without migrating data between storage boundaries.

**Access grant contract:**

```
AccessGrant {
    grant_id:     UniqueID
    member_id:    UniqueID
    pool_id:      UniqueID
    granted_by:   ActorID
    granted_at:   Timestamp
    effective_from: Date
    effective_to:   Date | null   // null = open-ended
    revoked_at:     Timestamp | null
    revoke_reason:  String | null
}
```

When a member migrates, the old pool's grant receives an `effective_to` and/or `revoked_at`, and the new pool receives a new grant. Data does not move -- visibility changes.

### Immutable Audit Trail

Every mutation is logged with full provenance:

```
AuditEntry {
    entry_id:       UniqueID
    entity_id:      UniqueID
    entity_type:    EntityType
    field_name:     String | null    // null for entity-level events
    old_value:      Value | null
    new_value:      Value | null
    effective_date: Date
    actor_id:       ActorID
    actor_role:     Role
    pool_id:        UniqueID         // which pool context the change was made in
    timestamp:      Timestamp
    operation:      Create | Update | Archive | Restore
}
```

The audit trail enables:
- Pool trust verification when receiving a migrating member.
- Regulatory compliance for public sector entities.
- Debugging and dispute resolution.
- Full provenance chain for every value in the system.

### Validation Status Risk

The member experience is designed from inferred behavior, not direct validation with facilities managers. This is an acknowledged risk.

**Mitigation:** The adapter architecture ensures member-facing views can iterate independently of domain logic. The core exposure model is stable; the user experience adapts. Usability testing with a pilot pool should occur before building the full member experience.

### Three User Tiers

The access model defines three tiers:

| Tier | Scope | Examples |
|------|-------|---------|
| **Member User** | Own data within granted pool context | View portfolio, submit changes, see quality scores |
| **Pool Administrator** | All members within their pool | Configure thresholds, approve changes, run exports |
| **CentuRisk System Admin** | Cross-pool visibility | Configure scoring rules, define custom fields, view audit trails across pools |

### Design Principles

These principles govern every architectural decision in the system.

**1. Pure Core, Impure Edges.**
The exposure core is a pure, deterministic computation engine. No side effects, no external dependencies, no state outside its input/output contracts. The edges -- adapters, I/O, authorization, notifications -- are impure. They handle format translation, external communication, and statefulness. This boundary makes the core testable, portable, and resilient.

**2. Boundary Contracts.**
Every boundary between system components is defined by a versioned contract: what goes in, what comes out, what is guaranteed. `SOVProcessingResult`, `Recommendation`, `QualityEvent` -- these are contracts. Contracts are versioned so components can evolve independently. A new appraisal format (`AppraisalIntakeV2`) fits without changing the core.

**3. Versioned Adapters.**
External formats change. The system adapts. Input adapters (`OnboardingV1`, `AppraisalIntakeV1`) and output adapters (`SOVGeneratorV1`) are versioned from day one. When a pool needs a new format, a new adapter version is added. The core is never touched.

**4. Composable and Extensible.**
Asset types are compositions, not hardcoded. Custom fields are extensible. Scoring rules are authored as data. Quality dimensions are composable. Recommendation categories are defined by CentuRisk admins. The system does not require code changes to support new asset types, new scoring dimensions, or new recommendation categories.

**5. Test by Layers.**
The pure core is tested exhaustively with unit and property-based tests. Adapters are tested in isolation against contracts. Integration tests verify adapters compose correctly with the core. No mocking of infinite external systems, no tangled state.

**6. Observe and Adjust.**
Quality events, NL query telemetry, and loss intake data are feedback loops. Unresolved NL queries inform improvements to the translation layer. Quality event patterns reveal data collection gaps. The system is designed to be measured and improved in production.

**7. Fail Predictably.**
When external services are unavailable, the core and critical adapters continue working. The system degrades gracefully. Maps and geospatial features become unavailable; everything else works. Errors are data -- visible and queryable, not hidden in logs.

**8. ABAC Authorization as Adapter Concern.**
The exposure core is authorization-agnostic. It returns data; adapters enforce access control. Cedar policies are evaluated at the adapter layer, not at the core. Field-level authorization is enforced at the search index. When a policy denies access to a field, that field does not exist in the user's view -- not queryable, not visible.

### Historical Data Migration

When a pool adopts the system, historical data (potentially tens of gigabytes, 60+ years) is imported during onboarding. The onboarding adapter has two modes sharing the same destination contract:

- **Interactive guided flow**: Individual member first-time submissions.
- **Batch processing pipeline**: Historical bulk import with progress tracking, partial failure handling, resumability, and asynchronous quality scoring.

The architecture must not preclude adding a persistent sync adapter later.

## Alternatives Considered

### Engagement-Driven System
An alternative would build proactive engagement features (email campaigns, push notifications, gamification) into the system. This was rejected because:
- The pool administrator is the human in the loop who drives action.
- Building engagement features adds complexity without clear Phase 1 value.
- The system's strength is accurate, current state presentation -- not behavior modification.

### Pool-Owned Data Model
A simpler model would scope all data to a single pool. This was rejected because:
- Members do migrate between pools, and evidence confirms this happens.
- Coupling data to a pool creates inadvertent stickiness that conflicts with member rights.
- The relationship-based model is only marginally more complex but enables portability.

### Computed Premium Impact
The system could compute premium impact forecasts using an actuarial engine. This was rejected for Phase 1 because:
- Pool administrators already produce these forecasts externally.
- Building an actuarial engine is substantial scope with uncertain accuracy requirements.
- Treating premium impact as an input adapter defers this complexity to a later phase.

## Consequences

**Positive outcomes:**
- The system delivers clear, measurable value from Phase 1: accurate exposure data presentation and quality scoring.
- Data portability is built in from day one, avoiding costly retrofits.
- The adapter architecture allows member UX iteration without touching domain logic, mitigating the validation risk.
- Pure core / impure edges makes the system highly testable and portable.
- The flywheel model gives every feature a clear purpose and measurement criterion.

**Negative outcomes and trade-offs:**
- Relationship-based access grants add complexity to every query (access must be checked against active grants, not just pool membership).
- The immutable audit trail will grow continuously and requires a retention/archival strategy.
- Not validating the member experience before building introduces rework risk.
- Decision-support-only scope means the system may feel passive to members who expect proactive guidance.

**New constraints introduced:**
- Every data access path must check the active access grant, not assume pool-scoped data.
- All mutations must produce audit entries -- no write path may bypass the audit trail.
- Member-facing views must be implemented as adapters, not as core logic, to preserve iteration freedom.
- The three-tier access model (member, pool admin, CentuRisk admin) must be reflected in Cedar policies from the start.

## Implementation Plan

1. **Access grant model and audit trail storage** -- Define the `AccessGrant` and `AuditEntry` schemas, implement write-path audit logging, and verify with unit tests that every mutation produces an audit entry. Delivers: the trust foundation that every other module depends on.

2. **Member data isolation with relationship-based access** -- Implement access grant evaluation so that data queries are filtered by active grants. Verify with integration tests that a member's data is invisible to a pool whose grant has expired. Delivers: data portability proof.

3. **Pool administrator decision-support views** -- Build the adapter layer that presents current approved state for a pool's granted members. Mock the exposure core with static data. Delivers: the first runnable view a pool admin can see.

4. **Premium impact as input adapter** -- Implement the input adapter for pool-administrator-provided premium impact values. Validate, persist, and render them in the member portal. Delivers: end-to-end premium impact display.

5. **Historical data bulk import pipeline** -- Build the batch processing adapter with progress tracking and partial failure handling. Feed the same core contract as the interactive flow. Delivers: onboarding capability for real pools.

6. **Member portal with quality dashboard** -- Wire the member-facing adapter to the quality model output. Present completeness, accuracy, and recency scores. Delivers: the first member-facing value.
