# ADR: CentuRisk SOV Pipeline and Approval Workflow

## Status
Proposed

## Context

The Statement of Values (SOV) pipeline is the central data ingestion path for CentuRisk. Every change to the exposure registry — whether from renewal submissions, member inline edits, onboarding, or bulk imports — flows through this pipeline. The pipeline must validate input, compute differences against current state, score data quality, and hand off results to an approval workflow that routes changes to human review or auto-approval based on business rules.

Several constraints shape this decision:

1. **Multiple input channels, single processing contract.** Renewal adapters, onboarding flows, inline edits, and bulk imports all produce SOV data. The pipeline must process all of them uniformly without knowing which adapter produced the data.
2. **Approval logic is business-critical and nuanced.** Not all changes are treated equally — new assets and valuations always require human approval, while edits and deactivations depend on the submitting user's auto-approve profile setting. Valuations have their own permission layer.
3. **Auditability is non-negotiable.** Public sector risk pools require traceability. Every submission must record its source, and reviewers must be able to filter and audit by submission channel.
4. **Asset identity is system-generated.** Diffs are computed against a CentuRisk system-generated unique ID that is independent of any mutable attribute (address, member label). This makes differential computation deterministic.
5. **The pipeline and approval workflow are distinct concerns.** The pipeline is deterministic processing (validate, diff, score). The approval workflow is stateful human decision-making (route, review, approve/reject). They must be separated by a clear contract.

## Decision

### Pipeline-to-Workflow Boundary

The SOV pipeline and approval workflow are separated by a clear contract. The pipeline produces; the workflow consumes and routes.

```
SOV Pipeline → SOV Approval Workflow
```

The contract between them:

```
SOVProcessingResult {
  validated_assets[]:   ValidatedAsset[]     -- assets that passed validation
  diff_summary:         DiffSummary          -- what changed relative to current state
  quality_assessment:   QualityAssessment    -- data quality scores for the submission
  errors[]:             ProcessingError[]    -- validation failures, parse errors, etc.
  source:               SourceDiscriminator  -- what triggered this processing
}
```

### Deterministic Pipeline: Validate, Diff, Score

The pipeline executes three deterministic stages in order:

1. **Validate.** Input data is validated against the SOV schema. Invalid records produce structured errors in the `errors[]` array. Valid records proceed.
2. **Diff against current state.** Validated assets are compared to their current state in the exposure registry using the system-generated asset ID. The diff identifies new assets, modified fields (with before/after values), deactivated assets, and unchanged assets. Diffs are deterministic — the same input against the same current state always produces the same diff.
3. **Score quality.** The quality model evaluates the submission and produces a quality assessment — completeness scores, dimension-level gaps, and overall quality indicators.

The pipeline does not make approval decisions. It produces a complete `SOVProcessingResult` and hands it to the approval workflow.

### Source Discriminator: Persisted and Queryable

```
SourceDiscriminator: Enum {
  renewal,          -- annual/periodic renewal submission
  inline_edit,      -- member editing an individual asset through the portal
  onboarding,       -- initial asset data load for a new member
  bulk_import       -- spreadsheet or file-based batch submission
}
```

Every `SOVProcessingResult` records what triggered it. This metadata is persisted alongside the processing result and is queryable. This enables:

- **Reviewers** to filter pending approvals by source (e.g., "show me only renewal submissions").
- **Administrators** to audit activity by channel (e.g., "how many inline edits vs. bulk imports this quarter").
- **System reporting** on submission patterns over time.

The source discriminator is extensible — new sources can be added as new adapters are built (e.g., API integrations) without changing the pipeline contract.

### Approval Routing: Three Inputs

The approval workflow evaluates three inputs to determine routing:

1. **Change type** — what kind of modification the submission represents.
2. **User profile** — specifically, whether the submitting user has the auto-approve flag enabled.
3. **Approver permissions** — for valuations, whether the reviewer is authorized to approve valuation changes.

