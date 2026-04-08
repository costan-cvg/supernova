# ADR: CentuRisk Asset Registry

## Status
Proposed

## Context

The asset registry is the foundational data model for CentuRisk. Every downstream module -- data quality scoring, SOV generation, CAT model exports, recommendations, and the member portal -- depends on the asset registry for exposure data. The registry must answer several difficult questions simultaneously:

1. **Identity**: How is an asset uniquely identified when it may change pools, addresses, and every mutable attribute over its lifetime?
2. **Hierarchy**: Pools organize assets in hierarchies (Pool > Member > Campus > Building > Unit), but depth and labels vary per pool. How do we query efficiently without coupling to a specific hierarchy depth?
3. **Temporal state**: A building added mid-year, a replacement cost updated in March, and a construction class correction in September all have different effective dates. How does the system resolve "what does this asset look like as of date X?"
4. **Approval workflow**: Members submit changes that pool administrators approve. Exports need approved values; impact previews need pending values. How do these two axes (temporal + approval) compose?
5. **Custom fields**: Pools have pool-specific attributes beyond the standard schema. How are these defined, modified, and audited?
6. **Asset types**: Buildings, contents, vehicles, and fine arts share some attributes but differ in others. How do we extend to new types without schema changes?

The registry must be the "deterministic, isolated heart of the system" -- containing no I/O, no authorization logic, and no knowledge of external formats.

## Decision

### Asset Identity

Every asset receives a CentuRisk-generated unique ID that is independent of pool membership, address, or any mutable attribute. This ID is the key for deterministic differential computation: diffs are always reproducible because identity never depends on attributes that might change.

```
AssetIdentity {
    asset_id:   UniqueID           // system-generated, immutable
    pool_id:    UniqueID           // current pool context (from access grant, not ownership)
    path:       MaterializedPath   // hierarchy position, e.g. "/pool-123/member-456/campus-789/building-012"
    asset_type: AssetType          // building | contents | vehicle | fine_arts
    lifecycle:  LifecycleState     // Draft | Active | PendingChange | Archived
    created_at: Timestamp
    created_by: ActorID
}
```

The `asset_id` never changes. The `path` changes only during rare administrative restructuring operations. The `pool_id` reflects the current access context, not ownership (per the data ownership model in ADR_centurisk-system-overview).

### Hierarchy: Materialized Path

Each asset stores its full ancestry as a delimited string. The hierarchy depth is configurable per pool, defaulting to 5 levels. Labels at each level are also configurable per pool.

**Example paths:**
```
/pool-123/member-456/campus-789/building-012/unit-345    (5 levels)
/pool-123/member-456/building-012                         (3 levels)
```

**Query patterns:**
- Descendants of a node: `WHERE path LIKE '/pool-123/member-456/%'`
- Aggregation at any level: `GROUP BY substring(path, 1, level_prefix_length)`
- Access control scoping: prefix filter on path, composing with relationship-based access grants.

**Key properties:**
- The core domain logic never traverses the tree. It operates on flat asset records carrying lineage as data.
- Whether a pool uses 3 levels or 5, queries are structurally identical -- only path length changes.
- Path recomputation happens only on rare administrative moves.

**Hierarchy configuration per pool:**

```
HierarchyConfig {
    pool_id:        UniqueID
    max_depth:      u8              // default 5
    level_labels:   Vec<String>     // e.g. ["Pool", "Member", "Campus", "Building", "Unit"]
}
```

### Temporal Model: Field-Level Mutations with Configurable Resolution

The storage model captures field-level mutations with per-field effective dates. This is the most granular option -- record-level snapshots can always be derived from field-level data, but not the reverse.

**Mutation record:**

```
FieldMutation {
    mutation_id:    UniqueID
    asset_id:       UniqueID
    field_name:     String
    value:          Value           // typed, serialized field value
    effective_date: Date            // when this value takes effect
    submitted_at:   Timestamp       // when the change was submitted
    submitted_by:   ActorID
    approved_at:    Timestamp | null
    approved_by:    ActorID | null
    approval_state: Pending | Approved | Rejected
}
```

**Resolution strategies** are configurable per pool by CentuRisk admins:

| Strategy | Behavior | Use Case |
|----------|----------|----------|
| **Record-level** | Latest full snapshot before date X | Pools that treat assets as atomic units |
| **Field-by-field** | Latest value per field before date X | Pools that track granular changes |
| **Conditional** | Rules that may be attribute-dependent or value-dependent | Pools with complex resolution requirements (e.g., different rules for high-value vs. low-value assets) |

