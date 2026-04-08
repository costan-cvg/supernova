# 02. Cross-Cutting Decisions (Resolved)

These four questions affect every module in the system. They were resolved first to establish the architectural foundation before diving into individual sections.

## 1. System Role: Decision-Support, Not Engagement Engine

**Question:** What brings members back to the portal outside of renewal season, and is the system responsible for driving that engagement?

**Answer:** Policy changes throughout the year, claims, value trending, and premium impact forecasting cause members to log in. The pool administrator drives action and engagement with members. The system generates the information used to make decisions and inform actions but does not need to push members to act.

**Spec Implication:** The system is a decision-support tool. The pool administrator is the human in the loop who drives member action. Phase 1 does not need proactive engagement features (email campaigns, push notifications to drive login). It needs to present current, accurate state when someone logs in.

**Follow-up Resolved:** Premium impact is externally provided by the pool administrator, not computed by the system. The system stores and displays it. No actuarial engine is needed in any phase until What-If modeling is built. Premium impact is an input adapter: the administrator enters values, the system validates and persists them, and the member portal renders them.

## 2. Member Experience: Based on Inferred Behavior

**Question:** Has the portal concept been validated with actual member facilities managers?

**Answer:** Not yet. Decisions are based on observed behavior of customers using other tools.

**Spec Implication:** The member experience design is based on inferred behavior, not direct validation. This is a known risk. Mitigation: the member-facing features (exposure self-service, data quality dashboard, loss prevention views) should be built to allow rapid iteration based on real feedback once a pilot pool is using the system. The adapter architecture already supports this because member-facing views are adapters over the exposure core. Changing the UX does not require changing domain logic. Usability testing should be flagged as a risk in the spec, with a potential lightweight pilot or prototype phase before building the full member experience.

## 3. Data Ownership: Member Owns Data, Not the Pool

**Question:** How do pools relate to each other? Can a member belong to more than one pool?

**Answer:** Members belong to one pool during a given period of time. They may migrate between pools. The data is the member's and they have the right to take it with them. There is evidence of members moving between pools. Visibility of data for a member should be considered as part of a permission model to allow for changing who can access what, not tightly coupled to the pool. There should be something in place to ensure the validity of data not being augmented out of band, verified by the system, so that pools can trust the data as members move.

**Spec Implications:**

- The permission model is relationship-based access, not simple pool-scoped hierarchy. A member's data exists independently; pools are granted visibility through an explicit, time-scoped, revocable relationship.
- Data integrity verification is required: an immutable audit trail where every mutation is logged, every value has provenance, and the pool can see the full history including which pool was active when each change was made.
- The access control model must not structurally couple a member's data to a single pool, even if day-to-day operation assumes one pool at a time.
- Data portability is a design principle, not a frequently exercised workflow. No polished handoff UI is needed in Phase 1, but when the rare move happens, an administrator can reassign the access grant without migrating data between storage boundaries.

## 4. Historical Data Migration Is Part of Onboarding

**Question:** When a pool adopts the system, does their historical data come with them or do they start fresh?

**Answer:** Historical data needs to come with them. Importing that data is part of onboarding. Data typically arrives as CSV or Excel files, potentially tens of gigabytes for large histories dating back over 60 years.

**Follow-up Resolved:** This is usually a one-time import per pool, but having the ability to sip from another system and sync over time is a valuable feature.

**Spec Implications:**

- The onboarding adapter has two distinct modes that share the same destination contract but differ in everything else: an interactive guided flow for individual member first-time submissions, and a batch processing pipeline for historical bulk import.
- The bulk import pipeline needs progress tracking, partial failure handling, resumability, and asynchronous quality scoring. This is a fundamentally different engineering effort from the guided flow.
- The contract between both modes and the exposure core is the same SOV schema. The bulk import adapter is a separate adapter that feeds the same core.
- The architecture should not preclude adding a persistent sync adapter later. The bulk import design must not assume import only runs once per member or hardcode onboarding as the only source of historical records.
