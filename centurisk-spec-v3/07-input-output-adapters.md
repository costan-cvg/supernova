# 7. Input and Output Adapters

Input adapters transform external data into the exposure core's schema. Output adapters transform core data into external formats. Format-specific logic always lives in the adapter, never in the core.

## 7.1 Onboarding and Historical Import

The onboarding adapter has two distinct modes sharing the same destination contract.

The **interactive guided flow** serves individual members submitting data for the first time. Format handling is a configuration problem — a separate process outside the system prepares input into supported formats before reaching the adapter. The adapter receives pre-structured data and maps it to the SOV schema.

The **batch import pipeline** handles historical data migration at pool onboarding. Data arrives as CSV or Excel, potentially spanning 60+ years of history for tens of millions of assets. This is a step function flow: upload, parse, validate schema, map to exposure model, assign or match asset IDs, run quality scoring, produce import summary for review. Each stage is independently resumable. If processing fails at step 4, you fix the issue and resume from step 4. At target scale, stages process in batches. This pipeline is greenfield; no existing ETL exists. The architecture does not preclude adding a persistent sync adapter for ongoing external integration in a future phase.

## 7.2 Appraisal Intake

Phase 1 supports a single CentuRisk-defined format (`AppraisalIntakeV1`). All appraisal providers — internal, third-party, or member-uploaded — must deliver in this format. External mapping happens outside the system boundary. When the schema evolves, new versions (`AppraisalIntakeV2`, etc.) keep the core stable.

## 7.3 CAT Export and SOV Generation

Phase 1 does not ship with specific CAT model export adapters for RMS, AIR, or similar. The system ensures data readiness — exposure data is complete, validated, and structured enough to support any format. Format-specific adapters are added on demand as pools or brokers request specific schemas. Pre-flight validation checks completeness against configurable field sets, not tied to a particular external standard. SOV generation produces submission-ready documents from live data in configurable formats (Excel with configurable columns, PDF). Broker-specific templates save common format requirements for reuse.
