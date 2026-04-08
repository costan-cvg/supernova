# ADR: CentuRisk Data Quality Model

## Status
Proposed

## Context

The data quality model is the mechanism by which CentuRisk makes the risk pool flywheel measurable. Without quality scoring, neither members nor pool administrators can see whether exposure data is improving, stagnating, or degrading. The quality model must answer: "How good is this asset's data, and where should effort be focused?"

Several constraints shape the design:

1. **Data source**: In Phase 1, the quality model uses only data already stored in the asset registry. No external reference data is consulted. This means accuracy scoring must work from cross-referencing attributes within each asset record.
2. **Rule authorship**: CentuRisk admins author scoring rules and customize them per pool. Pool administrators do not see or modify the scoring logic. This follows the same pattern as temporal resolution strategies: CentuRisk configures, the core evaluates, users see results.
3. **Threshold ownership**: While CentuRisk controls how quality is measured, pool administrators control what score levels matter to them. One pool may alert at 80% completeness; another at 60%.
4. **Recency granularity**: Recency must be field-scoped, not asset-scoped. Updating a contact name must not reset the recency clock on replacement cost. This composes directly with the field-level mutation store in the asset registry.
5. **Scale**: The quality model must perform at 1M+ assets. Scoring cannot be a bottleneck for exports, dashboards, or onboarding bulk imports.
6. **Composability with state resolution**: The quality model can evaluate any of the four view modes (approved/provisional x current/historical) from the asset registry. It defaults to approved-as-of-today but supports impact previews showing how pending changes would affect scores.

## Decision

### Three Scoring Dimensions

The data quality model evaluates assets across three orthogonal dimensions. Each dimension produces an independent score. Aggregate scores are computed from the three dimensions but the dimensional breakdown is always preserved.

#### 1. Completeness

Completeness measures how many required and recommended fields are populated on an asset record.

```
CompletenessScore {
    entity_id:          UniqueID
    entity_type:        EntityType
    score:              f64             // 0.0 to 1.0
    required_total:     u32
    required_populated: u32
    recommended_total:  u32
    recommended_populated: u32
    missing_required:   Vec<String>     // field names
    missing_recommended: Vec<String>    // field names
    evaluated_at:       Timestamp
}
```

**Scoring logic:**
- Required fields carry more weight than recommended fields. The exact weighting is configurable per pool by CentuRisk admins.
- A field is "populated" if it has a non-null, non-empty value in the resolved asset state.
- Which fields are required vs. recommended is determined by the `CustomFieldDefinition` (the `required` and `recommended` flags) plus the standard field schema for each asset type.
- Completeness is evaluated against the resolved asset state (respecting the temporal and approval axes), not against raw mutations.

#### 2. Accuracy

Accuracy cross-references asset attributes against consistency rules defined from the asset registry data only. No external reference data is consulted in Phase 1.

```
AccuracyScore {
    entity_id:          UniqueID
    entity_type:        EntityType
    score:              f64             // 0.0 to 1.0
    rules_evaluated:    u32
    rules_passed:       u32
    rules_failed:       u32
    failures:           Vec<AccuracyFailure>
    evaluated_at:       Timestamp
}

AccuracyFailure {
    rule_id:            UniqueID
    rule_description:   String          // human-readable description
    fields_involved:    Vec<String>     // the fields the rule checked
    expected:           String          // what the rule expected
    actual:             String          // what was found
}
```

**Example rules:**
- If construction class is "frame" and occupancy is "habitational", then sprinkler field must be present.
- If replacement cost exceeds $10M, then an appraisal date must be within 3 years.
- If asset type is "vehicle", then VIN must match standard format.
- If year built is before 1950, then a renovation date field should be populated.

**Rule definition:**

```
AccuracyRule {
    rule_id:            UniqueID
    pool_id:            UniqueID | null   // null = applies to all pools
    description:        String
    condition:          Expression         // when this rule applies
    assertion:          Expression         // what must be true
    severity:           Required | Recommended
    asset_types:        Vec<AssetType>    // which asset types this rule applies to
    created_by:         ActorID
    created_at:         Timestamp
    active:             bool
}
```

