# ADR: CentuRisk Input/Output Adapters

## Status
Proposed

## Context

CentuRisk RMIS sits at the boundary between messy real-world data (member spreadsheets, appraisal reports, historical CSV archives) and a clean internal exposure model. Data enters the system from multiple sources in multiple formats, and data leaves the system for CAT modeling, SOV submissions, and broker packages. The core principle is: **format-specific logic always lives in the adapter, never in the core.**

Key constraints:
- Onboarding must handle both individual member submissions (interactive) and historical data migration at pool onboarding (batch, potentially 60+ years of history, tens of millions of assets).
- A separate upstream process prepares raw member data into supported formats before it reaches the onboarding adapter. The adapter does NOT parse arbitrary spreadsheet layouts, merged cells, or unstructured formats.
- Appraisal data comes from multiple providers (CentuRisk internal, third-party, member-uploaded) but Phase 1 supports only one CentuRisk-defined intake format.
- Phase 1 does not ship with format-specific CAT model export adapters (RMS, AIR, etc.). The focus is data readiness — having exposure data complete enough to support any future format.
- SOV generation must produce submission-ready documents from live data in formats brokers and underwriters expect.
- All adapters interact with the exposure core through defined boundary contracts. The core is format-agnostic.

## Decision

Implement four adapter boundaries with clearly defined contracts, each handling a specific data flow direction. Input adapters transform external data to core schema; output adapters transform core data to external formats.

### Boundary Contracts

From the boundary summary, these are the defined contracts:

| Boundary | Contract | Direction |
|----------|----------|-----------|
| Onboarding Adapter -> Exposure Core | Validated asset data conforming to SOV schema | Adapter transforms and validates; core stores |
| Bulk Import Pipeline -> Exposure Core | Same SOV schema, batch-processed with step function stages | Pipeline processes in resumable stages; core stores |
| Appraisal Intake -> Exposure Core | Structured appraisal result mapped to target asset (`AppraisalIntakeV1`) | Adapter validates; core updates valuation and quality |
| Exposure Core -> CAT/SOV Export Adapters | Asset data filtered by scope, validated for completeness | Core provides; adapter maps to target format |

All input adapters produce data conforming to the exposure core's schema. All output adapters consume data from the exposure core's schema. The core never adapts to an external format.

### 1. Onboarding Adapter — Two Modes, One Destination

The onboarding adapter serves two distinct use cases that share the same destination contract (validated asset data conforming to SOV schema).

#### Interactive Guided Flow

For individual members submitting data for the first time or updating their exposure data.

- **Input:** Pre-structured data in a supported format. A separate process outside the system boundary has already transformed raw member data (whatever spreadsheets, PDFs, or emails the member provided) into the adapter's expected format.
- **Processing:** The adapter validates the data against the SOV schema, maps fields to the exposure model, and presents the member with a guided flow for review, correction, and confirmation.
- **Output:** Validated asset data submitted to the exposure core.

The adapter does NOT handle format detection, merged cell parsing, or layout inference. That complexity lives upstream, outside the system boundary.

#### Batch Import Pipeline

For historical data migration when a new pool onboards. This is a step function flow designed for scale (tens of millions of assets spanning decades of history).

**Pipeline Stages:**

```
Stage 1: Upload
  Input: CSV or Excel files
  Output: Raw file stored, metadata recorded
  Resumable: Yes — re-upload replaces file

Stage 2: Parse
  Input: Raw file
  Output: Parsed rows with column mapping
  Resumable: Yes — re-parse from stored file

Stage 3: Validate Schema
  Input: Parsed rows
  Output: Rows with validation results (pass/fail per field)
  Resumable: Yes — re-validate from parsed data

Stage 4: Map to Exposure Model
  Input: Validated rows
  Output: Candidate asset records conforming to SOV schema
  Resumable: Yes — re-map from validated data

Stage 5: Assign/Match Asset IDs
  Input: Candidate asset records
  Output: Records with assigned asset IDs (new) or matched IDs (existing)
  Resumable: Yes — re-match from mapped data

Stage 6: Quality Score
  Input: Asset records with IDs
  Output: Records with quality scores per dimension
  Resumable: Yes — re-score from ID-matched data

Stage 7: Import Summary
  Input: Scored asset records
  Output: Summary report for human review (counts, quality distribution, errors, warnings)
  Resumable: Yes — regenerate summary from scored data
```

**Key properties:**
- Each stage is independently resumable. If processing fails at stage 4, you fix the issue and resume from stage 4 without re-running stages 1-3.
- At target scale, stages process in batches. Stage 2 can parse in chunks of 10,000 rows rather than loading the entire file into memory.
- The pipeline is greenfield — no existing ETL system is being replaced.
- The architecture does not preclude adding a persistent sync adapter for ongoing external integration in a future phase, but Phase 1 scope is one-time import.

**Import summary contract:**

