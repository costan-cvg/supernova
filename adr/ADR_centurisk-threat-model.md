# Threat Model: CentuRisk RMIS

## System Overview

CentuRisk is a multi-tenant Risk Management Information System for public sector risk pools. It manages exposure data (assets, valuations, coverage) for member organizations (cities, counties, school districts) that pool their insurance risk. The system is a decision-support platform, not an engagement engine.

**Key components:**
- **Exposure Core** — pure, deterministic domain engine (asset registry, temporal resolution, quality scoring, recommendations)
- **SOV Pipeline** — ingests exposure data changes through validation, diff, and quality scoring; feeds an approval workflow
- **ABAC / Cedar Policy Engine** — attribute-based access control evaluated at every interface boundary
- **NL Query Layer** — rule-based natural-language-to-structured-query translation over a Cedar-aware search index
- **I/O Adapters** — bulk import (CSV/Excel), appraisal intake, SOV generation (Excel/PDF), CAT export pre-flight
- **Member/Pool Adapters** — member self-service portal, renewal workflow, coverage views, quality dashboard, recommendations
- **Authentication Layer** — federated IdP for CentuRisk staff; hosted directory (Okta/Auth0/Cognito) for pool admins and members; optional per-pool federation
- **Notification System** — in-app primary; email digest secondary
- **Immutable Audit Trail** — append-only log of every mutation with full provenance

**Data flow (simplified):**
```
External Identity Provider
         |
         v
  [Auth Adapter] --> AuthenticatedUser
         |
         v
  [Interface Adapters]  <-- Member/Pool Admin UI, API
         |
         v
  [Cedar Policy Engine] --> permit/deny decisions
         |
         v
  [Exposure Core] <--> [Asset Registry / Mutation Store]
         |
         |-----> [Quality Scoring Engine]
         |-----> [Recommendation Engine]
         |-----> [Audit Trail Store]
         |
  [SOV Pipeline] --> [Approval Workflow]
         |
  [Output Adapters] --> SOV Excel/PDF, CAT Export
         |
  [Search Index] <-- NL Query Layer
         |
  [Notification System] --> In-App, Email Digest
```

**Sensitive data in scope:**
- Asset valuations and replacement costs (financial data)
- Location data, construction attributes, occupancy data (property intelligence)
- Member PII (user credentials, session data)
- Loss event records (insurance claims precursors)
- Encryption keys (member-scoped, pool-granted)
- Cedar policies (intellectual property / authorization rules)
- Recommendation rules (intellectual property)
- Audit trail records (full mutation history)

---

## Trust Boundaries

1. **External to Application** — Public internet to the application. User-supplied input enters here (login, file uploads, inline edits, NL queries, loss event submissions). Protected by: TLS, authentication, rate limiting (not yet specified).
2. **Authentication Layer to Application** — The IdP token / session token is validated, and the identity adapter produces `AuthenticatedUser`. Trust boundary between: raw JWT/SAML token and a validated, system-internal identity object.
3. **Interface Adapter to Cedar Engine** — Every data request crosses a Cedar policy evaluation gate. Cedar runs at the adapter layer; the core is authorization-agnostic. If any adapter bypasses Cedar, it becomes an unprotected trust boundary.
4. **Cedar Engine to Exposure Core** — After Cedar permits an action, the core processes it. The core must not re-perform authorization, but it also must not receive data that was never authorized.
5. **Bulk Import Boundary** — CSV/Excel files uploaded by pool admins or CentuRisk staff. These files are untrusted external data entering the system. The seven-stage pipeline is the trust boundary enforcement mechanism.
6. **Appraisal Intake Boundary** — Structured appraisal records submitted as `AppraisalIntakeV1`. The adapter validates before the core accepts the data.
7. **SOV Export / CAT Export Boundary** — Data leaving the system toward brokers, CAT models, and underwriters. Data must be filtered to only what the requesting principal is authorized to export.
8. **NL Query Boundary** — User natural language input is translated to structured queries. The translation layer and search index are a trust boundary between human-entered text and the search engine.
9. **Inter-Pool Boundary** — The multi-tenancy model separates pools via `TenantContext`. A pool admin should never see another pool's data.
10. **Member / Pool Admin Boundary** — Members access only their own data within their pool grant. Pool admins access all members in their pool. This is enforced by Cedar policies and the access grant model.
11. **Audit Trail Boundary** — Immutable append-only store. Any path that can modify or delete audit entries represents a critical trust boundary violation.
12. **Encryption Key Store Boundary** — Member-scoped keys with pool grants. The key grant lifecycle is critical: revocation must be enforced before a former pool can access data they lost rights to.

---

## STRIDE Analysis

### Spoofing Threats