This requires a **change-type classifier** that runs before routing. The classifier inspects the `diff_summary` and categorizes each change as one of:

- New asset creation (activation)
- Valuation change (entry or edit)
- Attribute edit (non-valuation field changes on existing assets)
- Asset deactivation

### Approval Rules

**Always pending, regardless of user profile:**

| Change Type | Routing | Rationale |
|---|---|---|
| New asset creation | Always pending → admin review | New exposures affect pool-level risk aggregation |
| Valuation changes | Always pending → valuation-authorized reviewer | Valuations directly affect premium calculations and require specialized approval |

Valuation approvals have their own permission layer. Not any administrator can approve a valuation — only specific roles or users authorized per pool. This is a separate permission check from general admin approval.

**Governed by user profile auto-approve setting:**

| Change Type | Auto-Approve ON | Auto-Approve OFF |
|---|---|---|
| Attribute edits (non-valuation) | Accepted immediately | Pending → admin approval |
| Asset deactivation | Effective immediately | Pending → admin approval |

### Pipeline Serves Multiple Adapters

The pipeline is adapter-agnostic. Each adapter (renewal, onboarding, inline edit, bulk import) is responsible for transforming its input into the SOV schema. Once data enters the pipeline, processing is uniform. The core does not know which adapter produced the data — it knows only the `source` discriminator for auditability.

```
Renewal Adapter ──────────┐
Onboarding Adapter ───────┤
Inline Edit Adapter ──────┼──→ SOV Pipeline → SOVProcessingResult → Approval Workflow
Bulk Import Adapter ──────┘
```

### Supporting Contracts

The `SOVProcessingResult` must carry enough context for the approval workflow to route correctly:

```
DiffSummary {
  new_assets[]:         AssetDiff[]        -- assets not in current registry
  modified_assets[]:    AssetDiff[]        -- assets with field-level changes
  deactivated_assets[]: AssetDiff[]        -- assets being removed/deactivated
  unchanged_assets[]:   UUID[]             -- assets present but not modified
}

AssetDiff {
  asset_id:             UUID               -- system-generated unique ID
  change_type:          ChangeType         -- new | modified | deactivated
  field_changes[]:      FieldChange[]      -- per-field before/after (for modified)
  has_valuation_change: Boolean            -- fast check for approval routing
}

FieldChange {
  field_name:           String
  previous_value:       Any | null         -- null for new assets
  proposed_value:       Any
  is_valuation_field:   Boolean            -- marks fields subject to valuation approval
}

ProcessingError {
  asset_reference:      String             -- identifier from the source (row number, label)
  field_name:           String | null
  error_type:           String             -- validation_failure, parse_error, etc.
  message:              String
}

SourceDiscriminator:    Enum { renewal, inline_edit, onboarding, bulk_import }
```

### User Profile Requirement

The user profile model must include:

- An `auto_approve` flag as a configurable setting per user.
- This flag is set by pool administrators, not by the members themselves.

## Alternatives Considered

### Single Approval Rule for All Change Types

Rejected. Treating all changes uniformly (e.g., "all changes require approval" or "all changes are auto-approved") does not match CentuRisk's business reality. New assets and valuations carry different risk weight than attribute edits. A uniform rule would either create unnecessary review burden (everything pending) or insufficient oversight (everything auto-approved). The change-type-aware routing reflects actual business risk tiers.

### Separate Pipelines per Source

Rejected. Building distinct pipelines for renewal, onboarding, inline edit, and bulk import would duplicate validation, diffing, and quality scoring logic. A single pipeline with a source discriminator achieves uniform processing while preserving auditability. Adapters handle source-specific transformation; the pipeline handles source-agnostic processing.

### Approval Workflow Embedded in the Pipeline

Rejected. The pipeline is deterministic processing; the approval workflow is stateful human decision-making. Embedding approval logic in the pipeline would couple processing correctness to approval state, making both harder to test and evolve. The `SOVProcessingResult` contract cleanly separates these concerns — the pipeline can be tested in isolation with deterministic assertions, and the approval workflow can be tested with state machine assertions.

