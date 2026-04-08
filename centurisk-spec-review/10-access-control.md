# 10. Access Control — ABAC with Cedar Policy (Resolved)

The original document described a simpler role-based access model with three levels (role, data scope, field visibility). Through review, this has been significantly expanded to an Attribute-Based Access Control (ABAC) model using Cedar Policy language, with a richer user model and arbitrarily granular view-only scoping.

## 1. User Model: Four Categories, Not Three Tiers

**Discovery:** The original document described member users and pool administrators. The review identified Centurisk system admins. Further discussion revealed that Centurisk has operational staff beyond admins who work across pools and members, and that view-only users need arbitrarily granular scoping.

**Decision:** The user model supports four categories:

- Centurisk users — includes system admins and operational staff who work across pools and members. These are not all administrators; some are analysts, auditors, or support staff with varying levels of access.
- Pool administrators — manage their pool's configuration, approval workflows, and member relationships.
- Member users — manage their organization's exposure data within the portal.
- View-only users — can be scoped to any level of the hierarchy, from Centurisk-wide total view down to a single pool, member, query path, or individual asset field. These are not a fixed tier; the scope is defined by policy.

**Spec Implication:** Brokers (deferred beyond Phase 1) fit naturally into this model as view-only users with scoped read access defined by Cedar policy. No architectural changes needed when broker access is added.

## 2. ABAC with Cedar Policy Language

**Question:** Is the access model role-based (RBAC) or attribute-based (ABAC)?

**Answer:** Attribute-based access control. Cedar Policy language matches this use case well.

**Decision:** The access control model is ABAC, expressed using Cedar Policy language. Access decisions are based on attributes of the user, the resource (down to individual field level), and the context (hierarchy path, effective date, approval state) rather than fixed role-to-permission mappings. This replaces the three separate mechanisms in the original document (role-based access, data scoping, field-level visibility) with a single unified policy engine. All three are expressible as Cedar policies — they become specific patterns of attribute-based rules rather than separate subsystems.

**Spec Implications:**

- Cedar Policy is the authorization engine for the entire system in Phase 1. This is a foundational infrastructure component, not a future upgrade.
- Every access decision — what a user can see, edit, approve, export — is evaluated by the Cedar engine against the user's attributes, the resource's attributes, and the context.
- The policy engine is an adapter concern: the exposure core produces data; the Cedar policy layer filters it based on the requesting user's context. The core contains no authorization logic.

## 3. Named Profiles: Aliases for Cedar Policies

**Question:** How do administrators manage the complexity of ABAC policies?

**Answer:** Policies should be aliased to profile names for reference and reuse.

**Decision:** Cedar policies can be aliased to named profiles for convenience and reuse. A profile like "Pool Analyst" or "Centurisk Field Auditor" is a human-readable name for a Cedar policy that grants specific access scoped to specific attributes. Administrators assign profiles to users rather than writing policies directly. New profiles can be created by composing Cedar policies, and existing profiles can be updated without changing user assignments.

**Spec Implications:**

- Phase 1 ships with a set of Centurisk-defined named profiles covering the common access patterns (pool admin, member user, view-only at pool level, view-only at member level, Centurisk analyst, etc.).
- Centurisk admins can create custom profiles by writing Cedar policies. Pool admins assign existing profiles to their users.
- The profile system is a convenience layer over Cedar — the engine always evaluates the underlying policy, not the profile name.

## 4. Field-Level Visibility: Enforced Everywhere Including Search

**Question:** When a Cedar policy hides a field from a user, does the search index also enforce that restriction?

**Answer:** Yes, the search index should take into account the user's permissions at the field level.

**Decision:** Field-level visibility as defined by Cedar policy is enforced at the search index level. If a user's policy does not grant access to a field, that field is completely invisible in search — they cannot query against it, and it does not appear in results. This is not just a rendering restriction; it is an access restriction enforced uniformly across all interfaces: the NL query layer, the search index, the rendering layer, and export adapters all evaluate the same Cedar policies.