```
ResolutionStrategy {
    pool_id:        UniqueID
    strategy_type:  RecordLevel | FieldByField | Conditional
    rules:          Vec<ResolutionRule>   // only used for Conditional
}

ResolutionRule {
    condition:      Expression            // e.g., "replacement_cost > 1_000_000"
    resolution:     RecordLevel | FieldByField
}
```

The resolution strategy is domain core logic -- pure and deterministic. It evaluates configurable rules without knowing why a particular pool chose that rule. The pool-facing configuration UI is deferred to a later phase; Phase 1 uses CentuRisk-configured rules.

### State Resolution: Two Axes, Four View Modes

Asset state resolves along two independent axes:

| | **Approved Only** | **Approved + Pending** |
|---|---|---|
| **Current Date** | Default view. Exports use this. | Impact preview. Shows what pending changes would do. |
| **Specific Date** | Policy-period lookback. Historical queries. | Impact analysis for future or past dates. |

**Resolution function signature:**

```
resolve_asset_state(
    asset_id:       UniqueID,
    as_of_date:     Date,               // temporal axis
    include_pending: bool,              // approval axis
    strategy:       ResolutionStrategy  // pool-configured
) -> ResolvedAssetState
```

```
ResolvedAssetState {
    asset_id:       UniqueID
    as_of_date:     Date
    includes_pending: bool
    fields:         Map<String, ResolvedFieldValue>
    lifecycle:      LifecycleState
}

ResolvedFieldValue {
    value:          Value
    effective_date: Date
    approval_state: Pending | Approved
    source_mutation: UniqueID           // traceability back to the mutation
}
```

**Downstream consumer rules:**
- Exports (CAT model, SOV generation): Always resolve to approved values at a specific effective date.
- Quality model: Scores all four view modes, defaulting to approved-as-of-today.
- Impact reports: Show what pending changes would do to scores and export data for any effective date.

### Custom Fields

Custom fields are defined by CentuRisk during pool onboarding and modifiable by pool administrators afterward. All changes carry an auditable history visible to CentuRisk system admins.

```
CustomFieldDefinition {
    field_id:       UniqueID
    pool_id:        UniqueID
    field_name:     String
    field_type:     FieldType          // String | Number | Date | Boolean | Enum
    required:       bool
    recommended:    bool               // feeds completeness scoring
    asset_types:    Vec<AssetType>     // which asset types this field applies to
    validation:     ValidationRule | null
    created_at:     Timestamp
    created_by:     ActorID
}

CustomFieldChange {
    change_id:      UniqueID
    field_id:       UniqueID
    changed_by:     ActorID
    changed_at:     Timestamp
    change_type:    Created | Modified | Deactivated
    previous_state: CustomFieldDefinition | null
    new_state:      CustomFieldDefinition
}
```

### Asset Types: Composition Model

Asset types follow a composition model rather than inheritance. Buildings, contents, vehicles, and fine arts share common attributes with type-specific extensions. New asset types are data-driven extensions, not schema changes.

**Common attributes** (shared across all asset types):
- Asset ID, hierarchy path, lifecycle state
- Description, location (address, coordinates)
- Replacement cost, deductible
- Coverage effective dates
- Custom fields applicable to all types

**Type-specific extensions:**

| Asset Type | Example Type-Specific Fields |
|------------|------------------------------|
| **Building** | Construction class, occupancy, year built, square footage, stories, sprinkler system, roof type, COPE data |
| **Contents** | Associated building, content category, inventory method |
| **Vehicle** | VIN, year, make, model, vehicle class, garage location |
| **Fine Arts** | Appraised value, appraisal date, artist, medium, storage location |

The type-specific extension set is configurable by CentuRisk admins. Adding a new asset type means defining its extension fields as data, not changing the schema.

```
AssetTypeDefinition {
    type_id:            UniqueID
    type_name:          String          // "building", "contents", "vehicle", "fine_arts"
    common_fields:      Vec<FieldSpec>  // shared attributes
    type_specific_fields: Vec<FieldSpec>  // type-only attributes
    pool_overrides:     Map<UniqueID, Vec<FieldSpec>>  // per-pool customizations
}
```

### Lifecycle States

Assets move through four lifecycle states:

```
Draft --> Active --> PendingChange --> Active (after approval)
                                  --> Active (rejected, reverts)
Active --> Archived
Archived --> Active (restore, rare)
```

| State | Meaning | Visibility |
|-------|---------|------------|
| **Draft** | Newly created, not yet submitted for approval | Visible to creator and pool admin |
| **Active** | Approved and current | Visible in all views and exports |
| **PendingChange** | Active asset with unapproved modifications | Active values in exports; pending values in impact previews |
| **Archived** | Removed from active portfolio | Excluded from exports; visible in historical queries |

Lifecycle state interacts with the temporal model: an archived asset's historical mutations remain queryable. Archiving does not delete data -- it changes the asset's visibility in current-date views.

