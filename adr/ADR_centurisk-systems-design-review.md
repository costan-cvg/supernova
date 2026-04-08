# Systems Design Review: CentuRisk RMIS

## Executive Summary

- Architecture Quality: **Good, with targeted weaknesses**
- Boundary Violations: 4 (two critical, two high)
- Undocumented Trade-offs: 6
- Contract Completeness Gaps: 5
- Scalability Risks: 3 (all acknowledged but underspecified)
- Consistency Gaps Across Modules: 3

The architecture demonstrates strong foundational thinking: the pure-core / impure-edges principle is stated clearly and consistently referenced, boundary contracts are named and typed, the adapter pattern is applied uniformly, and deferred phases carry concrete integration contracts. The Cedar Policy decision for unified ABAC is architecturally sound and well-reasoned.

The weaknesses cluster in three areas: (1) the temporal model at scale has gaps that the spikes may not surface early enough, (2) several boundary contracts have untyped fields (`Any`, free `String`) that will become integration liabilities, and (3) Cedar enforcement has a gap between what the ADR claims and what the delivery sequence actually enforces.

---

## P0 Issues — Must Fix Before Implementation Commits

### [P0] Cedar Enforcement Gap: Adapter Layer Is Not the Core, But Enforcement Is Delayed Until Increment 5

**Location:** `ADR_centurisk-access-control.md`, `ADR_centurisk-delivery-strategy.md` (Increments 2-4)

**Problem:** The access control ADR correctly defines Cedar as an adapter-layer concern, sitting between every interface and the exposure core. However, the delivery sequence builds asset CRUD (Increment 2), data quality scoring (Increment 3), and the SOV pipeline with approval routing (Increment 4) before Cedar integration is added in Increment 5. This means three increments ship real data paths — including writes — with no authorization enforcement.

The ADR states: "Every new interface or adapter must integrate with the Cedar policy engine — no interface may bypass it." The increment sequence violates this constraint by construction.

**Impact:** If Increment 2-4 code is not refactored when Cedar is added in Increment 5 (a common outcome under schedule pressure), production data paths will exist without authorization enforcement. This is not a theoretical risk — it is the default outcome of the stated delivery sequence.

**Solution:** Increment 5 (Cedar) must either be pulled to immediately follow Increment 1 as a prerequisite gate, or Increments 2-4 must explicitly document that every data path built during those increments is marked with a TODO requiring Cedar integration and audited against that list in Increment 5. The delivery ADR should state this explicitly as a constraint, not leave it implicit.

---

### [P0] Untyped `Any` Fields at SOV Pipeline Boundary

**Location:** `ADR_centurisk-sov-pipeline.md` — `FieldChange` contract

**Problem:** The `FieldChange` contract defines:

```
FieldChange {
  previous_value:  Any | null
  proposed_value:  Any
  is_valuation_field: Boolean
}
```

`Any` is an untyped external boundary. This contract sits at the interface between the SOV pipeline and the approval workflow — one of the most security-critical paths in the system. Approval routing decisions depend on correctly interpreting `proposed_value` for valuation changes. An untyped `Any` means:

- The approval workflow cannot perform type-safe checks on valuation field content.
- Schema changes to field types can silently change the semantic meaning of `Any` values already in the approval queue.
- Serialization edge cases (null vs. absent vs. empty string) are invisible at the contract level.

The same problem appears in `ResolvedFieldValue.value: Value` in the asset registry ADR, where `Value` is used without definition.

**Impact:** Undocumented breaking behavior when field types change. Runtime deserialization errors in the approval workflow under edge inputs. Blocked gate per the contract completeness principle.

**Solution:** Define a `FieldValue` discriminated union at the contract boundary:

```
FieldValue =
  | TextValue { text: String }
  | NumericValue { number: Decimal }
  | DateValue { date: Date }
  | BooleanValue { value: bool }
  | EnumValue { variant: String }
  | NullValue
```

This is the same type system the `CustomFieldDefinition.field_type` enum already implies. The `FieldChange` contract should reference it explicitly.

---

## P1 Issues — Should Fix Before Proceeding to Full Implementation

### [P1] Temporal Resolution at Scale: The Spike Is Necessary But Its Output Is Underspecified

**Location:** `ADR_centurisk-infrastructure.md` (Performance Targets), `ADR_centurisk-delivery-strategy.md` (Spike 1)