```
ImportSummary {
  pipeline_id:        unique identifier for this import run
  pool_id:            target pool
  source_file:        original filename and metadata
  total_rows:         count of rows in source
  parsed_rows:        count successfully parsed
  validated_rows:     count passing schema validation
  mapped_assets:      count successfully mapped to exposure model
  new_assets:         count of newly created asset records
  matched_assets:     count matched to existing asset records
  quality_scores:     distribution summary (by dimension, by score range)
  errors:             list of errors with stage, row reference, and description
  warnings:           list of warnings (non-blocking issues)
  stage_durations:    time taken per stage
  status:             "complete" | "partial" | "failed" (with failed stage)
}
```

### 2. Appraisal Intake — Single CentuRisk-Defined Format

Phase 1 supports a single appraisal intake format: `AppraisalIntakeV1`.

**Design principles:**
- CentuRisk defines the schema. All providers map to it — whether CentuRisk's own appraisal services, third-party providers, or member-uploaded appraisals.
- External mapping happens outside the system boundary. If a third-party appraiser produces reports in their own format, someone (CentuRisk staff, an integration script, the provider themselves) maps it to `AppraisalIntakeV1` before submission.
- The adapter validates the incoming data, matches it to the target asset, and submits to the exposure core. The core updates valuation data and recalculates quality scores.

**Schema versioning:**
- When the appraisal schema needs to evolve, a new version (`AppraisalIntakeV2`) is created.
- The adapter supports the current version and the previous version simultaneously during a transition period.
- The core is stable — schema versions are an adapter concern.
- There is no need for multiple parallel format adapters in Phase 1 (e.g., no separate adapter for each appraisal provider).

**`AppraisalIntakeV1` contract (adapter -> core):**

```
AppraisalIntakeV1 {
  asset_id:              target asset identifier
  appraisal_date:        date of appraisal
  appraiser:             provider identifier
  source_type:           "centurisk" | "third_party" | "member_upload"
  valuation: {
    replacement_cost:    decimal
    actual_cash_value:   decimal (optional)
    functional_replacement_cost: decimal (optional)
    methodology:         string describing valuation approach
  }
  condition: {
    overall_rating:      enum (excellent | good | fair | poor)
    notes:               free text
    components:          list of component-level assessments (optional)
  }
  recommendations:       list of appraiser recommendations (optional)
  attachments:           list of file references (photos, reports)
  metadata: {
    format_version:      "V1"
    submission_timestamp: datetime
    submitter_user_id:   who submitted this into the system
  }
}
```

### 3. CAT Export — Data Readiness, Not Format-Specific Adapters

Phase 1 focuses on ensuring exposure data is **complete, validated, and structured enough** to support any CAT model format. Format-specific adapters (RMS, AIR/Verisk, etc.) are added on demand as pools or brokers request them.

**Pre-flight validation:**
- Before any export, a configurable pre-flight validation checks data completeness against a set of required fields.
- Field sets are configurable — different export targets require different fields. These are not tied to any particular external standard in Phase 1.
- Validation results indicate which assets are export-ready and which have gaps, with specific field-level detail.

**Pre-flight validation contract:**

```
ExportPreflightRequest {
  scope:               pool, member, or asset set to validate
  field_set:           named field set defining required fields for this export
  include_warnings:    boolean (include non-critical gaps)
}

ExportPreflightResult {
  total_assets:        count in scope
  ready_assets:        count meeting all required fields
  gap_assets:          count with missing required fields
  gaps: [
    {
      asset_id:        identifier
      missing_fields:  list of required fields not populated
      quality_score:   current quality score
    }
  ]
  readiness_percentage: float (ready_assets / total_assets)
}
```

**Architecture for future format adapters:**
- When a specific CAT format adapter is needed, it is added as an output adapter that consumes exposure core data (already validated) and maps it to the target schema.
- The core does not change. The adapter contains all format-specific logic: field mappings, enumerations, value transformations, output file format.
- Pre-flight validation field sets can be configured to match the target CAT model's requirements, providing early warning of data gaps before the export adapter runs.

### 4. SOV Generation — Submission-Ready Documents

SOV generation produces submission-ready documents from live exposure data. These are output adapters that transform core data into formats expected by brokers and underwriters.

**Supported formats (Phase 1):**
- **Excel** — configurable columns, configurable column ordering, configurable filters. Produces a workbook with asset data matching the broker's expected layout.
- **PDF** — formatted summary document suitable for submission packages.

**Broker-specific templates:**
- Common format requirements (which columns, which order, which filters, which summary sections) can be saved as named templates.
- Templates are reusable — once a broker's format requirements are captured, future SOV generations for that broker use the same template.
- Templates are a convenience layer. Every generation can also be configured ad-hoc.

**SOV generation flow:**

```
1. Select scope (pool, member subset, asset filters)
2. Select format (Excel or PDF) and template (or configure ad-hoc)
3. System runs pre-flight validation against the template's field set
4. If ready: generate document from live data
5. If gaps exist: present gap report, allow user to proceed with gaps noted or fix first
6. Document available for download and/or attachment to submission workflow
```

## Alternatives Considered

