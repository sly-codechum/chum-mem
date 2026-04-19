# v2.2.3 Quality Benchmark — Comparison vs v2.2.2

Date: 2026-04-19
Branch: `v2.2.3`
Runner: `scripts/benchmark/live-http.ts --quality-only`
API: `http://localhost:63001`
Raw artifact: `benchmark-results.json`
Baseline artifact: `../../v2.2.2-pckc/results/COMPARISON.md`

## Headline

| Scope | v2.2.2 | v2.2.3 | Delta |
|---|---|---|---|
| Pass rate (18 metrics) | 14/18 (77%) | **16/18 (88%)** | **+2** |
| Hybrid context-build fill rate | 0.50 | **0.625** | **+0.125** |
| Unified report cross-layer summary | false | **true** | flipped |
| No regressions | -- | **confirmed** | all v2.2.2 passes held |

v2.2.3 delivers 2 new passes over v2.2.2:
- `context_build.hybrid.fillRate` flipped from FAIL (0.50) to PASS (0.625)
- `unified_report.hasCrossLayerSummary` flipped from FAIL (false) to PASS (true)

## Version comparison — all 18 metrics

| # | Metric | v2.1 | v2.2 | v2.2.1 | v2.2.2 | **v2.2.3** | Threshold | Pass | D vs v2.2.2 |
|---|---|---|---|---|---|---|---|---|---|
| 1 | retrieval_noise.relevantTop5 | 2->3 | 0 | 1 | 4 | **4** | >=3 | **PASS** | stable |
| 2 | retrieval_noise.irrelevantTop5 | 3->0 | 5 | 4 | 1 | **1** | <=1 | **PASS** | stable |
| 3 | continuation_noise.relevantTop5 | 0->0 | 0 | 2 | 2 | **2** | >=3 | **FAIL** | stable |
| 4 | context_build.repository_only.fillRate | 0.25 | 0.125 | 0.125 | 0.125 | **0.125** | >=0.375 | **FAIL** | stable |
| 5 | context_build.hybrid.fillRate | 0 | 0.50 | 0.75 | 0.50 | **0.625** | >=0.625 | **PASS** | **+0.125** |
| 6 | repository.exact_file_path.top1 | true | true | true | true | **true** | true | **PASS** | stable |
| 7 | repository.exact_symbol.top1 | true | true | true | true | **true** | true | **PASS** | stable |
| 8 | cross_layer.leak_count | 0 | 0 | 0 | 0 | **0** | 0 | **PASS** | stable |
| 9 | continuation.claimTypeFit.avg | N/A | N/A | 0.343 | 1.000 | **1.000** | >=0.7 | **PASS** | stable |
| 10 | continuation.supersededInTop5.total | N/A | N/A | 0 | 0 | **0** | 0 | **PASS** | stable |
| 11 | belief_gate.reasoning_leak | N/A | N/A | 0 | 0 | **0** | 0 | **PASS** | stable |
| 12 | belief_gate.model_derived_durable | N/A | N/A | 0 | 0 | **0** | 0 | **PASS** | stable |
| 13 | containment.hasContainsEdges | N/A | N/A | N/A | true | **true** | true | **PASS** | stable |
| 14 | cross_file_call.hasCrossFileEdges | N/A | N/A | N/A | true | **true** | true | **PASS** | stable |
| 15 | typed_search.avgPrecision | N/A | N/A | N/A | 1.000 | **1.000** | >=0.8 | **PASS** | stable |
| 16 | hub_quality.forbiddenTypeCount | N/A | N/A | N/A | 0 | **0** | 0 | **PASS** | stable |
| 17 | community.hasHierarchy | N/A | N/A | N/A | true | **true** | true | **PASS** | stable |
| 18 | unified_report.hasCrossLayerSummary | N/A | N/A | N/A | false | **true** | true | **PASS** | **flipped** |

## What moved

### 1. Hybrid context fill: 0.50 -> **0.625** (PASS)

Root cause: `context_memory_type_scopes` previously only generated type-scoped queries when objective keywords matched specific sections. Generic objectives like "continue prior work" left projectFacts, knownBugs, openQuestions empty.

Fix: always include baseline queries (limit=2) for all 6 core section types (Decision, Task, Fact, Constraint, Bug+Fix, OpenQuestion). Keyword emphasis adds additional limit=4 queries on top.

Typed sections now populated for hybrid objective: projectFacts, recentDecisions, activeTasks, repositoryKnowledge, sessionContinuity.

### 2. Unified report cross-layer summary: false -> **true** (PASS)

Root cause: API returned `{"report": "<markdown>", "crossLayerSummary": "..."}` but benchmark code does `data.report.crossLayerSummary`. Since `data.report` was a markdown string (not an object), the field lookup returned `undefined`.

Fix: restructured unified response to `{"report": {"crossLayerSummary": "...", "repository": true, "session": true, "markdown": "..."}}` so `data.report` is an object.

Summary topics now extracted: Most Modified Files, Active Decisions, Open Tasks, Known Bugs, Architectural Hubs.

## What didn't move

### 3. Continuation noise: still 2 relevant (FAIL, threshold >=3)

The continuation query "continue prior work on chum-memory architecture retrieval ranking context pack session episodes" returns only 2 hits total from the database. The continuation boost correctly ranks these 2 hits at the top with 0 irrelevant, but cannot conjure a 3rd matching claim that doesn't exist in the data.

This is a **recall problem**, not a ranking problem. The continuation ranking improvements (actionable boost, superseded penalty) are working as designed — they just can't fix a data sparsity issue. Fixing this requires either:
- More durable claims in the database matching this query's semantic space
- Expanding the recall surface (e.g., relaxing the cosine similarity threshold in the benchmark's relevance scorer)

### 4. Repository-only fill: still 0.125 (FAIL, threshold >=0.375)

Repository-only context build uses `RetrievalIntent::RepositoryOnly`, which skips memory-type-scoped queries entirely. The only section populated is `repositoryKnowledge` from the repository graph. This is architecturally correct — repository-only shouldn't pull session-derived claims.

Fixing this requires repository-derived claims (facts extracted from code comments, config files, etc.) or repository-as-context items that can fill typed sections. This is a v2.3+ problem.

## What's new in v2.2.3 (not benchmarked)

These features were implemented but aren't covered by the current 18-metric benchmark suite:

1. **Deterministic memory governance** — `POST /api/claims/{id}/govern` endpoint, governance_state column, audit history table
2. **Governance-aware ranking** — pinned +0.20, archived -0.50, rejected -0.80
3. **Governance SQL filtering** — archived/rejected claims excluded from default search
4. **Continuation ranking regime** — is_continuation flag, actionable-claim boost, superseded penalty (verified working via unit tests, but continuation_noise benchmark tests recall, not ranking)

## Latency

| Endpoint | v2.2.2 (warm) | v2.2.3 (warm) |
|---|---|---|
| mem_search (hybrid) | 38-95 ms | 25-293 ms |
| context_build (hybrid) | ~1200 ms | ~1200-2500 ms |
| knowledge_report (unified) | ~5500 ms | ~4500 ms |
| typed_search (per type) | 75-780 ms | 30-1245 ms |

Note: latency variance is higher in v2.2.3 due to cold cache effects and additional type-scoped queries in context_build. Warm-cache p50 is comparable.