Rules are authored by CentuRisk admins and stored as data. The rule engine evaluates them -- no code changes are needed to add, modify, or deactivate rules. When the Valuation Estimator arrives in Phase 3, its output becomes another attribute available to accuracy rules. The rule engine itself does not change.

#### 3. Recency

Recency evaluates how recently specific CentuRisk-designated fields were updated. It is field-scoped, not asset-scoped.

```
RecencyScore {
    entity_id:          UniqueID
    entity_type:        EntityType
    score:              f64             // 0.0 to 1.0
    tracked_fields:     Vec<FieldRecency>
    evaluated_at:       Timestamp
}

FieldRecency {
    field_name:         String
    last_updated:       Timestamp | null   // null = never updated
    staleness_days:     u32 | null         // null = never updated
    threshold_days:     u32                // CentuRisk-configured freshness window
    is_stale:           bool
}
```

**Scoring logic:**
- CentuRisk configures which specific fields are tracked for recency per pool. This is part of the per-pool scoring configuration.
- A field's recency clock resets only when that specific field receives a new mutation. Updating unrelated fields on the same asset does not affect recency for tracked fields.
- Each tracked field has a CentuRisk-configured freshness threshold (e.g., "replacement cost must be updated within 365 days").
- The recency score is computed from the proportion of tracked fields that are within their freshness window.
- Recency composes directly with the field-level mutation store: each tracked field's recency is the time since its last mutation timestamp.

**Recency configuration per pool:**

```
RecencyConfig {
    pool_id:            UniqueID
    tracked_fields:     Vec<TrackedField>
}

TrackedField {
    field_name:         String
    freshness_days:     u32             // how many days before this field is considered stale
    weight:             f64             // relative importance in the recency score
    asset_types:        Vec<AssetType>  // which asset types this applies to
}
```

### Scoring Rules: CentuRisk-Authored, Per-Pool Customizable

All scoring configuration follows a consistent pattern:

| Aspect | Authored By | Visible To Pool Admin |
|--------|-------------|----------------------|
| Completeness weights (required vs. recommended) | CentuRisk admin | No |
| Accuracy rules (condition + assertion) | CentuRisk admin | No |
| Recency tracked fields and thresholds | CentuRisk admin | No |
| Notification thresholds | Pool admin | Yes (they set these) |

CentuRisk admins can customize all scoring rules per pool. A pool with unusual asset types or regulatory requirements gets pool-specific rules. The default rule set applies to all pools unless overridden.

```
ScoringConfiguration {
    pool_id:                UniqueID
    completeness_weights:   CompletenessWeights
    accuracy_rules:         Vec<AccuracyRule>
    recency_config:         RecencyConfig
    configured_by:          ActorID
    configured_at:          Timestamp
}
```

### Notification Thresholds: Pool-Admin-Configurable

Notification thresholds are the one piece pool administrators control. They determine what score levels trigger quality alerts. This separation means CentuRisk defines what "good data" means; the pool decides what scores warrant attention.

```
NotificationThreshold {
    pool_id:            UniqueID
    dimension:          Completeness | Accuracy | Recency
    threshold_value:    f64             // 0.0 to 1.0
    direction:          DroppedBelow | RoseAbove
    scope:              AssetLevel | MemberLevel | PoolLevel
    asset_types:        Vec<AssetType> | null  // null = all types
    hierarchy_level:    u8 | null       // null = all levels
    configured_by:      ActorID
    configured_at:      Timestamp
}
```

**Threshold scoping:**
- Pool administrators can set thresholds per quality dimension.
- Thresholds can optionally be scoped to specific asset types or hierarchy levels.
- The `direction` field distinguishes between "alert when score drops below X" and "notify when score rises above X" (for improvement tracking).

### Quality Events

Quality events are the primary observability mechanism -- structured signals emitted at the boundary between scoring and action. They are available for downstream consumption by notification adapters, dashboards, and analytics.

