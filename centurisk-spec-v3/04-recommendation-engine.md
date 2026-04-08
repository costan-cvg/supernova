# 4. The Exposure Core — Recommendation Engine

The recommendation engine is rule-based in Phase 1 and designed to be replaced by a statistical or ML engine without changing its output contract. The engine evaluates asset attributes against CentuRisk-authored rules and produces recommendations with a stable schema: `Recommendation { asset_id?, category, priority, action, rationale, expected_quality_impact }`. The rule logic is CentuRisk's intellectual property — members and administrators see recommendations but never the rules that produced them.

## Expected Quality Impact

The expected_quality_impact field is a qualitative indicator in Phase 1 — typically "could improve" — not a computed score prediction. The schema is modeled to accommodate quantitative values when the ML engine is built.

## Loss Event Intake

Loss events are collected and stored in Phase 1 via an intake form (`LossEvent { asset_id, event_type, date, severity_estimate, description }`) but do not feed the recommendation engine until the statistical engine replaces the rule-based approach. The intake contract is defined now so historical loss data is available retroactively when needed.