**Problem:** The performance target for temporal state resolution is 50ms for a single asset and 500ms for a batch of 100 at 1M assets / 90M mutation records. This is the right question to spike. However, the spike output specification ("performance data per strategy, recommended approach") does not define what happens if the spike finds no strategy meets the target.

At 90M mutation records indexed by `(asset_id, field_name, effective_date)`, the resolution query for a single asset requires filtering on `asset_id`, then selecting the latest record per `field_name` before `as_of_date`. With standard B-tree indexes, this is a multi-step scan. At 1M assets with an average of 90 mutations each, even a 1ms-per-mutation query produces 90ms for a single asset — already over the target. The ADR acknowledges "caching or materialized views may be needed" but does not commit to them as a required architectural element.

**Impact:** The temporal model is the foundational data structure. If Spike 1 finds the target cannot be met without pre-computed snapshots, the entire write path must change — mutations must also produce snapshots at specific effective dates. This is not a tuning change; it is an architectural change. Making this discovery in Increment 2 vs. Increment 14 is the difference between a course correction and a rewrite.

**Solution:** The spike specification should include a concrete fallback decision tree:
- If indexed scan meets targets: proceed with pure field-level mutation store.
- If not: adopt a hybrid model — field-level mutations for provenance, generated record-level snapshots at `effective_date` boundaries for fast resolution.
- The snapshot generation strategy must be defined before the mutation write path is built, because snapshots must be produced in the same transaction as mutations.

The delivery ADR should gate Increment 2 on Spike 1 completing with an affirmative outcome, not just a "finding."

---

### [P1] Resolution Strategy Configurable Per Pool: The Conditional Strategy Has No Expression Language Spec

**Location:** `ADR_centurisk-asset-registry.md` — `ResolutionRule.condition`

**Problem:** The conditional resolution strategy uses `Expression` as the type for both the condition and the assertion in `ResolutionRule`. No expression language is defined. The example given ("replacement_cost > 1_000_000") implies a predicate language over field values, but:

- No grammar is specified.
- Operator set is undefined (arithmetic, comparison, string matching, date arithmetic?).
- Type coercion rules are unspecified.
- Error handling for invalid expressions is absent.
- The same `Expression` type is reused in `AccuracyRule` with a different semantic context (condition = "when this rule applies" vs. "what must be true").

The accuracy rule engine in the data quality ADR depends on the same expression language for cross-attribute checks like "if construction class is 'frame' AND occupancy is 'habitational', then sprinkler field must be present." This is a non-trivial predicate language requirement.

**Impact:** Without a specified expression language, the accuracy rule engine and the conditional resolution strategy are both unimplementable. Two different teams building these two systems will produce incompatible expression parsers, violating the composability principle. This is a hidden coupling that the ADR does not expose.

**Solution:** Define a single expression language (or reference an existing one such as CEL — Common Expression Language) that is shared between `ResolutionRule.condition` and `AccuracyRule.condition`/`AccuracyRule.assertion`. Specify the operator set, type system, and error behavior in the asset registry ADR, and reference it from the data quality ADR. The expression language spec is a shared dependency that belongs in the system overview or as its own ADR.

---

### [P1] Materialized Quality Score Invalidation: The Aggregate Invalidation Chain Is Not Documented

**Location:** `ADR_centurisk-data-quality-model.md` — `MaterializedQualityScore`

**Problem:** The ADR states that "materialized scores must be invalidated atomically when underlying mutations are applied." It also states that member-level and pool-level aggregate scores are materialized and invalidated when underlying asset scores change. However, the invalidation chain is not specified:

A single field mutation on one asset must trigger:
1. Incremental rescoring of affected dimensions for that asset.
2. Invalidation of the asset-level `MaterializedQualityScore`.
3. Propagation of the delta to the member-level aggregate.
4. Propagation of that delta to the pool-level aggregate.
5. Threshold evaluation at each aggregate level for `QualityEvent` emission.

At 1M assets with potentially thousands of concurrent mutations during bulk import, the fan-out from a single mutation to pool-level aggregate invalidation is a consistency bottleneck. The ADR identifies that "bulk import triggers a large batch of scoring work" but does not specify whether the aggregate chain is eager (every mutation triggers the chain) or lazy (aggregates are recomputed on read or at a scheduled interval).

