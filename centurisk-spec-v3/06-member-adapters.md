# 6. Member-Facing Adapters

The member experience is a set of adapters over the exposure core. Changing the UX does not require changing domain logic. Each adapter renders a specific view of the same underlying data.

## Exposure Self-Service

Exposure self-service provides interactive portfolio views: maps with geospatial overlays, tabular asset lists with filtering, drill-down from portfolio to individual asset, and total insured value accumulation analysis by geography, construction type, or custom dimensions. Members see their exposures clearly.

## Renewal Experience

The renewal experience pre-populates statements of value with proposed replacement costs. These originate from CoreLogic (Marshall & Swift) data, manually entered by CentuRisk admins as valuations in the asset registry — no external feed or computation required. Members respond in three ways: approve the proposed values, modify and submit, or flag for discussion. Flagging creates a queue item for the pool administrator with the member's note. Discussion happens outside the system. The flag carries open and resolved states for tracking. Bulk approval is available for "clean" items with no unresolved flags. Items with open flags require individual review.

## Coverage Views

Coverage views are read-only in Phase 1. Members view coverage details, generate Certificates of Insurance on demand, and see coverage differentials across policy periods. These differentials are field-level: members see exactly which fields changed on which assets between periods, not merely that an asset changed.

## Data Quality Dashboard

Data quality dashboard renders the quality model's output: composite scores, per-asset breakdowns, and specific gaps organized by dimension. It highlights which actions would have the largest impact.

## Loss Prevention Views

Loss prevention views render the recommendation engine's output: prioritized suggestions mapped to the member's specific exposure profile.
