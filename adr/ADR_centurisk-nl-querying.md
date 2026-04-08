# ADR: CentuRisk Natural Language Querying

## Status
Proposed

## Context

CentuRisk users need to query exposure data across pools, members, and assets using natural language — asking questions like "show me all buildings in Dallas with replacement cost over $5M" or "which members haven't updated their vehicle schedules since 2024." The system manages complex hierarchical data with pool-specific custom fields, and users range from CentuRisk analysts (sophisticated) to member staff (less technical).

Key constraints:
- No user queries may leave the system boundary. This is internal infrastructure managing sensitive exposure data — no external LLM calls.
- Pool-specific custom fields must be indexed and searchable alongside standard fields.
- Access control (Cedar policies) must be enforced at the index level. If a user cannot see a field, they cannot query against it and it does not appear in results.
- When the system cannot interpret a query, it must suggest alternatives rather than returning empty errors. The effectiveness of suggestions must be measurable.
- CentuRisk admins need visibility into what users are searching for and what fails, to improve the system over time.

## Decision

Implement natural language querying as a **search index with an NL-to-query-syntax translation layer**. This is explicitly NOT an LLM. The NL layer is a deterministic mapping engine that parses user phrasing and translates it into structured queries against the search index.

### Architecture

```
User Input (natural language)
       ↓
NL Translation Layer
  - Parse intent and entities
  - Map to search index query syntax
  - Evaluate confidence score
       ↓
  [High confidence]          [Low confidence]
       ↓                          ↓
Cedar Policy Filter         Suggest alternatives
  - Remove non-visible       - Return ranked suggestions
    fields from query         - Log as unresolved
  - Scope to user's           - Emit NLQueryEvent
    accessible data
       ↓
Search Index Execution
  - Query against indexed exposure data
  - Custom fields included
  - Empty custom field values excluded
       ↓
Results (filtered by Cedar)
       ↓
User
```

### Boundary Contract

From the boundary summary:

```
NL Query → Exposure Views
Direction: Query adapter translates; view logic executes
Contract: Structured query derived from natural language input
```

The NL query adapter produces a structured query. It does not directly access the exposure core — it queries the search index, which is a read-optimized projection of core data.

### NL Translation Layer

The NL layer maps user phrasing to the search index's query syntax. It is a rule-based translation engine, not a statistical model. It handles:

- **Entity extraction** — identifying asset types, field names, member names, locations, date ranges from user input.
- **Intent classification** — determining if the user wants to filter, aggregate, compare, or list.
- **Synonym resolution** — mapping common terms to canonical field names (e.g., "building value" maps to `replacement_cost`, "TIV" maps to `total_insured_value`).
- **Query construction** — assembling extracted entities and intent into the search index's query syntax.

The translation layer maintains a mapping registry that includes:
- Standard field synonyms and aliases
- Pool-specific custom field names and their common alternative phrasings
- Common query patterns (e.g., "show me X where Y" maps to a filter query)

### Custom Field Indexing

Pool-specific custom fields are included in the search index. When pool administrators add or modify custom fields, the index is updated to include them.

Rules for custom fields in search:
- Custom fields are queryable using their configured display name and any configured aliases.
- Assets with **empty** custom field values are **excluded from results** for queries targeting those fields. Empty values are not surfaced as blanks — if a user queries "show me all assets with seismic zone," only assets with a populated seismic zone value appear.
- The NL layer's synonym registry must be updated when custom fields are added or renamed.

### Low-Confidence Handling

When the NL layer cannot confidently map a user's query to a structured search, it:

1. **Does NOT return an empty error or "no results found."**
2. **Suggests alternative phrasings** — ranked by similarity to the original query, each representing a query the system CAN confidently execute.
3. **Logs the event** — emits an `NLQueryEvent` for telemetry.

The confidence threshold is configurable. Queries below the threshold trigger the suggestion flow; queries above it execute directly.

### Effectiveness Telemetry

The NL layer emits structured events following the same pattern as `QualityEvent`:

```
NLQueryEvent {
  query_id:            unique identifier
  user_id:             requesting user (for access-scoped analysis)
  pool_id:             pool context (if applicable)
  original_query:      the user's raw input
  event_type:          "resolved" | "unresolved" | "suggestion_offered"
  confidence_score:    float (0.0 - 1.0)
  mapped_query:        the structured query produced (if resolved)
  suggestions:         list of alternative phrasings (if unresolved)
  timestamp:           event time
}

NLSuggestionEvent {
  query_id:            links to the originating NLQueryEvent
  suggestion_index:    which suggestion was selected (or null if rejected all)
  accepted:            boolean
  result_count:        number of results from the accepted suggestion (if any)
  user_action:         "accepted" | "rejected_all" | "rephrased_manually"
  timestamp:           event time
}
```

These events feed a telemetry pipeline that tracks:
- **Unresolved query rate** — what percentage of queries cannot be mapped.
- **Suggestion acceptance rate** — how often suggested alternatives are accepted.
- **Suggestion effectiveness** — whether accepted suggestions produce useful results (non-zero result count).
- **Common failure patterns** — recurring phrasings the system cannot handle, grouped by pool and user category.

CentuRisk admins can review unresolved query logs and use them to improve the NL mapping — adding new synonyms, query patterns, or field aliases. This is a manual observe-and-adjust cycle in Phase 1, not an automated learning system.

