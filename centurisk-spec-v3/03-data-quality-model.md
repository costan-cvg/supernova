# 3. The Exposure Core — Data Quality Model

The data quality model scores assets using only the data already stored in the asset registry. No external reference data is consulted in Phase 1. The model evaluates three dimensions. Completeness measures how many required and recommended fields are populated. Accuracy cross-references asset attributes against consistency rules (for example: if construction class is frame and occupancy is habitational, then a sprinkler field must be present). Recency evaluates how recently specific fields were updated. Scoring rules are authored by CentuRisk admins, customizable per pool, and not exposed to pool administrators. This follows the same pattern as temporal resolution: CentuRisk configures, the core evaluates, users see results.

## Field-Scoped Recency

Recency is field-scoped, not asset-scoped. CentuRisk configures which specific fields are tracked for recency per pool. Updating a contact name does not reset the recency clock on replacement cost. This composes directly with the field-level mutation store — each tracked field's recency is simply the time since its last mutation.

## Notification Thresholds

Notification thresholds — the score levels that trigger quality alerts — are the one piece pool administrators control. CentuRisk defines how quality is measured; the pool decides what scores matter to them. One pool might alert at 80% completeness; another at 60%.

## Quality Events

Quality events structure the primary observability mechanism. A QualityEvent carries entity_id, entity_type, dimension (completeness, accuracy, or recency), current_score, threshold, and direction. These structured signals emit at the boundary between scoring and action, available for downstream consumption.