## Alternatives Considered

### Surrogate Key from Pool + Address
Using a composite key from pool membership and address was rejected because:
- Members migrate between pools, changing the pool component.
- Addresses are mutable attributes that get corrected.
- Differential computation requires stable identity to produce reproducible diffs.

### Tree-Based Hierarchy (Adjacency List / Nested Sets)
A recursive tree structure was rejected because:
- The core would need tree traversal logic, violating the flat-record principle.
- Queries at different hierarchy depths would require different query shapes.
- Materialized path gives O(1) ancestry lookup and simple prefix-match queries.
- The hierarchy is essentially static (changes only during rare corrections), so the recomputation cost of materialized paths is negligible.

### Record-Level Snapshots Only
Storing full asset snapshots instead of field-level mutations was rejected because:
- Field-level resolution (which pools require) cannot be derived from record-level snapshots.
- Record-level snapshots can always be derived from field-level data, but not the reverse.
- Field-level mutations compose directly with field-scoped recency scoring in the quality model.
- Storage cost for field-level mutations is higher, but the granularity is essential for the temporal model and quality scoring.

### Inheritance-Based Asset Types
A class hierarchy (BuildingAsset extends Asset) was rejected because:
- Adding new asset types would require schema changes and code modifications.
- The composition model allows CentuRisk admins to define new types as data.
- Shared behavior operates on common attributes; type-specific behavior operates on extensions.

## Consequences

**Positive outcomes:**
- Stable, system-generated identity enables deterministic diffs and reliable cross-system references.
- Materialized paths make hierarchy queries hierarchy-depth-agnostic with simple prefix matching.
- Field-level mutations provide maximum temporal granularity, supporting all resolution strategies.
- The four view modes (2x2 of temporal and approval axes) give every downstream consumer exactly the data perspective it needs.
- Composition-based asset types allow data-driven extension without code changes.
- Custom field audit history satisfies CentuRisk system admin oversight requirements.

**Negative outcomes and trade-offs:**
- Field-level mutations increase storage volume compared to record-level snapshots. At scale (1M+ assets with decades of history), the mutation store will be large and requires indexing strategy.
- Resolving asset state requires aggregating across multiple mutation records, which is computationally more expensive than reading a single snapshot. Caching or materialized views may be needed for performance.
- Materialized paths must be recomputed on hierarchy moves. While moves are rare, the recomputation must be atomic and propagated to all descendants.
- The configurable resolution strategy adds complexity to every state-resolution query. The resolution engine must be well-tested across all three strategy types.

**New constraints introduced:**
- Every write to an asset field must produce a `FieldMutation` record -- no direct overwrites.
- The `resolve_asset_state` function must be a pure function in the core, taking strategy as input.
- Custom field definitions must be loaded as part of asset state resolution to determine which fields are valid for a given asset type and pool.
- Lifecycle state transitions must be enforced: only valid transitions are permitted, and each transition produces an audit entry.

## Implementation Plan

1. **Asset identity and hierarchy storage** -- Implement `AssetIdentity` with system-generated IDs and materialized paths. Build prefix-match queries for descendant lookup and aggregation. Test with 3-level and 5-level hierarchies to verify depth-agnostic behavior. Delivers: the foundational data model other modules build on.

2. **Field-level mutation store** -- Implement `FieldMutation` storage with effective dates and approval states. Build the write path that creates mutation records for every field change. Verify with unit tests that mutations are immutable and carry full provenance. Delivers: the temporal foundation for state resolution.

3. **State resolution engine** -- Implement `resolve_asset_state` as a pure function supporting all three resolution strategies (record-level, field-by-field, conditional). Test each strategy with property-based tests covering edge cases (same-date mutations, gaps, future effective dates). Delivers: the core computation that every downstream consumer depends on.

4. **Four view modes** -- Wire the resolution engine to support all four combinations of temporal and approval axes. Verify that exports see only approved values, impact previews include pending values, and historical queries resolve correctly. Delivers: the complete state resolution API.

5. **Asset type composition and custom fields** -- Implement the composition model for asset types with shared and type-specific attributes. Build the custom field definition and audit trail. Test that new asset types can be added as data without schema changes. Delivers: extensibility for pool-specific configurations.

6. **Lifecycle state machine** -- Implement the Draft > Active > PendingChange > Archived state transitions with enforcement and audit logging. Verify interaction with temporal model (archived assets remain in historical queries). Delivers: the complete asset lifecycle.

7. **Hierarchy configuration UI adapter** -- Build the adapter that allows CentuRisk admins to configure hierarchy depth and labels per pool. This is the first impure-edge component for the registry. Delivers: pool onboarding capability.
