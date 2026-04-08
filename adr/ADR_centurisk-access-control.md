# ADR: CentuRisk Access Control — ABAC with Cedar Policy

## Status
Proposed

## Context

CentuRisk RMIS manages sensitive exposure data across multiple risk pools, members, and asset hierarchies. Access requirements span a wide spectrum: CentuRisk operational staff need cross-pool visibility, pool administrators manage their own pool's configuration and members, member users manage their organization's data, and external stakeholders (brokers, auditors) need precisely scoped read access down to individual fields.

The original design described three separate access mechanisms: role-based access control, data scoping, and field-level visibility. Maintaining three independent enforcement mechanisms creates consistency risks — a field hidden from rendering but still queryable in search is a data leak. Additionally, the hierarchy of scopes (CentuRisk-wide, pool, member, query path, asset, field) does not map cleanly to traditional role-based tiers.

Key constraints:
- Access must be enforced uniformly across every interface: NL query layer, search index, rendering layer, export adapters.
- Field-level visibility is not cosmetic — invisible fields must be non-queryable and absent from results.
- Broker access is deferred beyond Phase 1 but the model must accommodate it without architectural changes.
- Pool administrators need to assign access without writing policy logic.
- CentuRisk admins need the ability to create custom access patterns.

## Decision

Adopt Attribute-Based Access Control (ABAC) using **Cedar Policy language** as the single unified authorization engine, replacing the three separate mechanisms with one. Cedar is selected because it supports fine-grained attribute-based policies, is open-source (Apache 2.0, maintained by AWS), has formal verification tooling, and its policy language is readable by non-developers.

### User Model: Four Categories

The user model supports four categories, each representing a distinct relationship to the system:

| Category | Description | Scope |
|----------|-------------|-------|
| **CentuRisk Users** | System admins, analysts, auditors, support staff | Cross-pool; varying access levels per role |
| **Pool Administrators** | Pool configuration, approval workflows, member management | Single pool and its members |
| **Member Users** | Manage their organization's exposure data | Single member within a single pool |
| **View-Only Users** | Read access at any granularity level | Arbitrary: CentuRisk-wide down to individual field |

CentuRisk users are NOT all administrators. The category includes analysts (read-heavy, limited write), auditors (read-only with full visibility into audit trails), and support staff (scoped access to assist specific members). Each has distinct Cedar policies.

### Access Decision Model

Every access decision evaluates three attribute sets:

```
AccessDecision = f(UserAttributes, ResourceAttributes, ContextAttributes)
```

**User Attributes:**
- `user.category` — CentuRisk | PoolAdmin | Member | ViewOnly
- `user.organization` — pool or member identifier
- `user.profiles` — list of assigned named profiles
- `user.id` — unique identifier

**Resource Attributes:**
- `resource.type` — Asset | Field | Pool | Member | Report | Workflow
- `resource.pool_id` — owning pool
- `resource.member_id` — owning member (if applicable)
- `resource.hierarchy_path` — full path in asset/org hierarchy
- `resource.field_name` — specific field identifier (for field-level policies)
- `resource.sensitivity` — classification level if applicable

**Context Attributes:**
- `context.effective_date` — temporal scoping (historical vs current)
- `context.approval_state` — draft | pending | approved | rejected
- `context.action` — read | write | approve | export | query
- `context.interface` — portal | api | export | search

Example Cedar policy for a Pool Analyst:

```cedar
permit(
  principal in CentuRisk::Role::"PoolAnalyst",
  action in [Action::"read", Action::"query"],
  resource
) when {
  resource.pool_id == principal.assigned_pool
};
```

Example Cedar policy for field-level restriction:

```cedar
forbid(
  principal,
  action,
  resource
) when {
  resource.type == "Field" &&
  resource.field_name == "replacement_cost" &&
  !(principal in CentuRisk::Role::"CentuRiskAdmin")
};
```

### Named Profiles: Cedar Policy Aliases

Named profiles are human-readable aliases for Cedar policies. They exist for administrative convenience — the Cedar engine always evaluates the underlying policy, never the profile name.

**Phase 1 Predefined Profiles:**

| Profile Name | Category | Access Pattern |
|-------------|----------|----------------|
| CentuRisk Admin | CentuRisk | Full system access, policy management |
| CentuRisk Analyst | CentuRisk | Read access across assigned pools, no configuration |
| CentuRisk Auditor | CentuRisk | Read-only with full audit trail visibility |
| CentuRisk Support | CentuRisk | Scoped to specific member for support cases |
| Pool Administrator | Pool Admin | Full pool config, member management, approval workflows |
| Pool Analyst | Pool Admin | Read access within pool, quality dashboards |
| Member Admin | Member | Full access to own org's exposure data |
| Member User | Member | Limited write access, own org's data |
| Member Read-Only | View-Only | Read access scoped to single member |
| Pool Read-Only | View-Only | Read access scoped to single pool |

**Profile management rules:**
- CentuRisk admins can create custom profiles by composing Cedar policies.
- Pool admins assign existing profiles to their pool's users — they cannot create profiles.
- Updating a profile's underlying Cedar policy immediately affects all users assigned that profile.
- A user can have multiple profiles; the Cedar engine evaluates the union of all applicable policies.

### Field-Level Visibility at Search Index

Field-level visibility is enforced at the search index, not as a post-query filter. When Cedar policy does not grant a user access to a field:

