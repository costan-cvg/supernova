# 2. The Exposure Core — Asset Registry

The exposure core is the deterministic, isolated heart of the system. It stores asset data, computes quality scores, resolves temporal state, and produces views. The core contains no I/O, no authorization logic, and no knowledge of external formats. What goes in is standardized; what comes out follows stable contracts.

## Identity

Every asset in the system has a stable, system-generated identity and carries its position in a configurable hierarchy as data, not structure. Each asset receives a CentuRisk-generated unique ID, independent of pool membership, address, or any mutable attribute. This ID is the key for deterministic differential computation: diffs are always reproducible because identity never depends on attributes that might change.

## Hierarchy

Pools organize assets in hierarchies — for example, Pool → Member → Campus → Building → Unit, though the labels and depth vary. The hierarchy depth is configurable per pool, defaulting to 5 levels. The system uses a materialized path pattern: each asset stores its full ancestry as a delimited string. Queries for descendants become prefix matches; aggregation at any level is a GROUP BY on a prefix substring. Because the core operates on flat records carrying lineage as data rather than enforcing structure, it is hierarchy-depth-agnostic. A query works identically whether a pool uses 3 levels or 5.

## Temporal Model

The core stores field-level mutations with per-field effective dates. A building added mid-year carries its own effective date. A replacement cost updated in March has a different effective date than a construction class correction in September. How that temporal state is resolved — the resolution strategy — is configurable per pool. Some pools resolve at record level (latest full snapshot before date X), others field-by-field (latest value per field before date X), and others use conditional rules that may be attribute-dependent or value-dependent. CentuRisk configures these strategies during pool onboarding. The rule engine exists in the core; the pool-facing configuration UI is deferred to a later phase.

## State Resolution

Asset state resolves along two axes: temporal (as of what effective date?) and approval state (approved values only, or approved-plus-pending?). This produces four view modes. Current state shows approved values at the present date — the default view. Historical queries show approved values at a specific past or future date. Provisional modes show approved-plus-pending at current date or at a specific date. Exports always use approved values. The quality model can evaluate any combination.

## Custom Fields and Asset Types

CentuRisk admins define custom fields during onboarding. Pool administrators can modify them afterward. All changes carry an auditable history. Asset types follow a composition model — buildings, contents, vehicles, and fine arts share common attributes with type-specific extensions. New asset types are data-driven extensions, not schema changes. Lifecycle states (Draft, Active, Pending Change, Archived) interact with the temporal model and approval routing.