```
QualityEvent {
    event_id:           UniqueID
    entity_id:          UniqueID
    entity_type:        EntityType      // Asset | Member | Pool
    dimension:          Completeness | Accuracy | Recency
    current_score:      f64
    previous_score:     f64 | null      // null on first evaluation
    threshold:          f64
    direction:          DroppedBelow | RoseAbove
    triggered_at:       Timestamp
    view_mode:          ViewMode        // which of the 4 view modes was active
    metadata:           Map<String, Value>  // extensible context
}
```

**Event emission rules:**
- A `QualityEvent` fires when a score crosses a threshold the pool admin has set.
- Events are emitted for the specific dimension that crossed the threshold, not for aggregate scores.
- Events carry enough context for downstream consumers to act without re-querying the quality model.
- Events are immutable once emitted. They form a time-series of quality state changes.

**Downstream consumers of quality events:**
- Notification adapters: Send alerts to pool administrators (email, in-app).
- Member dashboard: Display quality trend indicators.
- Analytics: Track quality improvement over time, identify systemic data collection gaps.
- Recommendations engine (future): Use quality events as input signals.

### Performance at Scale

The quality model must perform at 1M+ assets. Key performance decisions:

1. **Incremental scoring**: When a field mutation occurs, only the affected dimensions are rescored for that asset. A replacement cost update triggers recency rescoring for that field and completeness rescoring, but does not re-evaluate unrelated accuracy rules.

2. **Batch scoring for bulk import**: During historical data onboarding, quality scoring runs asynchronously after the import completes, not inline with each record insert.

3. **Materialized quality scores**: Current quality scores are materialized (cached) per asset and invalidated on mutation. Dashboard queries read materialized scores, not recompute them.

4. **Aggregate score computation**: Member-level and pool-level aggregate scores are computed from asset-level scores using configurable aggregation (weighted average, minimum, or percentile). These aggregates are also materialized and invalidated when underlying asset scores change.

```
MaterializedQualityScore {
    entity_id:          UniqueID
    entity_type:        EntityType
    dimension:          Completeness | Accuracy | Recency
    score:              f64
    computed_at:        Timestamp
    view_mode:          ViewMode
    valid:              bool            // false when underlying data has changed
}
```

### Scoring Pipeline

The scoring pipeline is a pure function in the exposure core:

```
score_asset(
    resolved_state:     ResolvedAssetState,
    scoring_config:     ScoringConfiguration,
    thresholds:         Vec<NotificationThreshold>
) -> ScoringResult

ScoringResult {
    completeness:       CompletenessScore
    accuracy:           AccuracyScore
    recency:            RecencyScore
    events:             Vec<QualityEvent>    // threshold crossings detected
}
```