| # | Threat | Component | Likelihood | Impact | Risk | Mitigation |
|---|--------|-----------|------------|--------|------|------------|
| S1 | Attacker replays a stolen JWT from CentuRisk federated path to gain cross-pool admin access | Auth Adapter / Federated IdP | Medium | Critical | **CRITICAL** | Enforce short JWT TTLs (15 min max), bind tokens to IP/device fingerprint where IdP supports it, validate `pool_scope` claim is present for pool-scoped tokens, implement token revocation list for compromise scenarios |
| S2 | Pool federation is added on-demand for a pool; attacker compromises the pool's IdP and mints tokens with elevated `user_category` claims | PoolFederated Auth Adapter | Medium | Critical | **CRITICAL** | CentuRisk must validate federated token claims against its own authoritative user/profile store — do not trust `user_category` or `cedar_profile_ids` embedded in external IdP tokens; re-derive these from the system's user record on every authentication |
| S3 | Attacker registers a member account in the hosted directory and spoofs another member's `member_scope` by manipulating request context | Hosted Directory Auth, Interface Adapters | Low | Critical | **HIGH** | `member_scope` in `AuthenticatedUser` must be derived from the user record and access grant model server-side — never accepted from client-supplied headers or request parameters; Cedar policies must validate `resource.member_id == principal.member_scope` |
| S4 | CSRF attack on state-changing endpoints (approval, renewal flag resolution, asset edits) causes legitimate pool admin to unknowingly approve attacker-submitted changes | Web Interface Adapters | Medium | High | **HIGH** | Enforce `SameSite=Strict` on session cookies, implement CSRF tokens for all state-changing operations, validate `Origin` and `Referer` headers |
| S5 | Loss event intake — attacker submits loss events with a spoofed `asset_id` belonging to another member's asset | Loss Event Intake | Medium | High | **HIGH** | Validate that the `asset_id` in the loss event is within the submitting member's authorized scope via Cedar; server-side re-authorization on every resource reference |
| S6 | Bulk import — attacker submits a file claiming to be from a different pool by manipulating pipeline metadata | Bulk Import Pipeline | Low | High | **MEDIUM** | `pool_id` on `BulkImportJob` must be bound to the authenticated principal's pool scope at creation time, not sourced from the file content; tamper-evident job metadata |
| S7 | The `auto_approve` flag on a user profile could be spoofed if the approval routing reads it from a user-supplied parameter instead of the server-side user record | SOV Pipeline / Approval Workflow | Low | High | **MEDIUM** | `auto_approve` status must be read from the authoritative server-side user profile at routing time, not from any client-controlled attribute; log auto-approve decisions in audit trail |

---

### Tampering Threats

| # | Threat | Component | Likelihood | Impact | Risk | Mitigation |
|---|--------|-----------|------------|--------|------|------------|
| T1 | Audit trail entries are modified or deleted by a privileged internal user or a compromised CentuRisk admin account, destroying regulatory compliance evidence | Audit Trail Store | Low | Critical | **HIGH** | Audit trail must be physically append-only (e.g., write to an immutable object store or WORM-compliant database, or use cryptographic chaining); CentuRisk admin operations on audit records must themselves be logged to a separate, independently protected audit log; restrict DDL access to audit tables at the database level |
| T2 | Field-level mutation records are modified in the database to alter historical valuation history without detection | Mutation Store | Low | Critical | **HIGH** | Implement cryptographic integrity checks on mutation records (HMAC or hash chain over mutation contents); any mismatch detectable on read; database-level write protection on committed mutation rows |
| T3 | Cedar policy files are modified outside the version-controlled policy management flow, granting unauthorized access silently | Cedar Policy Store | Low | Critical | **HIGH** | Cedar policy changes must go through a version-controlled, audited change workflow; detect out-of-band policy file modifications via file integrity monitoring; the ADR notes policies must be version-controlled but does not specify the enforcement mechanism |
| T4 | SOV export or CAT export file is tampered with in transit or in the download staging area before the recipient receives it | Export Adapter / Download | Medium | High | **HIGH** | Sign exported files with a CentuRisk-issued signature (HMAC or digital signature); provide the recipient with a verification mechanism; use HTTPS for all downloads; avoid storing exports in shared, publicly accessible storage |
| T5 | Renewal flag state (`open` / `resolved`) tampered to mark an unresolved flag as resolved, enabling fraudulent bulk approval of disputed assets | Renewal Flag Store | Medium | High | **HIGH** | Flag state transitions must be authorized by the pool admin role only; flag lifecycle events must be captured in the audit trail; bulk approval must re-check live flag state at execution time, not trust cached state |
| T6 | Materialized path hierarchy is modified during an administrative restructuring operation non-atomically, corrupting the access control prefix matching | Asset Registry / Hierarchy | Low | High | **MEDIUM** | Hierarchy recomputation must execute within an atomic database transaction propagated to all descendants simultaneously; any partial failure rolls back entirely; the ADR acknowledges this requirement but gives no implementation detail |
| T7 | Import intermediate stage outputs are tampered with between stages in the bulk import step function, injecting malicious asset data | Bulk Import Step Function State | Low | High | **MEDIUM** | Stage output data should be integrity-checked (checksum) at the start of each subsequent stage; the SHA-256 checksum is noted for the source file but not for intermediate stage outputs; extend checksum coverage to each persisted stage artifact |
| T8 | NL query synonym registry is modified to map benign query terms to field names the querying user should not be able to access, bypassing Cedar field filtering | NL Translation Layer | Low | Medium | **MEDIUM** | Changes to the synonym registry must go through the same audited change workflow as Cedar policies; the NL layer must re-evaluate Cedar visibility after synonym resolution, not before |

---

### Repudiation Threats

