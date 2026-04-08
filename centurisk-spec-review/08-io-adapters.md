# 08. Input/Output Adapters (Resolved)

Input and output adapters handle the boundaries between external data sources and the exposure core. Three decisions were resolved covering onboarding format scope, appraisal intake, and CAT export targeting.

## 1. Onboarding Format Handling: Configuration, Not NLP/ML

**Question:** Does the onboarding adapter need to parse arbitrary spreadsheet layouts, or are there supported templates with a manual-mapping fallback?

**Answer:** There is a separate process for preparing input that fits the configuration problem approach.

**Decision:** Onboarding format handling is a configuration problem. A separate process outside the system prepares input data into a supported format before it reaches the onboarding adapter. The adapter receives pre-structured data and maps it to the SOV schema — it does not need to parse arbitrary spreadsheet layouts, merged cells, or unstructured formats. The messy transformation from raw member data into something the system can ingest happens upstream, outside the system boundary.

**Spec Implication:** The onboarding adapter's scope is reduced to: accept data in known supported formats, validate it, and map it to the exposure model. The external data preparation process is outside the system boundary and does not need to be specified in the platform spec.

## 2. Appraisal Intake: Single Centurisk-Defined Format

**Question:** How many appraisal provider formats are needed in Phase 1?

**Answer:** We only need to support one format. We have control over that definition. Any other integrations would be mapped to it.

**Decision:** Phase 1 supports a single appraisal intake format defined by Centurisk (`AppraisalIntakeV1`). All appraisal providers — Centurisk's own services, third parties, or member uploads — must deliver data in this format. Any provider-specific mapping happens outside the system boundary before data reaches the adapter. If the schema evolves, `AppraisalIntakeV2` handles the new version, but there is no need for multiple parallel format adapters in Phase 1.

## 3. CAT Export: Data Readiness, Not Format-Specific Adapters

**Question:** Which specific CAT model schema versions (RMS, AIR/Verisk) are targeted for Phase 1?

**Answer:** We collect this information but don't explicitly state we provide output in any particular standard schema. We should be able to accommodate any format that we have the data to support. This could be added later for specifically requested schemas.

**Decision:** Phase 1 does not ship with specific CAT model schema export adapters. The system collects and manages exposure data with sufficient completeness to support future format-specific exports. CAT export adapters are added later as pools or brokers request specific schemas. The pre-flight validation checks data completeness against configurable field sets, not tied to any particular external standard. The architecture (format-specific mapping isolated in adapters, core agnostic to CAT formats) already supports adding new export adapters without changing the core.
