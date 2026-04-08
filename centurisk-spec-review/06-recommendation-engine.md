# 06. Loss Prevention / Recommendation Engine (Resolved)

The recommendation engine generates prioritized suggestions for members based on their exposure profile. Four decisions were resolved covering the engine architecture, rule authorship, impact modeling, and loss event integration.

## 1. Engine Architecture: Rule-Based in Phase 1, ML-Ready

**Question:** Are recommendations rule-based, ML/statistical, or hybrid?

**Answer:** For Phase 1 it will be rule-based. In the future it will be statistical/ML.

**Decision:** Phase 1 ships a configurable rule engine that matches asset attributes to a curated set of recommendations. The output contract (`Recommendation { asset_id?, category, priority, action, rationale, expected_quality_impact }`) is stable regardless of whether a rule or a model produces it. The engine is a swappable strategy behind this interface — replacing the rule engine with an ML model in a future phase does not affect downstream consumers.

## 2. Rule Authorship: Centurisk Only, Opaque to Users

**Question:** Who authors the recommendation rules — Centurisk or pool administrators?

**Answer:** Rules are authored by Centurisk. The way loss prevention recommendations are decided is hidden from pool admins and members.

**Decision:** Recommendation rules are authored exclusively by Centurisk. The rule logic is opaque to both pool administrators and members — they see the output (the recommendation with its action and rationale) but not the rules that produced it. This is Centurisk's intellectual property and a competitive differentiator. This follows the same access pattern as quality scoring rules and temporal resolution strategies.

## 3. Expected Quality Impact: Qualitative, Not Computed

**Question:** How is expected_quality_impact calculated?

**Answer:** We don't know how to calculate the expected_quality_impact. We are making a statement that these actions "could" improve vs "definitely" will.

**Decision:** expected_quality_impact in Phase 1 is a qualitative indicator, not a computed score prediction. Recommendations express that an action could improve the member's data quality or risk position — language in the UI and the data model should reflect possibility, not certainty. The field should be modeled to accommodate a future upgrade to quantitative impact when the ML engine is built (e.g., as an enum like high/moderate/low in Phase 1, upgradeable to a numeric value later).

## 4. Loss Events: Collected but Not Incorporated in Phase 1

**Question:** Does the recommendation engine use loss event data in Phase 1?

**Answer:** It could be used later but for Phase 1 it will not be incorporated.

**Decision:** In Phase 1, loss events are collected and stored via the LossEvent intake form (`asset_id, event_type, date, severity_estimate, description`) but do not feed the recommendation engine. The engine incorporates loss event signals in a future phase when the statistical/ML engine replaces the rule-based approach. Since the intake contract is already defined, loss event history will be available retroactively when the integration is built.
