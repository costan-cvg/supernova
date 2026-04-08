# 12. Design Principles Summary

**Pure Core, Impure Edges.** The exposure core is a pure, deterministic computation engine. It has no side effects, no external dependencies, no state outside its input/output contracts. The edges — adapters, I/O, authorization, notifications — are impure. They handle format translation, external communication, and statefulness. This boundary makes the core testable, portable, and resilient.

**Boundary Contracts.** Every boundary between system components is defined by a versioned contract: what goes in, what comes out, what is guaranteed. `SOVProcessingResult`, `Recommendation`, `QualityEvent` — these are contracts. Contracts are versioned so components can evolve independently. A new appraisal format (`AppraisalIntakeV2`) fits without changing the core.

**Versioned Adapters.** External formats change. The system adapts. Input adapters (`OnboardingV1`, `AppraisalIntakeV1`) and output adapters (`SOVGeneratorV1`) are versioned from day one. When a pool needs a new format, a new adapter version is added. The core is never touched.

**Composable and Extensible.** Asset types are compositions, not hardcoded. Custom fields are extensible. Scoring rules are authored as data. Quality dimensions are composable. Recommendation categories are defined by CentuRisk admins. The system does not require code changes to support new asset types, new scoring dimensions, or new recommendation categories.

**Test by Layers.** The pure core is tested exhaustively. Adapters are tested in isolation against contracts. Integration tests verify adapters work with the core. The architecture makes testing straightforward: no mocking of infinite external systems, no tangled state.

**Observe and Adjust.** Quality events, NL query telemetry, and loss intake data are not just for visibility — they are feedback loops. Unresolved NL queries inform improvements to the translation layer. Quality event patterns reveal data collection gaps. Loss data (once integrated) informs the ML engine. The system is designed to be measured and improved in production.

**Fail Predictably.** When external services are unavailable, the core and critical adapters continue working. The system degrades gracefully, not catastrophically. Maps and geospatial features become unavailable; everything else works. Error handling is explicit — errors are data, visible and queryable, not hidden in logs.

**ABAC Authorization as Adapter Concern.** The exposure core is authorization-agnostic. It returns data; adapters enforce access control. Cedar policies are evaluated at the adapter layer, not at the core. This keeps the core simple and adapters flexible. Field-level authorization is enforced at the search index. When a policy denies access to a field, that field does not exist in the user's view — not queryable, not visible.