| # | Threat | Component | Likelihood | Impact | Risk | Mitigation |
|---|--------|-----------|------------|--------|------|------------|
| R1 | A pool admin approves or rejects an asset valuation change and later denies doing so, creating a regulatory dispute | Approval Workflow / Audit Trail | Medium | High | **HIGH** | Ensure audit entries for approval actions include `actor_id`, `actor_role`, `pool_id`, `timestamp`, and the approved/rejected diff; the ADR defines this structure but approval-specific audit fields must be verified at implementation time; consider requiring an explicit acknowledgment step for valuation approvals |
| R2 | A member submits a renewal modification and later disputes the values they submitted | Renewal Adapter / SOV Pipeline / Audit Trail | Medium | High | **HIGH** | The `source: renewal` discriminator and the full `FieldChange` diff must be persisted in the audit trail; the member's submitted values must be immutably recorded with their `actor_id` at submission time |
| R3 | CentuRisk admin modifies a Cedar policy rule that retroactively narrows access, then denies that the policy was changed | Cedar Policy Audit Log | Low | Medium | **MEDIUM** | Cedar policy changes must produce audit entries in the immutable audit trail with previous and new policy state captured; the ADR notes policies must be "version-controlled and changes audited" but the audit store for policy changes is not specified |
| R4 | A CentuRisk admin modifies recommendation rules (intellectual property) and the system cannot prove which version of the rules was active when a recommendation was generated | Recommendation Engine / Rule Store | Low | Medium | **MEDIUM** | Recommendation records should reference the specific rule ID and rule version that produced them; rule versions must be immutably stamped when activated; this is unspecified in the current ADR |
| R5 | NL query telemetry (`NLQueryEvent`) stores the user's raw query text; user later disputes that they made a particular query | NL Query Telemetry | Low | Low | **LOW** | `NLQueryEvent` records the `user_id`; ensure this is the authenticated, system-assigned user ID, not a user-supplied identifier; retain telemetry records with the same protections as audit trail records |

---

### Information Disclosure Threats

