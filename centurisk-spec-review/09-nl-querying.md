# 09. Natural Language Querying (Resolved)

Natural language querying allows users to ask questions about their exposure portfolio in plain English. Three decisions were resolved covering the underlying technology, custom field support, and low-confidence handling.

## 1. Technology: Search Index, Not LLM

**Question:** Is this backed by an LLM, a rule-based parser, or something else?

**Answer:** This is essentially a search index with a natural language phrasing that needs to map to a search query syntax. Restrictions need to be in place to limit access to results and indexes.

**Decision:** Natural language querying is backed by a search index with an NL-to-query-syntax translation layer, not an LLM. The NL layer parses user phrasing and maps it to structured queries against the index. Access control restrictions apply to both the index (which records are searchable by a given user) and the results (which records are returned). No user queries leave the system boundary — this is entirely internal infrastructure. This dramatically reduces the technical complexity, hosting cost, and privacy concerns compared to an LLM-based approach.

## 2. Custom Fields: Indexed and Searchable

**Question:** Does the search index need to include pool-specific custom fields?

**Answer:** Yes. Users can search for custom fields, but if they don't have values they would not appear in the results.

**Decision:** The search index includes pool-specific custom fields. Users can query against them. Assets with empty custom field values are excluded from results for queries targeting those fields — empty values are not surfaced as blanks. The index must be updated when custom fields are added or modified by pool administrators.

## 3. Low-Confidence Handling: Suggest Alternatives, Measure Effectiveness

**Question:** What happens when the NL layer can't confidently map a user's phrasing to a structured query?

**Answer:** Suggesting alternatives is ideal, but we need to measure the effectiveness of the suggestions to ensure our users aren't being further frustrated by the lack of search results.

**Decision:** When the NL layer cannot confidently map a query, it suggests alternative phrasings rather than returning an empty error. The system must track the effectiveness of this flow — logging unresolved queries, suggestion acceptance/rejection rates, and whether suggested alternatives produce useful results. This telemetry feeds the observe-and-adjust principle.

**Spec Implications:**

- The NL layer emits structured events for failed queries and suggestion interactions (same pattern as QualityEvents).
- There is a feedback signal when a suggestion leads to a successful result versus further failure.
- Centurisk admins can review unresolved query logs to improve the NL mapping rules over time.
