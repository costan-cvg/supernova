# 1. The Governing Thought

Public sector risk pools that invest in exposure data quality and member engagement reduce losses, stabilize rates, and retain members. CentuRisk delivers this by making the risk pool flywheel visible and actionable to every participant. The flywheel flows thus: better data flows into better prevention, which reduces losses, which stabilizes rates, which attracts diverse membership, which generates more data — and the cycle repeats. The system's role is to make that cycle legible.

## System Role

Members log in to the system to handle policy changes, claims, view their portfolio's value trending, and understand premium impact forecasts. The system functions as a decision-support tool, not an engagement engine. The pool administrator — not the system — drives engagement and action. Phase 1 does not attempt to build proactive engagement features. It focuses on presenting current, accurate state whenever a member logs in. Premium impact forecasts are provided externally by the pool administrator; the system does not compute them.

## Data Ownership

Member data is owned by the member, not by the pool. The permission model is relationship-based: pools receive a time-scoped access grant. An immutable audit trail ensures data integrity across pool transitions — every mutation is logged with full provenance. Data portability is a design principle built into the architecture. Members may migrate between pools, and the system must not create inadvertent stickiness by coupling data to a pool. The architecture supports portability without requiring a formal handoff ceremony.

## Validation Status

The member experience is inferred from observed customer usage of other tools, not validated directly with facilities managers. This is a known risk. It is mitigated by the adapter architecture: member-facing views can iterate independently of domain logic. The core exposure model is stable; the user experience adapts.