**Impact:** If eager, concurrent bulk import at 1M records produces O(N) pool-level aggregate updates — a write amplification problem. If lazy, the dashboard shows stale aggregates during and after bulk import without clear user communication. Neither behavior is documented, so the implementation will make an undocumented choice.

**Solution:** Document the invalidation strategy explicitly:
- Specify whether aggregate invalidation is eager, lazy, or event-driven (e.g., debounced).
- Define the "eventual consistency window" for pool-level aggregates during bulk import.
- Specify whether `QualityEvent` emission is blocked during bulk import (to avoid flooding the notification system) or rate-limited.
- Add a note to the data quality ADR Consequences section documenting this trade-off.

---

### [P1] Multi-Tenancy Key Grant Revocation: No Defined Access Window for Old Pool After Transition

**Location:** `ADR_centurisk-infrastructure.md` — Encryption and Key Management

**Problem:** When a member migrates between pools, the key management rules state: "the old pool retains read access to historical data produced during its tenure via a time-bounded grant if required by regulation." This is the correct principle. However:

- The duration of the time-bounded historical read grant is not specified.
- Whether the old pool's access is limited to a specific date range (e.g., only data from before `effective_to`) is not specified.
- The regulatory driver for this retention is mentioned as a possibility ("if required by regulation") but the specific regulations governing public sector risk pool data retention are not referenced.
- There is no contract for what happens when the time-bounded grant expires: does the old pool receive notification? Is expired grant data purged?

**Impact:** The `EncryptionKeyGrant` contract includes `revoked_at` but not a `historical_access_until` or equivalent field. The grant revocation logic in the implementation will either over-restrict (old pool loses access immediately, causing regulatory exposure) or under-restrict (old pool retains indefinite access, causing a privacy violation) depending on how the developer interprets "time-bounded."

**Solution:** Add a `historical_access_scope` field to `EncryptionKeyGrant`:

```
EncryptionKeyGrant {
  ...
  revoked_at: Timestamp?,
  historical_access_until: Date?,     // null = no historical access after revocation
  historical_access_scope: DateRange? // which data timestamps the old pool may still read
}
```

Acknowledge that the duration of historical access must be configured per pool based on the regulatory jurisdiction of the member (state varies for public entities). This makes the regulatory dependency explicit rather than implicit.

---

### [P1] NL Query Layer: Confidence Threshold Is System-Wide But Should Be Per-Pool or Per-Field-Set

**Location:** `ADR_centurisk-nl-querying.md` — Low-Confidence Handling

**Problem:** The ADR states "the confidence threshold is configurable" but does not specify who configures it, at what scope, or via what interface. Given that pools have pool-specific custom fields with different synonym registries, a single system-wide confidence threshold will produce inconsistent behavior: a query against a custom field with a narrow synonym set will have consistently lower confidence than a query against standard fields, causing pool-specific custom field queries to always fall into the suggestion flow even when they are unambiguous.

Additionally, there is no documented behavior for what happens when a low-confidence query is Cedar-constrained: if the system cannot confidently resolve a query and also cannot suggest alternatives (because the alternatives would reference fields the user cannot see), the ADR does not specify the output.

**Solution:** Specify that the confidence threshold is configurable per pool by CentuRisk admins (consistent with the scoring rule authorship pattern). Document the edge case where Cedar restrictions eliminate all viable suggestions and specify what the system returns in that case (e.g., "No matching query could be constructed within your access scope").

---

### [P1] Bulk Import: No Idempotency Key or Duplicate Detection Strategy Across Pipeline Runs

**Location:** `ADR_centurisk-io-adapters.md`, `ADR_centurisk-infrastructure.md` — Bulk Import Pipeline

**Problem:** The bulk import pipeline supports resumable stages, but neither ADR specifies idempotency behavior across separate pipeline runs. A pool onboarding historical data may submit the same CSV twice (operator error, retry after perceived failure). Stage 5 (AssignAssetIds) is described as "assign new or match existing," but:

- The matching criteria for existing assets are not specified.
- If the same CSV is submitted twice, what prevents 1M duplicate assets from being created?
- `DuplicateDetected` appears in the `ImportError.error_type` enum but the deduplication algorithm is not specified.

At the data volume described (decades of history, tens of millions of assets), silent deduplication failures would be catastrophic and difficult to detect after the fact.

