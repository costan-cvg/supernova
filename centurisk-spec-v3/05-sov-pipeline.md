# 5. The SOV Pipeline and Approval Workflow

The SOV pipeline is deterministic processing; the approval workflow is stateful human decision-making. They are separated by a clear contract: `SOVProcessingResult { validated_assets[], diff_summary, quality_assessment, errors[], source }`. The pipeline validates input, diffs it against current state, and scores quality. The source discriminator records what triggered each processing result — renewal, member inline edit, onboarding, bulk import — and this metadata is persisted and queryable. Reviewers can filter by source; administrators can audit by channel; the system reports on patterns.

## Approval Routing

Approval routing evaluates three inputs: the type of change, the user's profile, and (for valuations) the approver's permissions.

New asset creation and valuation changes always require approval, regardless of user profile. Valuations have their own permission layer — specific roles or users authorized to approve pending valuations.

Other changes to active assets (edits and deactivation) follow the user's auto-approve setting. A user with auto-approve enabled makes changes that take effect immediately. The same change by a user without auto-approve enters pending state for administrator approval.

The pipeline serves multiple adapters — renewal, onboarding, inline edits, bulk import — through the same contract. The core does not know which adapter produced the data.

## Interaction with Asset-Level Locking

Before the approval workflow accepts a submission, it checks the asset's lock state. If the asset already has a pending change, the submission is rejected and the submitting user is notified. This check happens after the pipeline processes the input but before the approval routing creates a pending record. The sequence is: pipeline validates and diffs → lock check → if unlocked, the change enters pending state and the asset locks → approval routing determines whether the change requires review or auto-approves. Auto-approved changes acquire and release the lock in a single transaction — the asset is never visibly locked to other users. Changes that require review hold the lock until an approver acts.
