# 05. SOV Data Pipeline & Approval Workflow (Resolved)

The SOV Data Pipeline handles validation, diffing, and scoring of submissions. The Approval Workflow routes processed results to human review or auto-approval. Three decisions were resolved covering asset identity, source tracking, and the approval routing model.

## 1. Asset Identity: System-Generated Unique ID

**Question:** What is the asset identity key across SOV submissions? How are address changes or attribute mutations handled?

**Answer:** Centurisk has a system-generated unique ID for assets that is independent of pool or attributes.

**Decision:** Asset identity is a Centurisk system-generated unique ID. It does not depend on addresses, member-assigned labels, or any mutable attribute. The differential computation in the pipeline matches on this ID, making diffs deterministic and immune to attribute changes like address renumbering.

## 2. Source Discriminator: Recorded and Queryable

**Question:** Does the SOVProcessingResult contract need to carry a source indicator so downstream consumers know what triggered the processing?

**Answer:** Yes, this should all be recorded and queryable.

**Decision:** Every SOVProcessingResult records what triggered it — renewal submission, member inline edit, onboarding, bulk import, etc. This metadata is persisted and queryable, enabling reviewers to filter by source, administrators to audit activity by channel, and the system to report on submission patterns over time.

## 3. Approval Routing: Change-Type-Aware and User-Profile-Aware

**Question:** Does the approval workflow support auto-approval rules in Phase 1, or does every submission require human review?

**Answer:** The approval model is more nuanced than a simple toggle. It depends on both the type of change and the user's profile.

**Decision:** Approval routing evaluates three inputs: the type of change, the user's auto-approve profile setting, and (for valuations) the approver's permissions. The rules are:

**Always pending, regardless of user profile:**

- New asset creation (activation of a new asset).
- Valuation changes (entry or edit). Valuation approvals have their own permission layer — specific roles/users are authorized to approve pending valuations.

**Governed by user profile auto-approve setting:**

- Edits to existing active assets — auto-approve on: changes accepted immediately; auto-approve off: changes go to pending for admin approval.
- Deactivation of assets — auto-approve on: deactivation takes effect immediately; auto-approve off: creates a pending deactivation request.

**Spec Implications:**

- The user profile model must include an auto-approve flag as a configurable setting.
- The approval workflow needs a change-type classifier that runs before routing — it must distinguish new asset creation, valuation changes, attribute edits, and deactivation.
- Valuation approvals require a separate permission layer: not just "any admin" but specific authorized roles/users per pool.
- The SOVProcessingResult must carry enough context (change type, source, user profile) for the approval workflow to route correctly.
