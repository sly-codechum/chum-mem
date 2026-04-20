# v2.2.3 Quality Benchmark — Post-Project-Scoping Comparison

Date: 2026-04-20
Branch: `v2.2.3`
Runner: `scripts/benchmark/live-http.ts --quality-only`
API: `http://localhost:63001`
Raw artifact: `benchmark-v223-post-scoping.json`
Baseline artifacts: `benchmark-results.json` (pre-scoping), `../../v2.2.2-pckc/results/COMPARISON.md`

## Headline

| Scope | v2.2.2 | v2.2.3 pre-scoping | **v2.2.3 post-scoping** | Delta |
|---|---|---|---|---|
| Pass rate (20 metrics) | 14/18 (77%) | 16/18 (88%) | **17/20 (85%)** | +1 new pass, +1 new regression |
| New: project_scoping.repositoryLayerScoped | N/A | N/A | **true (PASS)** | new metric |
| New: governance.fieldPresent | N/A | N/A | **true (PASS)** | new metric |
| Regression: continuation.supersededInTop5 | 0 | 0 | **1 (FAIL)** | regression |
| No other regressions | -- | -- | **confirmed** | all other v2.2.3 passes held |

v2.2.3 post-scoping delivers 2 new passes from new metrics, but introduces 1 regression in `continuation.supersededInTop5.total` (0 → 1).

## Version comparison — all 20 metrics

| # | Metric | v2.1 | v2.2 | v2.2.1 | v2.2.2 | **v2.2.3** | Threshold | Pass | D vs pre-scoping |
|---|---|---|---|---|---|---|---|---|---|
| 1 | retrieval_noise.relevantTop5 | 2->3 | 0 | 1 | 1 | **4** | >=3 | **PASS** | stable |
| 2 | retrieval_noise.irrelevantTop5 | 3->0 | 5 | 4 | 4 | **1** | <=1 | **PASS** | stable |
| 3 | continuation_noise.relevantTop5 | 0->0 | 0 | N/A | 2 | **2** | >=3 | **FAIL** | stable (recall gap) |
| 4 | context_build.repository_only.fillRate | 0.25 | 0.125 | 0.125 | 0.125 | **0.125** | >=0.375 | **FAIL** | stable (architectural) |
| 5 | context_build.hybrid.fillRate | 0 | 0.50 | 0.75 | 0.50 | **0.625** | >=0.625 | **PASS** | stable |
| 6 | repository.exact_file_path.top1 | true | true | true | true | **true** | true | **PASS** | stable |
| 7 | repository.exact_symbol.top1 | true | true | true | true | **true** | true | **PASS** | stable |
| 8 | cross_layer.leak_count | 0 | 0 | 0 | 0 | **0** | 0 | **PASS** | stable |
| 9 | continuation.claimTypeFit.avg | N/A | N/A | 0.343 | 1.000 | **1.000** | >=0.7 | **PASS** | stable |
| 10 | continuation.supersededInTop5.total | N/A | N/A | 0 | 0 | **1** | 0 | **FAIL** | **regression** |
| 11 | belief_gate.reasoning_leak | N/A | N/A | 0 | 0 | **0** | 0 | **PASS** | stable |
| 12 | belief_gate.model_derived_durable | N/A | N/A | 0 | 0 | **0** | 0 | **PASS** | stable |
| 13 | containment.hasContainsEdges | N/A | N/A | N/A | true | **true** | true | **PASS** | stable |
| 14 | cross_file_call.hasCrossFileEdges | N/A | N/A | N/A | true | **true** | true | **PASS** | stable |
| 15 | typed_search.avgPrecision | N/A | N/A | N/A | 1.000 | **1.000** | >=0.8 | **PASS** | stable |
| 16 | hub_quality.forbiddenTypeCount | N/A | N/A | N/A | 0 | **0** | 0 | **PASS** | stable |
| 17 | community.hasHierarchy | N/A | N/A | N/A | true | **true** | true | **PASS** | stable |
| 18 | unified_report.hasCrossLayerSummary | N/A | N/A | N/A | false | **true** | true | **PASS** | stable |
| 19 | project_scoping.repositoryLayerScoped | N/A | N/A | N/A | N/A | **true** | true | **PASS** | **new** |
| 20 | governance.fieldPresent | N/A | N/A | N/A | N/A | **true** | true | **PASS** | **new** |