1. **Not queryable** — the field does not exist in the user's view of the search index.
2. **Not in results** — query results omit the field entirely, not masked or redacted.
3. **Not visible** — the field does not appear in any interface: NL query, search, rendering, exports.

This means the search index must be Cedar-aware: every query includes the requesting user's context, and the index evaluates field-level policies before returning results.

### Cedar as Adapter Concern

The Cedar policy engine is an **adapter concern**, not a core concern. The exposure core is authorization-agnostic — it produces data. The Cedar policy layer sits between core and every interface, filtering based on the requesting user's context.

```
User Request → Interface Adapter → Cedar Policy Engine → Exposure Core
                                         ↓
                                   Filter response
                                         ↓
                                   Filtered Result → User
```

The core contains no authorization logic. This keeps the domain model clean and makes the authorization engine replaceable (though Cedar is the Phase 1 and foreseeable-future choice).

### Broker Access (Deferred, Architecture-Ready)

Broker access is deferred beyond Phase 1. When added, brokers fit naturally as **view-only users** with scoped Cedar policies. A broker's policy would grant read access to specific pools, members, or data subsets as negotiated. No architectural changes are needed — only new Cedar policies and possibly a new named profile ("Broker — Pool X SOV Access").

## Alternatives Considered

### Role-Based Access Control (RBAC)
Traditional RBAC with fixed roles (admin, editor, viewer) mapped to permissions. Rejected because the access patterns in CentuRisk are too granular — view-only access at arbitrary hierarchy levels, field-level restrictions, and context-dependent decisions (approval state, effective date) don't map to static role-permission matrices without an explosion of roles.

### Three Separate Mechanisms (Original Design)
The original spec described role-based access + data scoping + field-level visibility as independent systems. Rejected because enforcing consistency across three mechanisms is error-prone (a field hidden in rendering but visible in search is a data leak), and the implementation cost of three systems exceeds one unified engine.

### Open Policy Agent (OPA) with Rego
OPA is a mature ABAC engine. Rejected in favor of Cedar because: Cedar's policy language is more readable for non-developers (important for CentuRisk admins writing custom profiles), Cedar has formal verification tooling (can prove policies don't conflict), and Cedar's entity model maps naturally to CentuRisk's hierarchy of pools/members/assets.

### Custom Authorization Logic in Core
Embedding access rules in the domain model. Rejected because it violates separation of concerns, makes the authorization model difficult to audit, and couples every core change to authorization changes.

## Consequences

**Positive:**
- Single enforcement point eliminates consistency gaps between interfaces.
- Cedar's formal verification can prove policies don't conflict or create unintended access.
- Named profiles provide a user-friendly management layer over powerful policy primitives.
- Broker access, when needed, requires zero architectural changes.
- Field-level enforcement at the search index prevents data leaks through query side-channels.
- Authorization logic is auditable — Cedar policies are plain text, versionable, reviewable.

**Negative / Trade-offs:**
- Cedar Policy is a dependency on a relatively new (2022) open-source project. Mitigation: Cedar is maintained by AWS, Apache 2.0 licensed, and the adapter pattern means it's replaceable.
- Search index must be Cedar-aware, adding complexity to the index layer. Every query evaluates policies, which has performance implications at scale.
- CentuRisk admins must learn Cedar syntax to create custom profiles. Mitigation: Phase 1 ships with predefined profiles covering common patterns; custom profiles are a power-user feature.
- Policy evaluation on every request adds latency. Mitigation: Cedar evaluations are fast (sub-millisecond for typical policy sets); caching of evaluation results for stable contexts is an option.

**New Constraints:**
- The search index implementation must support per-user field-level filtering, not just document-level access.
- Every new interface or adapter must integrate with the Cedar policy engine — no interface may bypass it.
- Cedar policies must be version-controlled and changes audited.
- Named profile changes propagate immediately; there is no "staged rollout" for policy changes in Phase 1.

## Implementation Plan

1. **Cedar policy engine integration** — Stand up the Cedar evaluation engine as an adapter service. Define the entity schema (User, Resource, Context) and load a minimal set of hardcoded policies. Verify evaluation works with unit tests against known user/resource/context combinations.

2. **Predefined named profiles** — Implement the Phase 1 profile set as Cedar policy files. Build the profile-to-policy mapping layer. Write tests proving each profile grants exactly the expected access patterns and nothing more.

3. **User model and profile assignment** — Implement the four-category user model with profile assignment. Pool admins can assign profiles to their users; CentuRisk admins can assign any profile. Verify through integration tests: create user, assign profile, evaluate access decision.

4. **Field-level enforcement at search index** — Integrate Cedar evaluation into the search index query path. When a user queries, evaluate field-level policies and exclude restricted fields from both the query scope and results. Test with users who have different field-level restrictions querying the same assets.

5. **Interface integration (NL query, rendering, exports)** — Wire Cedar evaluation into every adapter: NL query layer checks field visibility before translating queries, rendering layer filters fields before display, export adapters filter fields before generating output. End-to-end test: same asset viewed by users with different profiles shows different fields.

6. **Custom profile management** — Build the admin interface for CentuRisk admins to create and manage custom Cedar profiles. Include policy validation (syntax check, conflict detection via Cedar's formal analysis). Test: admin creates custom profile, assigns it, user's access matches the new policy.

7. **Audit and observability** — Log all policy evaluation decisions (who, what resource, what action, permit/deny, which policy). Ensure audit logs are queryable for compliance reporting. This telemetry follows the same structured event pattern used elsewhere in the system.