### Universal Format Adapter (Parse Anything)
Building an adapter that can handle any spreadsheet format, detecting layouts, parsing merged cells, and inferring column mappings. Rejected because: this is an unbounded problem, the accuracy of automated format detection is unreliable for the diversity of member spreadsheets, and the cost of building and maintaining such a system exceeds the cost of a separate upstream data preparation process.

### Multiple Appraisal Format Adapters in Phase 1
Building format-specific adapters for each known appraisal provider. Rejected because: the number of providers is small and variable, requiring each to map to a single CentuRisk-defined format shifts complexity to the provider (where it belongs), and adding a new provider does not require a system change.

### CAT Model Export Adapters in Phase 1
Shipping with RMS and AIR export adapters from day one. Rejected because: the specific schema versions and field mappings are pool-dependent and broker-dependent, building to a specific schema version locks the system to that version, and ensuring data readiness (the harder problem) provides more value than format-specific exports. Adapters are straightforward to add once data readiness is achieved.

### Batch Import as Real-Time Streaming Pipeline
Building the historical import as a streaming pipeline rather than a step function. Rejected because: historical import is a one-time operation per pool, the data arrives as files (not streams), and the step function model with resumable stages is simpler to operate, debug, and monitor. A streaming architecture adds complexity without benefit for this use case.

### SOV Generation as Static Report Only
Generating SOVs as point-in-time static reports rather than from live data. Rejected because: exposure data changes continuously (appraisals, quality updates, member submissions), and generating from live data ensures the SOV always reflects the current state. Point-in-time snapshots are a feature of the generation (date-stamped) but the source is live.

## Consequences

**Positive:**
- Clean boundary between format complexity (adapters) and domain logic (core) — the core never deals with CSV parsing, Excel formatting, or schema version differences.
- Resumable batch import pipeline handles the scale requirement (tens of millions of assets) without requiring the entire import to restart on failure.
- Single appraisal format reduces Phase 1 complexity dramatically while remaining extensible via versioning.
- Data readiness approach for CAT export provides value immediately (identifying gaps) while deferring the less-valuable format-specific work.
- Broker-specific SOV templates capture institutional knowledge about broker requirements for reuse.

**Negative / Trade-offs:**
- The "separate upstream process" for onboarding format preparation is outside the system boundary but essential for the system to work. If that process is poor, the adapter receives poor data. CentuRisk must invest in that upstream process even though it's not part of the platform.
- Single appraisal format means every provider must conform to CentuRisk's schema. Providers with significantly different data models face a mapping burden.
- No CAT export adapters in Phase 1 means pools cannot directly generate RMS/AIR files from the system — they must manually transform the exported data. This is acceptable if data readiness is the primary value proposition.
- Batch import pipeline has 7 stages, each requiring persistence and resumability. This is meaningful infrastructure to build and operate.

**New Constraints:**
- The exposure core's SOV schema is the single contract that all input adapters must produce. Changes to this schema affect every adapter.
- The batch import pipeline requires persistent storage for intermediate stage results (parsed rows, validated rows, etc.) to support resumability.
- Appraisal schema versioning requires the adapter to support at least two concurrent versions during transitions.
- SOV templates must be versioned — if a broker's format requirements change, old templates should be preserved for historical reference.
- Pre-flight validation field sets must be maintained as new export targets are added.

## Implementation Plan

1. **Onboarding interactive flow** — Implement the onboarding adapter for the interactive guided flow. Accept pre-structured data in a supported format, validate against SOV schema, map to exposure model, and submit to core. Test end-to-end: submit a valid member data file, verify it creates asset records in the core.

2. **Batch import pipeline (stages 1-3)** — Implement upload, parse, and schema validation stages with resumability. Each stage persists its output for the next stage. Test: upload a CSV, parse it, validate schema, verify each stage can be resumed independently after simulated failure.

3. **Batch import pipeline (stages 4-7)** — Implement exposure model mapping, asset ID assignment/matching, quality scoring, and import summary generation. Test: run the full pipeline on a representative historical dataset, verify the import summary accurately reflects the data, verify resumability at each stage.

4. **Appraisal intake adapter** — Implement the `AppraisalIntakeV1` adapter. Accept appraisal data in the defined format, validate it, match to target asset, and submit to core for valuation update and quality recalculation. Test: submit an appraisal, verify the asset's valuation data and quality scores update correctly.

5. **CAT export pre-flight validation** — Implement configurable field sets and pre-flight validation. Given a scope and field set, validate data completeness and produce a readiness report. Test: configure a field set, run pre-flight against a pool with mixed data completeness, verify the gap report is accurate.

6. **SOV generation (Excel)** — Implement the Excel SOV generator with configurable columns and filters. Support broker-specific templates. Test: generate an SOV for a pool using a configured template, verify the Excel output matches the expected layout and contains correct data from the core.

7. **SOV generation (PDF) and template management** — Implement PDF generation and the template CRUD interface for saving/loading broker-specific format configurations. Test: save a template, generate SOVs using it across multiple submissions, verify consistency.

8. **Batch import at scale** — Load test the batch import pipeline with target-scale data (millions of assets). Verify batch processing within stages works correctly, monitor memory usage and processing time, tune batch sizes. This is a hardening step, not new functionality.
