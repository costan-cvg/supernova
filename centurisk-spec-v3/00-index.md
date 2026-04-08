# CentuRisk RMIS — Platform Specification

**Value Proposition, Initial Scope, and Architecture Decisions**

v3 — April 8, 2026

## Document Structure

This specification is split into the following files, each covering a distinct module or concern. Files are numbered for reading order but are self-contained — any file can be referenced independently when generating development specs.

| File | Section | Summary |
|------|---------|---------|
| [01-governing-thought.md](01-governing-thought.md) | The Governing Thought | System role, flywheel, data ownership, validation status |
| [02-asset-registry.md](02-asset-registry.md) | Asset Registry | Identity, hierarchy, temporal model, custom fields, lifecycle |
| [03-data-quality-model.md](03-data-quality-model.md) | Data Quality Model | Completeness, accuracy, recency, scoring rules, thresholds |
| [04-recommendation-engine.md](04-recommendation-engine.md) | Recommendation Engine | Rule-based Phase 1, output contract, loss event intake |
| [05-sov-pipeline.md](05-sov-pipeline.md) | SOV Pipeline and Approval Workflow | Processing contract, source tracking, approval routing |
| [06-member-adapters.md](06-member-adapters.md) | Member-Facing Adapters | Self-service, renewal, coverage, quality dashboard, loss prevention |
| [07-input-output-adapters.md](07-input-output-adapters.md) | Input and Output Adapters | Onboarding, bulk import, appraisal intake, CAT export, SOV generation |
| [08-cross-cutting.md](08-cross-cutting.md) | Cross-Cutting Capabilities | Natural language querying, ABAC with Cedar Policy |
| [09-infrastructure.md](09-infrastructure.md) | Infrastructure Decisions | Authentication, multi-tenancy, notifications, progressive enhancement, performance, bulk import |
| [10-deferred.md](10-deferred.md) | What Is Explicitly Deferred | Phase 3–5 capabilities and boundary contracts for future integration |
| [11-boundary-summary.md](11-boundary-summary.md) | Boundary Summary | All module boundaries, contracts, and data flow directions |
| [12-design-principles.md](12-design-principles.md) | Design Principles | Pure core, boundary contracts, versioned adapters, ABAC, testability |
