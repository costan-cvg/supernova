# 04. Data Quality Model (Resolved)

The Data Quality Model scores assets across three dimensions: completeness, accuracy, and recency. Three decisions were resolved covering the data sources for scoring rules, notification threshold ownership, and recency granularity.

## 1. Scoring Rules: Asset Registry Data Only, Centurisk-Authored

**Question:** What does the accuracy dimension check in Phase 1 without the Valuation Estimator? Where does the reference data come from?

**Answer:** The data quality model uses information that is in the asset registry. There are a set of rules that define how the attributes of an asset impact the data quality score. The rules are defined by Centurisk admins. The scoring can be individualized per pool but Centurisk determines what the scoring configuration is.

**Decision:** The accuracy dimension works entirely from asset registry attributes — no external reference data needed in Phase 1. Rules cross-reference attributes already on the asset record (e.g., if construction class is frame and occupancy is habitational, then sprinkler field must be present). Rules are authored by Centurisk admins, customizable per pool, and not exposed to pool administrators.

**Spec Implications:**

- The rule engine evaluates asset attributes against Centurisk-defined rules. No external data sources or API calls are needed for scoring.
- This follows the same pattern as temporal resolution strategies: Centurisk configures, the core evaluates, the pool sees results without touching the rules.
- When the Valuation Estimator arrives in Phase 3, it adds a new data source that accuracy rules can reference. The rule engine itself does not change — the estimator's output becomes another attribute available to rules.

## 2. Notification Thresholds: Pool-Administrator-Configurable

**Question:** Are quality event thresholds Centurisk-configured per pool like the scoring rules, or can pool administrators tune them?

**Answer:** Pool admins should be able to adjust the notification thresholds.

**Decision:** Scoring rules (how quality is measured) are Centurisk-controlled. Notification thresholds (what score levels trigger alerts) are pool-administrator-configurable. This separates measurement logic from alerting sensitivity. One pool might alert at 80% completeness, another at 60%.

**Spec Implications:**

- The QualityEvent fires when a score crosses a threshold the pool admin has set. The threshold configuration UI is part of the pool administrator's Phase 1 experience.
- The pool admin needs a simple interface to set thresholds per quality dimension (completeness, accuracy, recency) and possibly per hierarchy level or asset type.

## 3. Recency: Field-Scoped, Centurisk-Designated

**Question:** Does recency reset when any field changes, or does it require explicit confirmation? Could a minor field update mask stale valuation data?

**Answer:** Centurisk can determine the fields that apply to recency.

**Decision:** Recency is evaluated per field, not per asset. Centurisk configures which specific fields are subject to recency tracking for each pool. A field's recency clock resets only when that specific field receives a new mutation. Updating unrelated fields on the same asset does not affect recency for tracked fields.

**Spec Implications:**

- Recency composes directly with the field-level mutation store from the Asset Registry temporal model. Every mutation already carries a timestamp — the recency dimension evaluates "for each designated field, how long since the last mutation?"
- No additional tracking infrastructure is needed beyond the existing mutation log.
- The recency field set is part of the per-pool scoring configuration that Centurisk admins manage.