### Access Control Integration

Cedar policies are enforced at the search index level, not as a post-query filter:

1. **Before query execution**, the Cedar engine evaluates which fields the requesting user can access.
2. **Non-visible fields are removed from the query** — if a user queries "show me replacement costs for Dallas buildings" but their Cedar policy does not grant access to `replacement_cost`, the query is rewritten to exclude that field (and the user is informed the field is not accessible).
3. **Results are filtered** — only fields the user has access to appear in results.
4. **The NL layer is Cedar-aware** — it does not suggest queries involving fields the user cannot see. Suggested alternatives only reference accessible fields.

This means the same natural language query executed by two users with different Cedar profiles may produce different structured queries and different results — by design.

## Alternatives Considered

### LLM-Based Query Translation
Using a large language model to interpret natural language and generate queries. Rejected because: queries would leave the system boundary (privacy and security concern for sensitive exposure data), LLM responses are non-deterministic (same query could produce different results), hosting costs are significant, and the query patterns in CentuRisk are domain-specific enough that a rule-based approach with a good synonym registry will cover the majority of cases.

### Free-Text Search Only (No NL Layer)
Providing a standard search box with keyword matching against indexed fields. Rejected because: users need to express complex filter/aggregate/compare intents that keyword search cannot handle ("buildings over $5M in counties with flood risk"), and the system needs to guide users when their queries don't match — keyword search just returns empty results.

### SQL-Like Query Language Exposed to Users
Giving users a structured query language (simplified SQL or custom DSL). Rejected because: the user base includes member staff who are not technical, and a query language creates a learning barrier. The NL layer is meant to lower that barrier. Power users who need precise queries can use the search index's query syntax directly as an advanced option.

### Embedding-Based Semantic Search
Using vector embeddings to find semantically similar queries from a pre-built query library. Rejected for Phase 1 because: it adds infrastructure complexity (vector database, embedding model), the query patterns are sufficiently structured that rule-based mapping works, and it introduces a dependency on an embedding model that may need to leave the system boundary. Could be reconsidered in a future phase if the rule-based approach proves insufficient.

## Consequences

**Positive:**
- All query processing stays within the system boundary — no privacy or security concerns from external API calls.
- Deterministic query mapping means identical inputs produce identical outputs, making debugging and auditing straightforward.
- Effectiveness telemetry creates a feedback loop for continuous improvement of the NL mapping.
- Access control is enforced at the index level, eliminating data leakage through search.
- Pool-specific custom fields are first-class citizens in search, matching the system's extensibility model.

**Negative / Trade-offs:**
- Rule-based NL mapping has a ceiling — queries outside the mapping registry will fail. Mitigation: the suggestion flow handles failures gracefully, and telemetry identifies gaps for admin intervention.
- Maintaining the synonym registry and query pattern library is an ongoing operational task. Each new pool with custom fields requires updates.
- The NL layer cannot handle truly ambiguous or novel phrasings. Users must adapt to phrasing patterns the system understands, or rely on suggestions.
- Cedar-aware query rewriting adds complexity to the query path. Every query now involves a Cedar evaluation step before index execution.

**New Constraints:**
- The search index must support per-user field-level filtering (same constraint as access control ADR).
- The NL translation layer must be updated whenever pool-specific custom fields are added or renamed.
- Telemetry events must be retained long enough for CentuRisk admins to review and act on patterns.
- The confidence threshold must be tuned per deployment — too low and users get wrong results, too high and users get too many suggestion prompts.

## Implementation Plan

1. **Search index with standard fields** — Stand up the search index populated from exposure core data. Index standard fields (asset type, location, replacement cost, member, pool, etc.). Verify basic structured queries return correct results. This is the foundation the NL layer translates into.

2. **NL translation layer with core query patterns** — Implement the rule-based NL parser with support for common query patterns: filter by field value, filter by range, filter by location, list/count aggregation. Build the synonym registry for standard fields. Test: "show me all buildings in Dallas" translates to the correct structured query and returns results.

3. **Low-confidence handling and suggestions** — Implement the confidence scoring and suggestion flow. When a query cannot be confidently mapped, generate and return ranked alternative phrasings. Test: an ambiguous query returns suggestions; selecting a suggestion executes successfully.

4. **Telemetry event emission** — Emit `NLQueryEvent` and `NLSuggestionEvent` for every query interaction. Build the admin view showing unresolved query patterns, acceptance rates, and effectiveness metrics. Test: execute a mix of successful and failed queries, verify telemetry captures the full lifecycle.

5. **Custom field indexing** — Extend the search index to include pool-specific custom fields. Update the NL layer's synonym registry to include custom field names and aliases. Test: query against a pool-specific custom field returns results; query against an empty custom field value excludes that asset from results.

6. **Cedar policy integration at index level** — Integrate Cedar evaluation into the query path. Before executing a query, evaluate the requesting user's field-level policies and modify the query scope accordingly. Ensure suggested alternatives only reference fields the user can access. Test: two users with different Cedar profiles execute the same NL query and receive appropriately different results.

7. **Admin mapping management** — Build the interface for CentuRisk admins to review unresolved query logs, add synonyms and query patterns, and see the impact of their changes on resolution rates. This closes the observe-and-adjust feedback loop.
