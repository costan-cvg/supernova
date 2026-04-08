# 11. Boundary Summary

| Boundary | Contract | Direction |
|----------|----------|-----------|
| Onboarding Adapter → Exposure Core | Validated asset data conforming to SOV schema | Adapter transforms and validates; core stores |
| Bulk Import Pipeline → Exposure Core | Same SOV schema, batch-processed with step function stages | Pipeline processes in resumable stages; core stores |
| Appraisal Intake → Exposure Core | Structured appraisal result mapped to target asset (`AppraisalIntakeV1`) | Adapter validates; core updates valuation and quality |
| SOV Pipeline → SOV Approval Workflow | `SOVProcessingResult { validated_assets[], diff_summary, quality_assessment, errors[], source }` | Pipeline produces; workflow consumes and routes |
| Renewal Adapter → SOV Pipeline | Pre-populated SOV with proposed values and quality flags | Renewal orchestrates; pipeline validates and diffs |
| Exposure Core → Member/Pool Adapters | Exposure views, quality scores, recommendations | Core produces; adapters render with scope |
| Exposure Core → CAT/SOV Export Adapters | Asset data filtered by scope, validated for completeness | Core provides; adapter maps to target format |
| Data Quality Model → Notification System | `QualityEvent { entity_id, entity_type, dimension, current_score, threshold, direction }` | Model emits; notification adapter delivers |
| Recommendation Engine → Adapters | `Recommendation { asset_id?, category, priority, action, rationale, expected_quality_impact }` | Engine produces; adapters render per audience |
| NL Query → Exposure Views | Structured query derived from natural language input | Query adapter translates; view logic executes |
| Loss Event Intake → Recommendation Engine | `LossEvent { asset_id, event_type, date, severity_estimate, description }` | Intake captures; engine incorporates (future) |
| Cedar Policy Engine → All Adapters | ABAC policy evaluation per authenticated user context | Engine evaluates; adapters enforce restrictions |
