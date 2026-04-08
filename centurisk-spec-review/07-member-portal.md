# 07. Member Portal / Self-Service (Resolved)

The Member Portal provides exposure self-service, renewal workflows, coverage views, and access to data quality and recommendation outputs. Four decisions were resolved covering renewal pre-population, flag handling, bulk approval, and coverage differentials.

## 1. Renewal Pre-Population: CoreLogic Data via Centurisk Admins

**Question:** Where do the proposed values for renewal pre-population (e.g., inflation-adjusted building values) come from?

**Answer:** Marshall & Swift, now called CoreLogic, provides the information. It is manually inputted by Centurisk admins. The valuations live in the asset registry and are made visible to the member.

**Decision:** Renewal pre-population values originate from CoreLogic (Marshall & Swift) data, manually entered by Centurisk admins as valuations in the asset registry. These follow the normal valuation approval path. No external data feed adapter or automated computation is needed — the input is a Centurisk admin action through the existing valuation intake. Members see these as proposed values during the renewal workflow.

## 2. Flag for Discussion: Queue Item, Out-of-System Resolution

**Question:** When a member flags an item during renewal, is there a structured in-system conversation or does discussion happen externally?

**Answer:** The flag just lands in the pool administrator's queue with the member's note, and the actual discussion happens outside the system (phone call, email).

**Decision:** Flag for Discussion creates a queue item for the pool administrator with the member's free-text note attached. The system does not need a messaging or thread feature. The flag needs: the asset/field being flagged, the member's note, a timestamp, and a resolution state (open/resolved) so the administrator can track which flags have been addressed. Resolution happens when the administrator marks it done, not when a reply is sent.

## 3. Bulk Approval: No Unresolved Flags = Clean

**Question:** What defines a "clean" item eligible for bulk approval during renewal?

**Answer:** No flags left unresolved by the member, or no flags at all, means clean.

**Decision:** An item is "clean" for bulk approval if it has no flags at all, or all flags have been resolved by the member. Any item with unresolved flags requires individual review and cannot be bulk-approved. This is a simple, defensible definition that prevents members from accidentally skipping items that need attention.

## 4. Coverage Differentials: Field-Level Across Policy Periods

**Question:** Are coverage differential views at the asset level or the field level?

**Answer:** Field level.

**Decision:** Coverage differential views show field-level comparisons across policy periods. Members see exactly which fields changed on which assets between period A and period B. This composes naturally with the field-level mutation store — the differential is a query comparing resolved field values at each period's effective date and surfacing the deltas.