**Solution:** Define the asset ID matching strategy in Stage 5 explicitly:
- Primary key: system-generated `asset_id` (requires the source file to carry it, which is only possible for re-imports of previously imported data).
- Natural key fallback: specify the combination of fields treated as a natural key for deduplication (e.g., `member_id + address + asset_type + year_built`).
- Document the behavior when a natural key match finds conflicting values vs. identical values.
- Add an `import_idempotency_key: String` to `BulkImportJob` that prevents re-processing the same source file.

---

## P2 Issues — Note and Proceed

### [P2] `LossEvent` Contract Is Underspecified for ML-Readiness

**Location:** `ADR_centurisk-recommendation-engine.md`

The `LossEvent` contract captures `severity_estimate: String` (free text) and `event_type: String` (free text). When the ML engine arrives in a future phase, these fields will require normalization and vocabulary control. Free-text severity and event type will produce a noisy training dataset. Suggest adding controlled vocabulary enums with an escape hatch (`Other { description: String }`) to preserve flexibility while establishing structure.

---

### [P2] `segment_id: String?` on Assets Is an Implicit Cross-Phase Contract

**Location:** `ADR_centurisk-delivery-strategy.md` — Policy and Coverage Management deferred item

The delivery ADR introduces `segment_id: String?` on assets as a placeholder for the Phase 4 policy link. This field is an opaque string with no validation, no referential integrity, and no documented format. If any consuming system (SOV generation, CAT export, NL querying) starts treating `segment_id` as meaningful data before Phase 4 formalizes it, changes to the policy link model will break those consumers. Recommend explicitly marking `segment_id` as an internal staging field that must not be exposed through any adapter output until Phase 4.

---

### [P2] `QualityEvent.metadata: Map<String, Value>` Is an Escape Hatch That Will Accrete

**Location:** `ADR_centurisk-data-quality-model.md`

The `metadata: Map<String, Value>` field on `QualityEvent` is described as "extensible context." Extensible maps on event contracts consistently accrete undocumented fields over time, making the event contract effectively untyped. Downstream consumers (notification adapters, analytics, recommendations) that parse `metadata` will develop undocumented dependencies on specific keys. Recommend defining a `QualityEventContext` union type that is extended explicitly rather than a free map. Version the event contract when new context fields are needed.

---

### [P2] Notification Contract: `SystemAnnouncement` Source Has No Scope Definition

**Location:** `ADR_centurisk-infrastructure.md` — Notification Contract

The `Notification.source` enum includes `SystemAnnouncement { announcement_id }`. No ADR defines the `SystemAnnouncement` contract, who creates announcements, what scope they are delivered to, or how they are managed. Given the stated system role as decision-support (not engagement engine), unbounded system announcements could undermine that design principle if used carelessly. Either define the announcement contract in the notifications section or explicitly note that `SystemAnnouncement` is reserved for future use and disabled in Phase 1.

---

### [P2] `AppraisalIntakeV1` Condition Assessment Uses Free-Text Notes

**Location:** `ADR_centurisk-io-adapters.md`

The `condition.notes: free text` field in `AppraisalIntakeV1` will be stored in the exposure core but is not indexed, not queryable through the NL layer, and not scored by the quality model. This is acceptable for Phase 1 but should be explicitly noted as a limitation: appraisal condition notes are dark data that cannot be queried or acted on until a structured condition field schema is defined.

---

### [P2] Delivery Sequence: Increment 13 (Custom Fields) Is Scheduled Too Late

**Location:** `ADR_centurisk-delivery-strategy.md`

Custom fields are built in Increment 13, which is second-to-last. However, custom field definitions affect: completeness scoring (Increment 3), accuracy rules (Increment 3), SOV pipeline validation (Increment 4), renewal workflow (Increment 7), NL query indexing (Increment 9), and bulk import mapping (Increment 11). Every one of those increments will need to be tested and hardened again after custom fields are added. This is not a blocking issue if the increments are designed with the custom field extension point in place from the start (which the composition model supports), but it is a risk that should be explicitly acknowledged in the delivery ADR. The custom field extension point should be stubbed in from Increment 2 even if the configuration UI ships in Increment 13.

---

## Positive Observations