| # | Threat | Component | Likelihood | Impact | Risk | Mitigation |
|---|--------|-----------|------------|--------|------|------------|
| I1 | IDOR: a member user constructs a direct API request with another member's `asset_id` and retrieves asset data outside their Cedar-authorized scope | All Data Access APIs | High | Critical | **CRITICAL** | Cedar policy evaluation must occur on every single resource fetch, not only on list/search queries; every handler that accepts a resource ID must re-validate the principal has access to that specific resource; this is the single highest-probability attack on the system given multi-tenancy |
| I2 | Cross-tenant data leakage: a bug in `TenantContext` injection (missing tenant filter on a query) exposes one pool's data to another pool's users | Multi-Tenancy Layer / Repository Operations | Medium | Critical | **CRITICAL** | The ADR mandates a permanent cross-tenant leakage prevention test in CI — this is essential and must cover every query path; a single missing tenant filter in any repository operation is a critical P0 defect; add database-level row-security as defense-in-depth even if application-level filtering is the primary control |
| I3 | Search index query returns field values that Cedar should have hidden, because Cedar evaluation is done incorrectly (e.g., post-query filter misapplied rather than index-level field exclusion) | Search Index / Cedar Integration | Medium | High | **HIGH** | Cedar field-level enforcement must be at query construction time (fields removed from the query), not post-filter (fields removed from results); a post-filter is bypassable by count-based inference — if the result count changes when a field is present vs absent in a query, the field's content is partially leaked; verify with adversarial tests |
| I4 | Verbose `ProcessingError` messages in `SOVProcessingResult` leak internal field names, schema details, or database structure to member users | SOV Pipeline / Error Handling | Medium | Medium | **MEDIUM** | `ProcessingError.message` must be sanitized before rendering to member-facing adapters; raw validation error messages (e.g., SQL constraint names, internal field paths) must be mapped to user-friendly messages; internal diagnostic details logged server-side only |
| I5 | `ImportSummary` returned to a pool admin contains row-level error details that inadvertently expose another member's asset attributes (e.g., duplicate detection errors showing existing asset field values) | Bulk Import Pipeline | Medium | High | **HIGH** | Import summary errors must not expose field values from existing records that were not part of the current import file; duplicate detection errors should report the fact of a duplicate (and its asset_id) without echoing the conflicting field values from the existing record |
| I6 | NL query telemetry stores raw user query text which may contain sensitive data (e.g., "show me assets with replacement cost below $500K in ZIP 90210 owned by member Acme School District") | NL Query Telemetry / `NLQueryEvent` | High | Medium | **HIGH** | The ADR notes CentuRisk admins can review unresolved query logs — this is intentional; however, access to telemetry must be Cedar-controlled; CentuRisk analysts should not be able to use telemetry to infer member portfolio details; consider scrubbing or tokenizing specific field values from raw query text in telemetry |
| I6b | `NLQueryEvent` stores `original_query` (the user's raw natural language input) which may contain PII (member names, addresses) in queries like "find assets at 123 Main St, Springfield owned by John Doe" | NL Query Telemetry | High | Medium | **HIGH** | Implement a PII scrubbing step before persisting `original_query` to telemetry storage; alternatively, apply strict data retention limits (e.g., 90-day rolling window) and access controls on raw query logs |
| I7 | SOV export files (Excel/PDF) contain full asset portfolio data; if exported files are stored in a shared or publicly accessible location, a compromised URL leaks sensitive exposure data to unauthorized parties | SOV Export / Download Staging | Medium | High | **HIGH** | Generated export files must be stored with per-user access controls; download URLs must be time-limited (pre-signed with short TTL) and single-use; files must be deleted from staging after download or after TTL expiry |
| I8 | `AccessGrant` records expose member-to-pool relationship history; if any API returns historical grant records without authorization, former pool administrators can learn that a member left their pool | Access Grant Store | Low | Medium | **MEDIUM** | Access grant queries must require CentuRisk admin or the specific member's authorization; pool admins must not be able to enumerate other pools' grant history for a member |
| I9 | Member-scoped encryption keys: if `EncryptionKeyGrant` metadata is accessible to any pool admin, a pool whose grant was revoked can learn the key ID that is currently held by the new pool | Key Grant Metadata | Low | High | **MEDIUM** | `EncryptionKeyGrant` records must be scoped such that each pool can see only its own grant, not the full member grant history; CentuRisk admin access to full history is appropriate but must be logged |
| I10 | Error messages during federated authentication reveal whether a user account exists in the CentuRisk system (account enumeration) | Auth Adapter | High | Low | **MEDIUM** | Return generic error messages for all authentication failures ("Invalid credentials"); do not differentiate between "user not found" and "wrong password" in any externally visible response |
| I11 | Digest email bodies contain summaries of unacknowledged notifications; if email is delivered to a shared mailbox or intercepted, pool-level quality alerts and approval status are disclosed | Email Digest | Medium | Medium | **MEDIUM** | Digest emails should include deep links but minimize the sensitive content in the email body itself; consider whether summarizing quality scores and approval counts in an email body is acceptable given the public-sector context |

---

### Denial of Service Threats

| # | Threat | Component | Likelihood | Impact | Risk | Mitigation |
|---|--------|-----------|------------|--------|------|------------|
| D1 | A member user submits an extremely large file as a bulk import, exhausting disk or memory in the upload or parse stage | Bulk Import Pipeline / File Upload | Medium | High | **HIGH** | Enforce maximum file size limits at the upload boundary; the ADR specifies no file size limit; recommend 100MB-500MB limit with a configurable override for CentuRisk admins; stream-parse files rather than loading entirely into memory |
| D2 | A malicious or buggy NL query is crafted to generate an extremely expensive search index query (e.g., unbounded regex, cross-field OR explosion) | NL Query Layer / Search Index | Medium | High | **HIGH** | Apply query complexity limits (max terms, max range size, max result window) at the search index level before execution; NL layer should not generate unbounded queries; search queries must time out and return an error rather than running indefinitely |
| D3 | The incremental quality rescoring triggered by bulk import creates a scoring job storm that saturates the background worker pool, delaying other pool operations | Quality Scoring Pipeline | Medium | Medium | **MEDIUM** | Rate-limit or batch the scoring jobs queued by bulk import; separate import scoring jobs from interactive scoring jobs with distinct worker pools or priority queues; the ADR notes asynchronous scoring but does not specify backpressure |
| D4 | An attacker repeatedly submits invalid SOV data through the inline edit path, flooding the approval queue with junk pending-approval records | SOV Pipeline / Approval Workflow | Medium | Medium | **MEDIUM** | Rate-limit SOV submissions per user/member; validation failures should not create pending approval records — the approval queue must only contain records that passed validation; enforce per-user submission rate limits |
| D5 | An attacker triggers coverage differential computation for large date ranges (hundreds of policy periods, thousands of assets), causing a very expensive query | Coverage Differential View | Low | Medium | **MEDIUM** | Enforce limits on the number of policy periods that can be compared in a single differential request; enforce maximum asset count per comparison request |
| D6 | The email digest job processes a pool with a very large number of unacknowledged notifications, causing the digest generation to run for an extended time and delay notifications for other pools | Email Digest Scheduler | Low | Low | **LOW** | Impose a maximum notification count included in a single digest; use per-pool digest jobs with independent scheduling, not a single sequential run across all pools |
| D7 | The bulk import pipeline is retried in a tight loop after a failing stage, consuming processing resources without making progress | Bulk Import Step Function | Low | Medium | **LOW** | Implement exponential backoff and a maximum retry count per stage; failed stages must require explicit administrator action to resume (the ADR implies this but does not specify retry constraints) |

---

### Elevation of Privilege Threats

| # | Threat | Component | Likelihood | Impact | Risk | Mitigation |
|---|--------|-----------|------------|--------|------|------------|
| E1 | A pool admin assigns themselves a custom Cedar profile that grants CentuRisk admin privileges | Cedar Profile Management | Medium | Critical | **CRITICAL** | Pool admins must only be able to assign predefined profiles; profile creation and custom Cedar policy authoring must be restricted to CentuRisk admin category only; the Cedar policy engine must enforce this through its own forbid rules, not just through UI restrictions |
| E2 | A member user exploits a flaw in the `auto_approve` flag evaluation to have their submissions auto-approved when the flag is not set for their account | SOV Pipeline / Approval Routing | Medium | High | **HIGH** | `auto_approve` routing logic must be covered by explicit tests for boundary cases: user with flag not set must always produce a pending record; the default must be deny (pending approval), not allow (auto-approve) |
| E3 | A CentuRisk support user (limited, member-scoped access) escalates to see other members' data by crafting requests that omit the member scope parameter | CentuRisk Support Profile / Cedar | Medium | High | **HIGH** | CentuRisk support profiles must have a `forbid` rule that denies access to any resource where `resource.member_id` does not match the support user's assigned member scope; the scope must be set server-side and not be modifiable by the support user |
| E4 | An attacker compromises a pool administrator account and uses the `AccessGrant` creation API to grant themselves a new pool's access grant with an open-ended `effective_to: null` | Access Grant Store | Low | Critical | **HIGH** | Access grant creation must be logged and alerted on (security anomaly detection); consider requiring dual authorization (two pool admins) for access grant modifications; the ADR does not specify who can create/modify access grants — this must be tightened to CentuRisk admins only for cross-pool grant operations |
| E5 | A view-only user discovers that the NL query layer, if Cedar integration is incomplete, allows querying fields that should be invisible to them by using field name synonyms that bypass the pre-query field removal step | NL Layer / Cedar | Medium | High | **HIGH** | Cedar field-level visibility must be evaluated after synonym resolution (on the resolved canonical field name), not before; the ADR notes the NL layer is Cedar-aware and "does not suggest queries involving fields the user cannot see," but this must be enforced by the Cedar gate, not just by omitting the field from suggestions |
| E6 | A CentuRisk analyst (read-only) discovers a path through the export adapters that does not enforce Cedar field-level restrictions, obtaining a full, unrestricted SOV export | Export Adapters | Low | High | **MEDIUM** | Every output adapter (SOV, CAT) must evaluate Cedar before generating output; the ADR states Cedar must be integrated into "every adapter" but this must include export adapters explicitly; add export-specific authorization tests |
| E7 | Pool federation: a pool with its own IdP misconfigures the group-to-profile mapping, causing regular member users to receive pool admin profiles | PoolFederated Auth Adapter | Low | High | **MEDIUM** | CentuRisk must own the authoritative mapping from pool IdP groups to CentuRisk profiles; this mapping must live in CentuRisk's system, not in the pool's IdP claims; validate profile assignments against the authoritative mapping on every token exchange |

---

## Attack Surface Summary

- **External-facing endpoints:** Login (federated + hosted directory), member portal API, pool admin API, file upload (bulk import, appraisal intake), NL query endpoint, SOV export download, webhook/callback for IdP federation, Certificate of Insurance generation
- **Internal interfaces:** Cedar policy evaluation service, search index, exposure core repository layer, bulk import step function stages, quality scoring worker, notification scheduler, email delivery service
- **Third-party integrations:** External IdP (Okta/Auth0/Cognito), CentuRisk federated IdP, email delivery (SES/SendGrid), map tile provider, geocoding service, hazard data provider

---

## Critical Findings (P0 — MUST FIX Before Shipping)

### [P0-CRITICAL] IDOR: Object-Level Authorization Not Explicitly Enforced Per-Resource-Fetch

**Threat:** I1

**Component:** All API endpoints that accept resource IDs (asset IDs, member IDs, mutation IDs, loss event IDs, renewal flag IDs)

**Attack Vector:** A member user is authenticated and holds a valid session. They observe an `asset_id` from their own portfolio (e.g., from a browser network tab) and substitute it with an incrementally guessed or enumerated ID belonging to another member's asset. If the endpoint fetches by ID without re-evaluating Cedar against the requesting principal's scope, the response returns data from a different member's portfolio — constituting a cross-tenant data leak.

**Why the current ADR does not fully address this:** The ADR describes Cedar as operating between interface adapters and the core, filtering responses. However, it does not specify that Cedar is evaluated on each individual object fetch (as opposed to only on list/search queries). The ABAC policy examples shown are scoped to `read` actions on resources, but there is no specification that guarantees every single-resource fetch routes through Cedar before returning data.

**Mitigation:**
- Implement a mandatory Cedar evaluation wrapper on every repository method that returns a single resource by ID
- Adopt an explicit "deny by default" pattern: if Cedar does not explicitly permit access to a resource, the fetch returns 404 (not 403, to avoid resource enumeration)
- Add a CI-enforced contract test that verifies: given a valid session for Member A, fetching any resource owned by Member B returns 404

---

### [P0-CRITICAL] Cross-Tenant Data Leakage via Missing TenantContext in Repository Operations

**Threat:** I2

**Component:** Multi-tenancy layer, all repository/store operations

**Attack Vector:** If a single repository method is implemented without injecting `TenantContext` — even one rarely-used code path such as a batch quality scoring query, an audit trail lookup, or a notification fetch — the query executes without a pool filter and can return results across all pools. In a multi-tenant SaaS serving public sector entities, this constitutes a regulatory breach.

**Why the current ADR does not fully address this:** The ADR mandates that "all queries include tenant filtering; no query path bypasses isolation except explicit `CrossPool` scope held only by CentuRisk users" and requires a "permanent CI test." This is architecturally correct but the enforcement mechanism is a CI test, not a type-system or database-level enforcement. A developer adding a new repository method can inadvertently omit `TenantContext` and the only safeguard is the test suite's coverage of that specific code path.

**Mitigation:**
- Consider making `TenantContext` a required constructor argument on the repository/store type itself (not an optional method parameter), so the type system enforces its presence at compile time in Rust
- Add database-level row security as defense-in-depth (PostgreSQL RLS or equivalent) — this catches what the application layer misses
- The CI test is necessary but not sufficient; it must be explicitly tested against every query path, not just the happy path

---

### [P0-CRITICAL] Pool-Federated Identity Claims Must Not Be Trusted for User Category or Profile Assignment

**Threat:** S2

**Component:** PoolFederated Auth Adapter, `AuthenticatedUser` construction

**Attack Vector:** When a pool adds its own IdP via federation (added on-demand), the IdP issues tokens containing claims. If the system trusts any claim in the pool's IdP token for `user_category` or `cedar_profile_ids`, a pool's IT administrator who controls their own IdP can mint a token claiming `user_category: CentuRiskUser` or `cedar_profile_ids: [CentuRiskAdmin]`. This grants the pool's user cross-pool CentuRisk admin privileges without any CentuRisk approval.

**Why the current ADR does not fully address this:** The `AuthenticatedUser` contract includes `user_category`, `cedar_profile_ids`, `pool_scope`, and `member_scope`. The infrastructure ADR describes adding federation "on demand" but does not specify which claims from the external IdP are trusted and which must be sourced from CentuRisk's own database. This gap is a P0 design flaw.

**Mitigation:**
- The authoritative source for `user_category` and `cedar_profile_ids` must always be CentuRisk's own user database, keyed on the validated `sub` claim from the external IdP
- The only claim trusted from a pool's IdP is the user's stable identifier (`sub`) and possibly their email for lookup purposes
- After validating the IdP token's signature and claims, the system must perform a lookup in its own user table to retrieve the user's `user_category` and profiles — never from the IdP token
- This must be explicitly documented in the infrastructure ADR and enforced as a contract test

---

### [P0-CRITICAL] Cedar Profile Escalation: Pool Admins Must Not Be Able to Create or Self-Assign Elevated Profiles

**Threat:** E1

**Component:** Cedar Profile Management, named profiles

**Attack Vector:** The ADR states "CentuRisk admins can create custom profiles by composing Cedar policies. Pool admins assign existing profiles to their pool's users — they cannot create profiles." However, if the enforcement of this restriction is only in the UI layer (pool admins don't see a "create profile" button), an API-level call to the profile creation endpoint with a pool admin session token would succeed if the Cedar policy engine itself does not deny the action. An attacker who can enumerate or brute-force the profile creation API endpoint could write a Cedar policy that grants themselves CentuRisk admin rights.

**Mitigation:**
- The profile creation API endpoint must itself be protected by a Cedar `forbid` rule: `forbid(principal, Action::"createProfile", resource) unless principal in CentuRisk::Role::"CentuRiskAdmin"`
- Pool admin self-assignment of profiles must be bounded by a Cedar policy that only allows assigning profiles with a maximum privilege level lower than the pool admin's own
- Add an integration test: attempt to create a profile via the API using a pool admin session token and assert a 403 response
- Consider implementing profile content validation: Cedar's formal analysis tools can verify that no profile grants privileges exceeding its category's ceiling

---

## High Priority Findings (P1 — SHOULD FIX Before Production)

### [P1] Audit Trail Integrity: No Cryptographic Tamper Detection Specified

**Threat:** T1, T2

**Component:** Audit Trail Store, Mutation Store

**Attack Vector:** A CentuRisk system admin with database access (or a compromised DBA account) modifies audit trail entries or mutation records to cover up a regulatory breach, fraudulent approval, or data manipulation. The current ADR states the audit trail is "immutable" but does not specify the mechanism. If "immutable" means only "the application layer does not expose a delete endpoint," a database-level modification goes undetected.

**Mitigation:**
- Implement a cryptographic hash chain over audit entries: each `AuditEntry` contains a hash of the previous entry's hash plus its own content; tampering with any entry invalidates all subsequent hashes
- Run a periodic integrity verification job that walks the hash chain and alerts on violations
- Store audit entries in an append-only structure at the storage layer (e.g., PostgreSQL `INSERT`-only table with DDL-level `DELETE`/`UPDATE` denied to application role; or offload to an immutable object store with Object Lock)
- Similarly, `FieldMutation` records should be hash-chained or at minimum have a database-level write-once constraint

---

### [P1] Bulk Import: No Input Size Limits or File Type Validation Specified

**Threat:** D1, and injection via crafted Excel/CSV

**Component:** Bulk Import Pipeline, File Upload (Stage 1)

**Attack Vector (DoS):** A pool admin uploads a 10 GB Excel file, exhausting server memory during parsing (Excel parsers are notorious for loading entire workbooks into memory). Alternatively, a ZIP bomb embedded in an XLSX file (XLSX is a ZIP container) causes memory exhaustion.

**Attack Vector (Content Injection):** A maliciously crafted Excel file contains formula injection (cells starting with `=`, `+`, `-`, `@`) that executes when the import summary is opened by a pool admin in Excel. While this attacks the admin's workstation rather than the server, it can be used for credential harvesting. Additionally, XLSX files can contain macros or embedded objects.

**Mitigation:**
- Enforce a maximum upload file size (recommended: 100MB for Excel, 500MB for CSV)
- Detect and reject XLSX files containing macros or embedded objects before parsing
- Sanitize CSV/Excel cell values that begin with formula injection characters before inclusion in any exported output (the import summary Excel file)
- Use a streaming CSV parser; for Excel, use a SAX-based parser that does not require loading the full workbook into memory
- Validate MIME type by content inspection (magic bytes), not by file extension
- Apply the SHA-256 checksum (already specified for source files) to detect file corruption before processing

---

### [P1] NL Query Raw Input Stored as PII in Telemetry Without Retention or Scrubbing Policy

**Threat:** I6, I6b

**Component:** NL Query Telemetry, `NLQueryEvent.original_query`

**Attack Vector:** Users may embed personally identifiable information or sensitive exposure details in natural language queries (member names, specific addresses, dollar amounts, owner names). The `NLQueryEvent` stores the raw query text, and CentuRisk admins have access to this telemetry. This creates a PII aggregation risk: the telemetry store becomes a secondary repository of sensitive member portfolio intelligence that may not be subject to the same access controls as the primary exposure store.

**Mitigation:**
- Define and implement a retention policy for `NLQueryEvent` records (recommend 90-day rolling window for raw query text)
- Implement a PII detection pass before persisting `original_query`: detect and redact or tokenize names, addresses, ZIP codes, dollar amounts
- Access to raw query telemetry must be governed by Cedar policies with the same rigor as access to asset data
- Consider hashing or tokenizing `original_query` in analytics aggregations so pattern analysis can be done on tokens rather than raw text

---

### [P1] SOV Export Files: No Download URL Expiry or Access Control Mechanism Specified

**Threat:** I7

**Component:** SOV Generation Adapter, Export Download

**Attack Vector:** SOV exports contain full portfolio data for potentially thousands of assets including replacement costs, locations, construction details, and occupancy information. If generated files are stored with persistent, guessable, or shareable URLs (e.g., a static path on a CDN or an S3 key without a pre-signed TTL), a link shared via email to a broker can be forwarded to unauthorized parties indefinitely.

**Mitigation:**
- Generate time-limited pre-signed URLs for all export downloads (recommend 1-hour TTL)
- Store exported files in private object storage, not publicly accessible storage
- Implement single-use download tokens: after first download, the URL is invalidated
- Log every file download (user ID, timestamp, export scope) in the audit trail
- Delete export files from staging storage after download or after TTL expiry (whichever comes first)

---

### [P1] Renewal Flag Bypass: Bulk Approval State Check Must Be Atomic

**Threat:** T5

**Component:** Renewal Workflow, Bulk Approval

**Attack Vector:** A member initiates a bulk approval request. The system queries flag states for each asset, determines all are "clean," and begins processing approvals in a loop. A concurrent request from the pool admin resolves a flag as "open" on one of those assets between the eligibility check and the approval submission. The race condition allows a disputed asset to be bulk-approved despite having an open flag at the time of processing.

**Mitigation:**
- The bulk approval operation must re-check flag state atomically within the same database transaction as the approval insert — the eligibility check and the approval write must be a single atomic operation
- Use a database-level SELECT FOR UPDATE or optimistic locking (version counter on the flag record) to prevent the race
- If any flag state changes between eligibility check and approval write, abort the entire bulk approval and return a list of assets that became ineligible

---

### [P1] Access Grant Creation: Insufficient Specification of Who Can Create/Modify Grants

**Threat:** E4

**Component:** Access Grant Store, `AccessGrant` lifecycle

**Attack Vector:** The `AccessGrant` contract defines `granted_by: ActorID` but the ADR does not explicitly specify which roles are authorized to create or modify grants. If pool admins can create grants (plausibly implied by "pool administrators manage their pool's members"), a compromised pool admin account can create a new grant for an arbitrary `member_id` pointing to a pool they control, gaining access to that member's data.

**Mitigation:**
- Access grant creation must be a CentuRisk admin action only (not pool admin)
- Pool admins may be able to revoke a grant for a member currently in their pool (a natural pool management operation) but must not be able to create grants pointing to other pools
- Grant modifications must produce audit entries; alert on any grant creation or modification outside of the expected member onboarding/offboarding workflows
- Add Cedar policies governing the `createAccessGrant` and `revokeAccessGrant` actions with explicit principal constraints

---

### [P1] Loss Event Free-Text Field: Injection and Privacy Risk

**Threat:** Injection / I6 class

**Component:** Loss Event Intake, `LossEvent.description`

**Attack Vector:** The `LossEvent` schema includes a free-text `description` field. If this field is used in rendered views without output encoding (e.g., shown in the recommendation engine's context, pool admin dashboard, or future ML feature extraction), it is an XSS injection vector. Additionally, free-text descriptions of loss events may contain PII (names of individuals involved in incidents, witness information) that should not be accessible to the ML feature extraction pipeline without redaction.

**Mitigation:**
- Apply context-appropriate output encoding to all free-text fields before rendering (HTML-encode in browser contexts, escape in any template-based document generation)
- Implement a maximum length constraint on `description` (recommend 2,000 characters)
- Document that `description` may contain PII and apply appropriate data retention and access controls
- When the ML engine eventually consumes loss event data, `description` must go through a PII scrubbing step before feature extraction

---

## Recommended Security Controls

1. **Mandatory Cedar evaluation wrapper on all single-resource fetches** — Addresses I1 (IDOR); every repository method returning a single record by ID must invoke Cedar before returning data.

2. **Database-level row security as defense-in-depth** — Addresses I2 (cross-tenant leakage); PostgreSQL RLS or equivalent provides a second enforcement layer that catches application-level tenant filter omissions.

3. **Cryptographic hash chain on audit trail and mutation store** — Addresses T1, T2; tamper detection without relying solely on access control.

4. **Authoritative user attribute store (never trust external IdP for `user_category` or profiles)** — Addresses S2 (federated identity spoofing); the user database is the source of truth for category and profile assignments.

5. **Cedar `forbid` rules for profile management actions** — Addresses E1; the Cedar engine itself must reject profile creation/assignment by non-CentuRisk-admin principals, independent of UI restrictions.

6. **Pre-signed, time-limited, single-use export download URLs** — Addresses I7; prevents indefinite re-use of exported sensitive data links.

7. **Atomic flag state check within bulk approval transaction** — Addresses T5; prevents race condition between flag resolution and bulk approval.

8. **File upload size limits and streaming parsers for bulk import** — Addresses D1; prevents memory exhaustion from large or malicious files.

9. **NL query telemetry PII scrubbing and retention policy** — Addresses I6, I6b; prevents accumulation of sensitive data in the analytics store.

10. **Rate limiting on all state-changing endpoints** — Addresses S4 class, D4; SOV submissions, login attempts, NL queries, and approval actions should all have per-user rate limits.

11. **Cedar-aware export adapter authorization** — Addresses E6; export adapters must enforce Cedar field-level policies, not just entity-level access.

12. **Access grant creation restricted to CentuRisk admins** — Addresses E4; prevents a compromised pool admin from creating unauthorized cross-pool grants.

13. **Anomaly detection on Cedar policy changes and access grant modifications** — Addresses T3, E4; security-relevant administrative actions should trigger alerts for out-of-band review.

---

## Residual Risks After Mitigations

1. **Cedar policy authoring errors by CentuRisk admins** — Even with formal verification tooling, a CentuRisk admin can write a Cedar policy that unintentionally grants broader access than intended. Cedar's formal analysis tools reduce but do not eliminate this risk. Mitigation: mandatory peer review for any Cedar policy change; automated conflict detection on policy deployment.

2. **Compromised CentuRisk admin account** — A CentuRisk admin has the highest privilege level in the system. If their account is compromised (phishing, credential stuffing, insider threat), the attacker has cross-pool read access and can modify Cedar policies. Mitigation: enforce MFA for all CentuRisk admin logins, implement privileged access workstation requirements, apply time-limited just-in-time (JIT) access elevation rather than standing admin permissions; audit all admin actions in real time.

3. **Rule-based NL translation ceiling** — The NL layer cannot handle novel phrasings; however, failed queries are logged. An attacker who systematically probes the NL layer's failure modes to map the synonyms associated with restricted field names gains intelligence about the schema. Mitigation: NL failure telemetry must require the same Cedar authorization level as the data the queries target; admin review of unresolved query logs must apply the same access controls.

4. **Email digest as a secondary exfiltration channel** — Notification content in digest emails is a reduced-form copy of sensitive system data transmitted through an external channel (email). If a user's email account is compromised, the attacker gains access to quality alerts, approval status, and renewal summaries without authenticating to CentuRisk. Mitigation: minimize content in digest emails (deep links only, no inline data values); consider whether any digest content is sensitive enough to warrant S/MIME signing.

5. **Phase-1 broker access deferral** — Broker access is explicitly deferred but the architecture (view-only users with Cedar policies) is designed to accommodate it. When broker access is eventually added, the security model must be re-reviewed at that time. The risk is that the "add a named profile" simplicity understates the security complexity of granting external, non-public-sector parties access to sensitive exposure data.

---

## Monitoring Recommendations

1. **Cedar policy evaluation deny rate per user** — Alert condition: a user's deny rate spikes above baseline; may indicate an IDOR probe or privilege escalation attempt.
2. **Cross-tenant query attempt detection** — Alert condition: any query reaching the database layer without a `TenantContext` filter (detectable via query plan analysis or an application-level assertion); this should be a zero-tolerance alert.
3. **Access grant creation and modification events** — Alert condition: any `AccessGrant` created or modified outside the expected onboarding/offboarding workflow hours, or by a pool admin principal.
4. **Cedar policy change events** — Alert condition: any policy file modification; all changes must be reviewed by a second CentuRisk admin within 24 hours.
5. **Bulk import file upload size and type anomalies** — Alert condition: upload exceeds 90% of the configured size limit, or MIME type does not match the file extension.
6. **Unusual export generation activity** — Alert condition: a single user generates more than N SOV exports in a 24-hour period, or an export covers a scope (member count, asset count) significantly larger than that user's historical baseline.
7. **Authentication failure rate** — Alert condition: more than 10 failed login attempts for a single account within 5 minutes (brute force indicator); more than 100 failed logins from a single IP in 10 minutes (credential stuffing indicator).
8. **Audit trail hash chain verification failures** — Alert condition: any hash chain mismatch detected; this is a P0 security incident requiring immediate investigation.
9. **NL query telemetry access by CentuRisk staff** — Alert condition: any CentuRisk analyst or support user queries telemetry data scoped to a pool they are not assigned to; log and review.
10. **`auto_approve` flag changes on user profiles** — Alert condition: any change to a user's `auto_approve` flag; these changes directly affect approval bypass for non-valuation submissions and warrant oversight.

---

## ADR Gap Summary (Issues That Should Be Documented in ADRs)

The following security requirements are implied or partially addressed in the ADRs but should be explicitly specified to ensure implementation correctness. These are not new requirements — they are clarifications to existing ADRs.

**ADR_centurisk-access-control.md:**
- Explicitly specify that Cedar is evaluated on every single-resource fetch (not only on searches/lists)
- Explicitly specify that `user_category` and `cedar_profile_ids` are always sourced from CentuRisk's user database, never from external IdP token claims
- Explicitly specify that Cedar `forbid` rules govern profile creation and assignment as a server-side enforcement, not just UI restriction

**ADR_centurisk-infrastructure.md:**
- Specify file size and type validation requirements for the bulk import upload stage
- Specify that export download URLs must be time-limited (pre-signed TTL) and single-use
- Specify that pool-federated IdP claims are only used for user lookup (by `sub`), not for role/profile assignment

**ADR_centurisk-system-overview.md:**
- Define who is authorized to create and modify `AccessGrant` records (CentuRisk admin only for creation; pool admin for revocation of their own pool's grants only)
- Specify that audit trail "immutability" is enforced at the storage layer (append-only or hash chain), not only at the application layer

**ADR_centurisk-nl-querying.md:**
- Specify a retention policy for `NLQueryEvent.original_query` raw text
- Specify a PII scrubbing step before persisting raw query text to telemetry

**ADR_centurisk-sov-pipeline.md:**
- Explicitly state that the `auto_approve` flag is read from the server-side user record at routing time, never from a client-supplied parameter
- Specify that bulk approval eligibility re-checks flag state atomically within the approval transaction

**ADR_centurisk-recommendation-engine.md:**
- Specify that `Recommendation` records reference the rule ID and rule version that generated them (for repudiation resistance and audit purposes)
agentId: a3dce3f5089cec736 (use SendMessage with to: 'a3dce3f5089cec736' to continue this agent)
<usage>total_tokens: 80727
tool_uses: 11
duration_ms: 463794</usage>