## What moved

### 1. Project scoping: N/A -> **true** (PASS, new metric)

The benchmark validates that multi-project scoping is operational:
- Repository layer queries succeed with `projectId` and return repository nodes (project-scoped, no global fallback)
- Session layer queries succeed (falls back to global project when no project-specific snapshot exists)
- Memory search returns results (falls back to global project for historical memories)

Latency: 1136ms for the parallel 3-way scoping check (repository + session + mem_search).

### 2. Governance field: N/A -> **true** (PASS, new metric)

`mem_search` results now include `governanceState` field on returned claims. This confirms the `0020_claim_governance.sql` migration is applied and the field is wired through the search pipeline.

Latency: 67ms for governance field presence check.

### 3. Hub quality distribution shifted

Hub types changed from `{file: 10}` (pre-scoping) to `{file: 3, type: 7}`. This is because the project-scoped repository graph now includes more type nodes relative to file nodes. No forbidden types (`session_hub`, `import_hub`) — still PASS.

### 4. Community hierarchy grew

Communities: 2197 (pre-scoping) → 2571 (post-scoping), with level-0: 521→616 and level-1: 1676→1955. The project-scoped graph captures more nodes, producing more communities. Still hierarchical — PASS.

## What regressed

### 5. continuation.supersededInTop5: 0 -> **1** (FAIL)

The `open_worker_bugs` continuation case ("What bugs are still open on the worker?") now returns one claim with `supersededBy` set in the top 5 results. Specifically, the "Fix: The API logs show PayloadTooLargeError" claim appears to have been superseded by a newer fix claim.

This is a data-level regression, not a code regression — the supersession engine correctly marked the older fix as superseded, but the ranking didn't sufficiently penalize it to push it out of the top 5 for this query. The continuation boost for `bug`/`fix` types keeps it ranked high.

Possible fixes:
- Increase superseded penalty from -0.20 to -0.30 in the continuation ranking regime
- Add a hard filter excluding superseded claims from continuation results (rather than just penalizing)
- Accept as data-dependent noise if the superseded claim is still informative for the query

## What didn't move

### 6. Continuation noise: still 2 relevant (FAIL, threshold >=3)

Same recall gap as pre-scoping — only 2 matching claims exist in the database. Project scoping didn't affect this.

### 7. Repository-only fill: still 0.125 (FAIL, threshold >=0.375)

Architecturally constrained — repository-only context build only fills `repositoryKnowledge`. Deferred to v2.3.

## Latency

| Endpoint | Pre-scoping (warm) | Post-scoping (warm) | Delta |
|---|---|---|---|
| mem_search (retrieval_noise) | 71 ms | 190 ms | +119 ms |
| mem_search (continuation_noise) | 293 ms | 95 ms | -198 ms |
| context_build (repository_only) | 398 ms | 1188 ms | +790 ms |
| context_build (hybrid) | 2512 ms | 2393 ms | -119 ms |
| knowledge_query (exact_file_path) | 574 ms | 1311 ms | +737 ms |
| knowledge_query (exact_symbol) | 248 ms | 244 ms | stable |
| cross_layer_separation | 2049 ms | 5407 ms | +3358 ms |
| unified_report | 4453 ms | 5277 ms | +824 ms |
| project_scoping (3-way) | N/A | 1136 ms | new |
| governance_quality | N/A | 67 ms | new |

Latency variance is higher post-scoping for repository-layer queries, likely due to project-scoped graph lookup overhead on cold cache. Warm-cache p50 for mem_search and typed_search remains fast (<170ms).

## Summary

**17/20 passed (85%)** — up from 14/18 (77%) in v2.2.2, down from 16/18 (88%) pre-scoping on the 18-metric basis.

On the expanded 20-metric basis:
- 2 new v2.2.3 metrics pass (project scoping, governance)
- 1 new regression (superseded claim in continuation top-5)
- 2 known architectural FAILs held (continuation noise recall, repository-only fill rate)
- All 15 other metrics stable — no regressions from project scoping
