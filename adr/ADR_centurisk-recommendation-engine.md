# ADR: CentuRisk Recommendation Engine

## Status
Proposed

## Context

CentuRisk needs a recommendation engine that evaluates member exposure profiles and produces prioritized loss prevention suggestions. The engine must serve two audiences — pool administrators who manage risk across members and members who consume recommendations for their own portfolios — through the same output contract rendered by different adapters.

Several constraints shape this decision:

1. **Phase 1 is rule-based.** CentuRisk does not yet have the statistical foundation (sufficient loss event history, validated feature sets) to justify an ML approach. Rules are authored from domain expertise.
2. **ML is an explicit future requirement.** The architecture must not paint itself into a corner — replacing the rule engine with a statistical model should be a backend swap, invisible to downstream consumers.
3. **Rule logic is intellectual property.** The way CentuRisk decides what to recommend is a competitive differentiator. Members and pool administrators see the output (action, rationale) but never the rules or weights behind it.
4. **Loss events will eventually feed the engine.** Loss event data needs to be collected now so historical data is available retroactively when the ML engine is built, even though it does not influence Phase 1 recommendations.
5. **Impact estimation is uncertain.** CentuRisk cannot yet quantify the expected quality improvement of a recommendation — only express qualitative likelihood ("could improve" vs "will improve").

## Decision

### Engine Architecture: Swappable Strategy Behind a Stable Interface

The recommendation engine is implemented as a **strategy pattern** behind a stable output contract. Phase 1 ships a configurable rule engine that matches asset attributes against a curated rule set. The strategy interface is the only contract downstream consumers depend on — replacing the rule engine with an ML model in a future phase does not affect adapters, views, or API consumers.

The engine boundary is:

```
Recommendation Engine → Adapters
```

The engine produces; adapters render per audience scope (member sees their own, pool admin sees across members).

### Output Contract

```
Recommendation {
  asset_id?:               UUID | null       -- null for portfolio-level recommendations
  category:                String             -- e.g., "structural", "fire_protection", "valuation"
  priority:                Enum               -- high | moderate | low
  action:                  String             -- human-readable directive
  rationale:               String             -- explanation of why this recommendation applies
  expected_quality_impact: QualityImpact      -- qualitative in Phase 1
}
```

The `asset_id` is optional because some recommendations apply at the portfolio level (e.g., "complete construction type for all assets missing this field") rather than to a specific asset.

### Expected Quality Impact: Qualitative Indicator in Phase 1

```
QualityImpact (Phase 1):  Enum { high, moderate, low }
QualityImpact (Future):   Numeric(0.0..1.0) | Enum   -- upgradeable
```

In Phase 1, `expected_quality_impact` is a qualitative enum — `high`, `moderate`, or `low` — expressing possibility, not certainty. UI language should reflect this: "could improve data quality" rather than "will improve by X%." The field is modeled as a type that can be widened to a numeric value when the ML engine produces computed impact predictions. The enum is the floor, not the ceiling.

### Rule Authorship: CentuRisk Only, Opaque to Users

Recommendation rules are authored exclusively by CentuRisk. The rule logic is opaque to both pool administrators and members. They see the recommendation output (category, action, rationale) but never the rules, thresholds, or weights that produced it. This follows the same access pattern as quality scoring rules and temporal resolution strategies — CentuRisk controls the logic; users consume the results.

Rule management is an internal CentuRisk administrative function, not exposed through the member or pool admin adapters.

### Loss Event Intake: Collected Now, Consumed Later

```
LossEvent {
  asset_id:           UUID               -- the asset this event pertains to
  event_type:         String             -- e.g., "fire", "water_damage", "wind"
  date:               Date               -- when the event occurred
  severity_estimate:  String             -- qualitative severity
  description:        String             -- free-text event description
}
```

The boundary is:

```
Loss Event Intake → Recommendation Engine (future)
```

Loss events are collected and stored in Phase 1 via a structured intake form. They do **not** feed the recommendation engine until the statistical/ML engine replaces the rule-based approach. The intake contract is defined now so that:

- Historical loss data accumulates from day one.
- No schema migration is needed when the ML engine is ready to consume loss events.
- The intake UX can be validated and iterated independently of engine integration.

### ML-Ready Architecture

The engine replacement path is explicit:

1. The `Recommendation` output contract is immutable across engine versions. Adding fields is additive; removing or changing semantics requires a new contract version.
2. The engine is injected as a strategy — the dispatch layer calls `engine.evaluate(asset_context) -> Vec<Recommendation>` without knowing whether `engine` is rule-based or ML-based.
3. Loss event data, quality scores, and asset attributes are stored in a form consumable by both rule evaluation and future feature extraction.
4. No adapter, view, or API consumer references engine internals. They depend only on the `Recommendation` schema.

## Alternatives Considered

### Ship ML from Phase 1

Rejected. Insufficient loss event history, unvalidated feature sets, and unclear impact quantification make an ML approach premature. Building ML infrastructure before the data exists would be speculative investment with no user-facing value. The rule engine delivers recommendations immediately with available domain expertise.

### Expose Rules to Pool Administrators

Rejected. CentuRisk identified rule logic as intellectual property and a competitive differentiator. Exposing rules would also create a support burden — administrators modifying rules could produce conflicting or low-quality recommendations. CentuRisk retains control; users see outputs.

### Compute expected_quality_impact as a Score in Phase 1

Rejected. CentuRisk acknowledged they do not yet know how to calculate impact quantitatively. Presenting computed scores without a validated model would mislead users. A qualitative indicator ("could improve") is honest and sufficient. The schema accommodates future numeric values without breaking changes.

### Skip Loss Event Collection Until ML Phase

Rejected. Retroactive data collection is impossible — loss events that happen between now and the ML phase would be lost. Collecting now with a simple intake form is low-cost and builds the historical dataset the ML engine will need.

## Consequences

**Positive:**
- Recommendations are available to members from Phase 1 using domain expertise encoded as rules.
- The stable output contract means adapters, dashboards, and notification integrations never need to change when the engine evolves.
- Loss event history accumulates from day one, reducing the cold-start problem for the ML engine.
- Rule opacity protects CentuRisk's intellectual property without limiting user value.

**Negative / Trade-offs:**
- Rule-based recommendations require manual curation by CentuRisk — there is ongoing labor to author and maintain rules.
- Qualitative impact indicators provide less actionable prioritization than computed scores would. Members must rely on CentuRisk's judgment rather than data-driven impact estimates.
- Loss events are collected but provide no engine value in Phase 1 — the intake form is a forward investment, not an immediate feature.

**New constraints:**
- The `Recommendation` output contract is a stability boundary. Changes to it require versioning and adapter coordination.
- CentuRisk must staff rule authorship as an operational responsibility, not a one-time setup.
- The strategy interface must be designed before the first rule is written — it governs how both rule and ML engines are invoked.

## Implementation Plan

1. **Define the engine strategy interface and Recommendation output contract.** Implement the trait/interface that both rule-based and future ML engines will satisfy. Write contract tests that validate any engine implementation produces conformant output. This is testable immediately — a no-op engine returning empty results satisfies the interface.

2. **Build the rule engine with a minimal rule set (2-3 rules).** Implement the Phase 1 rule engine matching asset attributes to recommendations. Start with a small rule set (e.g., "missing construction type," "valuation older than 3 years," "no square footage recorded"). Validate that the engine produces correct `Recommendation` output for known asset profiles. This is runnable — feed it test asset data and inspect output.

3. **Build the Loss Event intake form and persistence.** Implement the `LossEvent` schema and intake endpoint. Store events associated with assets. No engine integration — just capture and persist. This is testable as a standalone CRUD operation and usable by members immediately.

4. **Wire the engine output to member-facing and pool admin adapters.** Connect the recommendation engine to the loss prevention views adapter (member) and the pool admin dashboard. Members see prioritized recommendations for their portfolio; pool admins see recommendations across members. This delivers the first end-to-end user-facing value from the engine.

5. **Expand the rule set based on CentuRisk domain expertise.** Add additional rules covering more asset dimensions and risk categories. Each rule addition is independently deployable and immediately visible in recommendation output. Iterate based on CentuRisk feedback on recommendation quality and coverage.

6. **Validate the ML-readiness seam.** Write an integration test that swaps the rule engine for a mock ML engine and confirms downstream adapters render correctly without changes. This validates the strategy pattern works as designed and documents the replacement path for the future ML phase.
