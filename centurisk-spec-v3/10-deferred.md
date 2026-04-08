# 10. What Is Explicitly Deferred

These capabilities are intentional exclusions, not oversights.

- **Valuation Estimator** (Phase 3, may pull into Phase 1 if capacity allows) — replacement cost estimation from asset attributes. The exposure core's asset model and valuation tracking are designed to receive estimator results when built.

- **Policy and Coverage Management** (Phase 4) — linking exposure to policies, endorsement workflows, premium calculations. Initial release provides read-only views. The boundary contract for future integration is defined in the asset model's segment linkage points.

- **Full Claims Lifecycle** (Phase 5) — reserve accounting, adjuster workflows, claim adjudication, payment tracking. Initial release includes only loss event intake.

- **Scheduled and Formatted Reporting** (Phase 5) — the full reporting engine. NL querying and built-in exposure views serve the most common needs; the full engine adds scheduled delivery and cross-module reports.

- **Premium "What-If" Modeling** — interactive modeling of premium impact. The boundary contract is defined: `PremiumFactorSource` is a versioned adapter initially returning static configuration.

- **Pool Health Analytics** — aggregate exposure analysis, concentration risk modeling, financial projections.

- **Full Communication Adapter** — structured campaigns, task workflows, survey distribution. Initial release uses lightweight notifications.

- **Member Team and Delegation Model** — multiple users within a member organization. The ABAC model accommodates this without schema changes.
