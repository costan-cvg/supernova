# 8. Cross-Cutting Capabilities

## 8.1 Natural Language Querying

Natural language querying is a search index with an NL translation layer, not an LLM. It runs entirely within the system boundary. The NL layer maps user phrasing to the search index's query syntax and returns results in contextually appropriate formats. The search index includes pool-specific custom fields; assets with empty custom field values are excluded from results for queries against those fields. When the NL layer cannot confidently map a query, it suggests alternative phrasings. The system measures suggestion effectiveness — logging unresolved queries, acceptance rates, and outcomes — so CentuRisk admins can improve the mapping over time. The NL layer emits structured events for this telemetry, following the same pattern as quality events.

## 8.2 Access Control — ABAC with Cedar Policy

Access control is Attribute-Based (ABAC), expressed using Cedar Policy language, and enforced uniformly across every interface in the system. The original design described three separate mechanisms (role-based access, data scoping, field-level visibility). These are unified into a single Cedar policy engine where all three are patterns of attribute-based rules. Access decisions evaluate attributes of the user, the resource (down to individual field level), and the context (hierarchy path, effective date, approval state).

The user model supports four categories: CentuRisk users (system admins and operational staff across pools), pool administrators, member users, and view-only users scoped to any hierarchy level from pool-wide down to individual asset fields. Cedar policies are aliased to named profiles for convenience — "Pool Analyst" or "CentuRisk Field Auditor" is a human-readable name for a Cedar policy. Administrators assign profiles; CentuRisk admins create custom profiles by composing Cedar policies.

Field-level visibility is enforced at the search index. If a Cedar policy does not grant a user access to a field, that field is completely invisible — not queryable, not in results. Every interface (NL query, search, rendering, exports) evaluates the same Cedar policies.