The pipeline takes resolved asset state (from the asset registry's state resolution engine) and scoring configuration (from CentuRisk admin settings), and produces scores and events. It has no side effects, no I/O, and no knowledge of where the data came from or where the events go.

## Alternatives Considered

### External Reference Data for Accuracy
Using external databases (Marshall & Swift, RS Means, property tax records) for accuracy scoring was considered. This was rejected for Phase 1 because:
- It introduces external dependencies into the pure core.
- The asset registry already contains enough cross-referenceable attributes for meaningful accuracy scoring.
- External data sources can be added in later phases as additional attributes available to the rule engine, without changing the scoring architecture.

### Asset-Scoped Recency
Treating recency as a single "last updated" timestamp per asset was rejected because:
- Updating a contact name would reset the recency clock on replacement cost, masking stale valuation data.
- Field-scoped recency composes naturally with the field-level mutation store -- no additional tracking infrastructure is needed.
- Pool administrators need to know specifically which fields are stale, not just that "something changed recently."

### Pool-Admin-Visible Scoring Rules
Exposing scoring rule configuration to pool administrators was rejected because:
- CentuRisk's value proposition includes defining what "good data" means for each pool.
- Pool administrators lack the context to author cross-referencing accuracy rules.
- Separating scoring rules (CentuRisk) from notification thresholds (pool admin) gives each party control over their domain.
- The threshold configuration gives pool administrators meaningful control without exposing rule complexity.

### Real-Time Scoring on Every Query
Computing quality scores on every dashboard or export request was rejected because:
- At 1M+ assets, real-time recomputation is prohibitively expensive.
- Materialized scores with invalidation-on-mutation provide near-real-time accuracy with predictable performance.
- The incremental scoring approach (rescore only affected dimensions on mutation) keeps materialized scores fresh without full recomputation.

## Consequences

**Positive outcomes:**
- Three orthogonal dimensions (completeness, accuracy, recency) provide actionable, specific quality signals rather than a single opaque score.
- Field-scoped recency prevents masking of stale critical fields by unrelated updates.
- CentuRisk-authored rules with per-pool customization support diverse pool requirements without exposing complexity to pool administrators.
- Pool-admin-configurable thresholds give administrators meaningful control over their alert sensitivity.
- Quality events as structured boundary contracts decouple scoring from notification/action, enabling flexible downstream consumption.
- The scoring pipeline is a pure function, making it exhaustively testable with property-based tests.
- Materialized scores with incremental invalidation support 1M+ asset scale.

**Negative outcomes and trade-offs:**
- Materialized quality scores introduce eventual consistency -- a brief window after mutation where the dashboard score is stale. The window is bounded by the rescoring latency.
- Field-level recency tracking requires CentuRisk admins to explicitly designate tracked fields per pool, adding configuration burden during onboarding.
- The accuracy rule language must be expressive enough for cross-attribute checks but not so complex that it becomes a maintenance burden. The rule expression language needs careful design.
- Bulk import triggers a large batch of scoring work. The asynchronous scoring pipeline must handle this gracefully with progress tracking and backpressure.
- Three separate dimension scores create a richer but more complex quality model. UX for the member dashboard must distill this into actionable guidance without overwhelming users.

**New constraints introduced:**
- Every field mutation must trigger incremental quality rescoring for affected dimensions.
- The scoring pipeline must be a pure function in the core -- no I/O, no side effects.
- Quality events must be emitted at the boundary, not inside the scoring function. The scoring function returns events; the adapter layer emits them.
- Materialized scores must be invalidated atomically when underlying mutations are applied.
- CentuRisk admin tooling must support authoring and testing accuracy rules before deploying them to a pool.

## Implementation Plan

1. **Completeness scoring** -- Implement the completeness dimension as a pure function that takes resolved asset state and field definitions (required/recommended flags) and returns a `CompletenessScore`. Test with property-based tests varying field population. Delivers: the simplest quality dimension, end-to-end testable.

2. **Accuracy rule engine** -- Implement the rule evaluation engine that evaluates `AccuracyRule` conditions and assertions against resolved asset state. Start with 3-5 representative rules (construction class/sprinkler, replacement cost/appraisal date, VIN format). Test each rule in isolation and in combination. Delivers: the cross-referencing accuracy dimension.

3. **Field-scoped recency scoring** -- Implement recency evaluation using the field-level mutation timestamps from the asset registry. Configure tracked fields and freshness thresholds. Test that updating an untracked field does not affect recency scores. Delivers: the recency dimension with correct field-scoping.

4. **Scoring pipeline composition** -- Compose all three dimensions into the `score_asset` pure function. Add threshold evaluation and `QualityEvent` generation. Test with property-based tests covering threshold crossings in both directions. Delivers: the complete scoring pipeline.

5. **Materialized scores and incremental invalidation** -- Implement materialized quality score storage. Build the invalidation path triggered by field mutations. Verify that only affected dimensions are rescored. Load test with simulated 1M asset dataset. Delivers: production-scale scoring performance.

6. **Notification threshold configuration adapter** -- Build the pool-administrator-facing UI adapter for setting notification thresholds per dimension, asset type, and hierarchy level. This is the first impure-edge component for the quality model. Delivers: pool admin control over alert sensitivity.

7. **Quality event emission and downstream wiring** -- Build the adapter that takes `QualityEvent` outputs from the scoring pipeline and routes them to notification channels and dashboards. Delivers: actionable quality alerts reaching pool administrators.
