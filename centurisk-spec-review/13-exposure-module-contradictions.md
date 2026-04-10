# 13. Exposure Module Spec (v0.7) — Contradiction Resolution

Source: Exposure-Module-Spec_7.odt (April 2026, DRAFT)
Resolution Date: April 10, 2026

This document records how each contradiction or significant change between the Exposure Module Spec v0.7 and the existing reviewed spec was resolved.

## 1. Access Control Model: RBAC Rights as Cedar Aliases

**Contradiction:** The new spec defines a fixed RBAC rights table (View, Add, Edit, Deactivate, Export, Approve, Manage Exposure Types). The existing spec uses ABAC with Cedar Policy.

**Resolution:** Keep Cedar Policy as the authorization engine. The RBAC rights table from v0.7 becomes a convenience shorthand — each right maps to a named Cedar policy. "View" is an alias for a Cedar policy that grants read access scoped to the user's attributes. "Approve" is an alias for a Cedar policy that grants approval authority scoped by change type and pool. The rights table is a UI/administrative simplification, not a replacement for Cedar.

**Spec Impact:** No change to `10-access-control.md`. The v0.7 rights table is valid as a list of named profiles that map to Cedar policies. Implementation evaluates Cedar policies, not the rights directly.

## 2. Approval Workflow: Keep Change-Type-Aware Routing

**Contradiction:** The new spec describes approval as a binary "approval authority" flag. The existing spec routes by change type — new assets and valuations always pending, edits and deactivation follow auto-approve.

**Resolution:** Keep the existing change-type-aware routing. The v0.7 language is simplified for readability but the implementation follows the granular model: new asset creation and valuation changes always go to pending regardless of user profile. Edits and deactivation follow the auto-approve setting.

**Spec Impact:** No change to `05-sov-pipeline.md`. The v0.7 spec's Section 2.2 should be read as a simplified description of the existing routing model, not a replacement.

## 3. Pending Changes: Asset-Level Locking with Concurrency Control

**Contradiction:** The new spec states "only one pending change per record." The existing spec uses field-level mutations that could allow independent pending changes.

**Resolution:** When any property of an asset changes and enters pending state, the entire asset is locked from further changes until that pending change is approved or rejected. This applies universally:

- **Single-user flow:** User submits a change → asset locks → pending approval indicator appears everywhere the asset is displayed → no edit controls are available until resolution.
- **Concurrent edit flow:** Two users have the same asset open for editing. The first user to save locks the asset. The second user's save attempt is blocked. The system notifies the blocked user that pending changes exist and they cannot submit until the pending change is resolved.
- **Approval-authority user flow:** A user with approval authority opens an asset that has pending changes. Before they can submit their own changes, the system notifies them that pending approvals exist and must be resolved first.
- **Display behavior:** Wherever the asset appears — exposure list, search results, reports, member portal — the interface shows that changes are pending and the asset is locked for submissions.

This is an optimistic locking model: the lock is acquired at save time, not at edit-open time. Users can open and prepare edits freely; the constraint is enforced on submission.

**Spec Impact:** New section added to `02-asset-registry.md`. Interaction added to `05-sov-pipeline.md` approval routing.

## 4. Access Scoping: Cedar Policy Concern

**Contradiction:** The new spec assigns each user a fixed hierarchy scope during account setup. The existing spec defines scoping as a Cedar policy attribute with arbitrary granularity.

**Resolution:** Keep Cedar policy scoping. Hierarchy scoping from v0.7 Section 2.3 is one pattern expressible as a Cedar policy — not a separate mechanism. The v0.7 description of "user assigned to a level" is valid as a simplified explanation of what happens when a Cedar policy scopes a user to a hierarchy prefix.

**Spec Impact:** No change to `10-access-control.md`.

## 5. Effective Date View: Keep Future-Date Queries

**Contradiction:** The new spec restricts effective date view to past dates only. The existing spec supports four view modes including future-date impact analysis.

**Resolution:** Keep the existing four view modes. Future-date queries are valuable for impact analysis (what does the portfolio look like on a future effective date given pending changes?). The v0.7 restriction to past-only is not adopted.

**Spec Impact:** No change to `02-asset-registry.md` or `03-asset-registry.md`.

## 6. History Reconstruction: Forward Projection, Not Backward Replay

**Contradiction:** The new spec describes backward replay from current state. The existing spec uses forward projection from immutable facts.

**Resolution:** Keep forward projection as described in `02-asset-registry.md`. Current state is a projection over timestamped facts resolved by the pool's configured resolution strategy. The v0.7 "replay backwards" language is not adopted — backward replay is fragile if current state is corrupted or resolution strategies change. The spec leaves the reconstruction mechanism to the architecture team, with the constraint that the source of truth is the immutable mutation log, not the current record.

**Spec Impact:** No change to `02-asset-registry.md`.

## 7. User Model: Cedar Policies Cover the Rights Table

**Contradiction:** The new spec defines user roles with a fixed rights table. The existing spec defines four user categories with Cedar policies.

**Resolution:** The four-category Cedar model is authoritative. The v0.7 rights table (View, Add, Edit, Deactivate, Export, Approve, Manage Exposure Types) enumerates the actions that Cedar policies can grant or deny. Each right is a named profile. The existing user categories (CentuRisk users, pool admins, member users, view-only users) remain the user model. The rights table is a useful enumeration of permission actions, not a competing model.

**Spec Impact:** No change to `10-access-control.md`. The rights enumeration from v0.7 is noted as a useful reference for the set of actions Cedar policies need to cover.

## 8. Custom Exposure Types Storage: Data-Driven Extensions

**Contradiction:** The new spec flags storage architecture as an open question. The existing spec already decided on data-driven extensions.

**Resolution:** Keep the existing decision. Custom exposure types are data-driven extensions, not schema changes. The v0.7 open question (Section 8) is resolved: shared structure with type discrimination, not dedicated table per type.

**Spec Impact:** No change to `02-asset-registry.md`.

## 9. Policy Linkage: Deferred

**Contradiction:** The new spec introduces policy linkage as a new concept.

**Resolution:** Ignore for now. Policy linkage depends on a Policy module that does not yet exist. Exposure records should not be tightly coupled to policies or assets. When the Policy module is specified, linkage will be defined as an adapter concern at the boundary, not as a structural property of the exposure record.

**Spec Impact:** No changes.

## 10. Contacts on Exposure Records: Deferred

**Contradiction:** The new spec adds named contacts per exposure record.

**Resolution:** Ignore for now. Contact association can be added later without changing the core exposure model.

**Spec Impact:** No changes.
