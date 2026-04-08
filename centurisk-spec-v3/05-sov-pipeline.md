# 5. The SOV Pipeline and Approval Workflow

The SOV pipeline is deterministic processing; the approval workflow is stateful human decision-making. They are separated by a clear contract: `SOVProcessingResult { validated_assets[], diff_summary, quality_assessment, errors[], source }`. The pipeline validates input, diffs it against current state, and scores quality. The source discriminator records what triggered each processing result — renewal, member inline edit, onboarding, bulk import — and this metadata is persisted and queryable. Reviewers can filter by source; administrators can audit by channel; the system reports on patterns.

## Approval Routing

Approval routing evaluates three inputs: the type of change, the user's profile, and (for valuations) the approver's permissions.

New asset creation and valuation changes always require approval, regardless of user profile. Valuations have their own permission layer — specific roles or users authorized to approve pending valuations.

Other changes to active assets (edits and deactivation) follow the user's auto-approve setting. A user with auto-approve enabled makes changes that take effect immediately. The same change by a user without auto-approve enters pending state for administrator approval.

The pipeline serves multiple adapters — renewal, onboarding, inline edits, bulk import — through the same contract. The core does not know which adapter produced the data.