### Auto-Approve Based on Change Magnitude Rather Than User Profile

Considered but rejected for Phase 1. A magnitude-based rule (e.g., "auto-approve changes under $50K") introduces threshold tuning and edge cases. CentuRisk's current model is simpler: trusted users get auto-approve; other users go through review. Magnitude-based rules could be layered on in a future phase as an additional routing criterion.

## Consequences

**Positive:**
- All SOV data flows through a single, testable, deterministic pipeline regardless of source.
- The `SOVProcessingResult` contract decouples pipeline processing from approval workflow state management, making both independently testable and evolvable.
- Source discriminators provide full auditability — every change is traceable to its origin channel.
- Approval routing is explicit and inspectable — the rules are documented and the three-input model (change type, user profile, approver permissions) covers the known business cases.
- The pipeline contract supports multiple adapters today and new adapters in the future without pipeline changes.

**Negative / Trade-offs:**
- The change-type classifier adds complexity to the approval workflow — it must correctly distinguish valuations from attribute edits, which requires knowing which fields are valuation fields.
- The `auto_approve` flag on user profiles creates an operational responsibility for pool administrators to manage user trust levels correctly.
- Valuation-specific permissions are a separate authorization layer that must be maintained alongside general role-based access control.

**New constraints:**
- The `SOVProcessingResult` is a stability boundary. Changes to it require coordination between pipeline and approval workflow.
- The field-level diff model must classify fields as valuation or non-valuation for approval routing to work correctly. This classification must be maintained as the asset schema evolves.
- Asset identity (system-generated UUID) must be assigned before pipeline processing — the diff stage depends on it for matching.

## Implementation Plan

1. **Define the SOVProcessingResult contract and supporting types.** Implement the `SOVProcessingResult`, `DiffSummary`, `AssetDiff`, `FieldChange`, `ProcessingError`, and `SourceDiscriminator` types. Write contract tests that validate the schema. This is testable immediately — the types can be instantiated and serialized without any pipeline logic.

2. **Build the validation stage with a single adapter (inline edit).** Implement SOV schema validation that accepts asset data and produces `validated_assets[]` and `errors[]`. Wire the inline edit adapter as the first source. This is the thinnest vertical slice — a member edits one field, the pipeline validates it, and a result is produced. Testable end-to-end with mock asset data.

3. **Build the diff stage against current asset state.** Implement the differential computation that compares validated assets to their current registry state using system-generated asset IDs. Produce `DiffSummary` with field-level changes, including `has_valuation_change` and `is_valuation_field` markers. Testable by seeding known current state and submitting known changes, then asserting the diff.

4. **Build the quality scoring stage.** Wire the data quality model to evaluate submissions and produce a `QualityAssessment` as part of the processing result. This completes the three-stage pipeline (validate, diff, score). Testable by verifying quality scores for submissions with known completeness characteristics.

5. **Build the approval workflow with change-type routing.** Implement the change-type classifier and approval routing rules. New assets and valuations go to pending. Attribute edits and deactivations check the user's auto-approve flag. Valuation approvals check the reviewer's valuation-approval permission. This is testable as a state machine — given a `SOVProcessingResult` and a user profile, assert the correct routing decision.

6. **Wire the renewal adapter to the pipeline.** Implement the renewal adapter that pre-populates SOV data with proposed values and submits through the same pipeline. Verify the source discriminator is set to `renewal` and the approval workflow routes correctly. This validates the adapter-agnostic pipeline design.

7. **Wire the onboarding and bulk import adapters.** Implement remaining adapters, each transforming their source-specific input into the SOV schema and submitting to the pipeline. Each adapter is independently testable and immediately visible — submit data, see it processed, see it routed to approval.

8. **Build the approval queue UI for pool administrators.** Render pending approvals with source filtering, change-type indicators, and approval/rejection actions. Support bulk approval for clean items. This delivers the end-to-end approval experience to pool administrators.
