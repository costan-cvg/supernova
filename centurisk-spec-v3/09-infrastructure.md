# 9. Infrastructure Decisions

Six infrastructure decisions complete the specification foundation. These were absent from the original value proposition but are necessary for building the system.

## Authentication

Authentication follows a two-path model. CentuRisk internal users authenticate via federated identity from day one. Pool administrators and members use a hosted directory (Okta, Auth0, or AWS Cognito). Federation for pools is added per demand. The identity layer is an adapter — the core receives an authenticated user with role and scope regardless of source.

## Multi-Tenancy

Multi-tenancy uses logical isolation with a changeable implementation strategy. The domain core is agnostic to the isolation mechanism. Encryption keys are member-scoped and shared with the member's current pool — key access follows the pool relationship, supporting data portability. When a member moves pools, key access is revoked from the old pool and granted to the new.

## Notifications

Notifications use in-app delivery as the primary channel. Email operates as a digest of unacknowledged in-app notifications, with frequency configurable per pool. The notification system tracks state: created, delivered, acknowledged.

## Progressive Enhancement

Progressive enhancement governs external service dependencies. The core experience works without external services — data views, tables, quality scores, recommendations all function without connectivity. Maps, geospatial overlays, and hazard layers enhance when available. External services are never in the critical path.

## Performance

Performance target for Phase 1 is 1 million assets and 2,500 members per pool. With the field-level mutation store, this means potentially tens of millions of mutation records. Performance testing at this scale is a Phase 1 requirement.

## Bulk Import Pipeline

The bulk import pipeline is a greenfield step function flow accepting CSV or Excel. Each stage is independently resumable. At target scale, stages process in batches.