**Strong boundary contract discipline.** The ADRs name contracts explicitly (`SOVProcessingResult`, `AppraisalIntakeV1`, `AuthenticatedUser`, `QualityEvent`, `Recommendation`, `ExternalServiceResult<T>`). This is not universal in architectural documentation and reflects disciplined thinking about module interfaces.

**Correct principle for Cedar placement.** The decision to make Cedar an adapter concern rather than a core concern is architecturally correct and well-argued. The consequence that the core is authorization-agnostic makes it genuinely portable and testable.

**Explicit ML seam in the recommendation engine.** The strategy pattern with a stable `Recommendation` output contract, and the explicit note that the ML replacement path requires only a backend swap, is exactly the right way to handle this kind of anticipated future evolution.

**Deferred items carry integration contracts.** The eight deferred capabilities each define a typed boundary contract (`ValuationEstimate`, `PolicyLink`, `Claim`, etc.). This is rare and valuable — most architectural deferrals leave future integration to future architects. Having these contracts in Phase 1 ADRs means they can be reviewed and critiqued before they become commitments.

**Technical spikes are first-class delivery items.** Placing three performance spikes as Increment 0 with a hard gate before proceeding reflects appropriate risk management for novel technical challenges.

**Progressive enhancement is specified operationally.** The `ExternalServiceResult<T>` contract with `Available / Degraded / Unavailable` states, combined with the enhancement tier table, is a concrete and testable degradation model — not just a vague "it should degrade gracefully" statement.

---

## Questions for Clarification

1. **Expression language scope:** Is the intent to share one expression language between `ResolutionRule.condition` and `AccuracyRule.condition`/`AccuracyRule.assertion`, or are these intentionally separate engines? If separate, what prevents semantic drift?

2. **Historical key grant duration:** Is the "time-bounded historical access grant" for migrating members intended to be a fixed regulatory default (e.g., 7 years for public records), or fully configurable per pool? If configurable, who sets it and what is the default?

3. **Aggregate invalidation during bulk import:** Is the intent to suppress pool-level aggregate recalculation during bulk import and trigger it once at completion, or should aggregates update continuously? This has implications for the quality dashboard experience during a live import.

4. **Cedar enforcement in Increments 2-4:** Is the intention that these increments use a permissive stub Cedar policy (allow everything) that is replaced in Increment 5, or is the Cedar layer simply absent? The distinction matters for whether the Increment 5 work is a wiring task or a security audit.

5. **`FieldValue` typing:** The asset registry uses `field_type: FieldType (String | Number | Date | Boolean | Enum)` in `CustomFieldDefinition`, but `Value` is unqualified throughout the mutation and resolution contracts. Is `Value` intended to be the same `FieldType` union, and if so, should that be stated explicitly in the asset registry ADR?

---

## Refactoring Roadmap

### Immediate (Before Committing to Increment 2)

1. Resolve the Cedar enforcement sequence question (P0): either pull Cedar to Increment 2 as a permissive stub that is hardened in Increment 5, or document the explicit audit requirement.
2. Define `FieldValue` as a typed discriminated union and replace `Any` in `FieldChange` and `Value` in `ResolvedFieldValue` (P0).
3. Specify the expression language for `ResolutionRule.condition` and `AccuracyRule.condition`/`AccuracyRule.assertion` (P1) — this is a shared dependency that blocks both the asset registry and quality model implementations.

### Before Spike 1 Completes

4. Define the spike output contract more precisely: require the spike to specify whether a hybrid snapshot model is needed, and require that decision to be incorporated into the asset registry ADR before Increment 2 begins (P1).
5. Document the aggregate invalidation strategy in the data quality ADR (P1).

### Before Increment 5 (Authentication / Multi-Tenancy)

6. Add `historical_access_scope` to `EncryptionKeyGrant` and document the regulatory dependency (P1).
7. Add idempotency key and deduplication strategy to `BulkImportJob` (P1).

### Short Term (Before Member-Facing Increments)

8. Scope the confidence threshold to per-pool configuration and document the Cedar-constrained suggestion edge case (P1).
9. Replace `QualityEvent.metadata: Map<String, Value>` with a typed context union (P2).
10. Define or explicitly defer `SystemAnnouncement` scope in the notification contract (P2).
11. Explicitly stub custom field extension points from Increment 2 even though the configuration UI ships in Increment 13 (P2).
