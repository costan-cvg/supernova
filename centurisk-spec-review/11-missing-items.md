# 11. Items Missing from the Document (Resolved)

The following topics were not addressed in the original document but are required for a development specification. All six have been resolved through Q&A.

## 1. Authentication: Federated for Centurisk, Hosted Directory for Pools

**Question:** How do users authenticate? Is there an existing identity provider, or is this greenfield?

**Answer:** Authentication must be adaptable per pool, allowing either hosting the user directory or federation from the pool's identity provider. Centurisk should be able to log in using their federated identity. Initially pools and members will use a hosted directory, implementable using Okta, Auth0, or AWS Cognito.

**Decision:** Phase 1 ships with two authentication paths. Centurisk internal users (system admins) authenticate via their federated identity provider from day one. Pool administrators and members authenticate via a hosted directory implemented using Okta, Auth0, or AWS Cognito. Federation for pools/members is added later when a pool demands it. The identity layer is an adapter — the core receives an authenticated user with role and scope regardless of source.

## 2. Multi-Tenancy: Logical Isolation, Member-Scoped Encryption

**Question:** Are pools sharing the same database with application-level isolation, or does each pool get its own data store?

**Answer:** Member data should be logically isolated but the implementation of the data store should be decided by how that isolation can be supported and allow for the implementation to be changeable. Encryption keys should be specific to members but shared with pools. We should be able to migrate data from one partition to another with minimal effort.

**Decision:** Multi-tenancy uses logical isolation with a changeable implementation strategy. The domain core is agnostic to the isolation mechanism (row-level security, separate schemas, separate databases — all valid, swappable). Encryption keys are member-scoped and shared with the member's current pool — key access follows the pool relationship, supporting the data portability model. When a member moves pools, key access is revoked from the old pool and granted to the new one. Data migration between partitions is an operational task the architecture must support with minimal effort.

## 3. Notifications: In-App Primary, Email Digest for Unacknowledged

**Question:** What is the notification delivery mechanism for Phase 1?

**Answer:** Both in-app and email. Emails should be a digest of unacknowledged in-app notifications. Digest frequency is configurable per pool.

**Decision:** In-app notifications are the primary channel — users see them when they log in. Email operates as a digest of unacknowledged in-app notifications: if a user hasn't acknowledged their notifications, they receive a periodic email summarizing what's waiting. Digest frequency is configurable per pool (daily, weekly, etc.) as a pool administrator setting. The notification system stores notification state (created, delivered, acknowledged/unacknowledged), and the email digest is a scheduled job that queries for unacknowledged notifications.

## 4. Progressive Enhancement for External Service Dependencies

**Question:** What happens when external services (geospatial data, map tile providers, hazard overlays) are unreachable?

**Answer:** Progressive enhancement is the approach. Depending on the client and connection speed, the experience should enhance to allow for higher bandwidth/fidelity.

**Decision:** The system follows a progressive enhancement model. The core experience works without external services — data views, tables, quality scores, and recommendations are all available regardless of connectivity. When external services are available and the client has sufficient bandwidth, the experience enhances with map overlays, geospatial visualizations, and hazard layers. External services are never in the critical path. Every feature that depends on an external service must have a functional fallback using only data from the exposure core. The UI is built base-first, enhancement-second.

## 5. Performance Target: 1 Million Assets, 2,500 Members Per Pool

**Question:** What is the target pool size range for Phase 1?

**Answer:** 1 million assets and 2,500 members per pool.

**Decision:** Phase 1 must support up to 1 million assets and 2,500 members per pool. With the field-level mutation store, this means potentially tens of millions of mutation records per pool. Performance testing at this scale is a Phase 1 requirement, not a nice-to-have. The materialized path queries, temporal resolution, search index, quality scoring, and coverage differential views all need to be benchmarked at this boundary during development.

## 6. Bulk Import Pipeline: Step Function Flow with Replay and Resume

**Question:** Is there an existing data pipeline for historical data migration, or is this greenfield?

**Answer:** There isn't an existing data pipeline. Uploading a CSV or Excel file and running a step function flow with replay and resume should be sufficient.

**Decision:** The bulk import pipeline is greenfield. The implementation is a step function flow (a state machine with discrete processing stages) that accepts CSV or Excel files. Processing stages include: upload, parse, validate schema, map to exposure model, assign/match asset IDs, run quality scoring, and produce import summary for review. Each stage is independently resumable — if processing fails at any stage, the issue can be fixed and processing resumed from that stage rather than starting over. At 1 million assets, individual stages may need to process in batches/chunks.
