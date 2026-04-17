# v2.2.1 Quality Benchmark Results

Date: 2026-04-16
Branch: `v2.2.1`
Runner: `scripts/benchmark/live-http.ts --quality-only`
API: `http://127.0.0.1:63001`
Raw artifact: `benchmark-v2.2.1-quality.json`

## Version Comparison: v2.1 → v2.2 → v2.2.1

| # | Metric | v2.1 | v2.2 | v2.2.1 | Threshold | Pass |
|---|---|---|---|---|---|---|
| 1 | retrieval_noise.relevantTop5 | 2→3 | 0 | **1** | ≥3 | **FAIL** |
| 2 | retrieval_noise.irrelevantTop5 | 3→0 | 5 | **4** | ≤1 | **FAIL** |
| 3 | continuation_noise.relevantTop5 | 0→0 | 0 | **2** | ≥3 | **FAIL** |
| 4 | context_build.repository_only.fillRate | 0.25 | 0.125 | **0.125** | ≥0.375 | **FAIL** |
| 5 | context_build.hybrid.fillRate | 0.00 | 0.50 | **0.75** | ≥0.625 | **PASS** |
| 6 | repository.exact_file_path.top1 | true | true | **true** | true | **PASS** |
| 7 | repository.exact_symbol.top1 | true | true | **true** | true | **PASS** |
| 8 | cross_layer.leak_count | 0 | 0 | **0** | 0 | **PASS** |
| 9 | continuation.claimTypeFit.avg | N/A | N/A | **0.343** | ≥0.7 | **FAIL** |
| 10 | continuation.supersededInTop5.total | N/A | N/A | **0** | 0 | **PASS** |
| 11 | belief_gate.reasoning_leak | N/A | N/A | **0** | 0 | **PASS** |
| 12 | belief_gate.model_derived_durable | N/A | N/A | **0** | 0 | **PASS** |

**Result: 7/12 passed** (58%)

## Improvements (v2.2 → v2.2.1)

1. **Continuation noise improved**: 0/5 → 2/0 relevant/irrelevant (only 2 hits returned, but both relevant)
2. **Hybrid context build fill rate**: 0.50 → 0.75 (6/8 typed sections filled — best ever)
3. **Memory-only context fill rate**: 0.375 → 0.50 (4 sections: projectFacts, recentDecisions, activeTasks, knownBugs)
4. **Belief gate integrity**: All checks pass — 0 reasoning leaks, 0 model-derived durable claims
5. **Supersession correctness**: 0 superseded claims in any top-5 result
6. **Repository search**: No regression — file path and symbol lookups remain exact top-1 hits

## Remaining Gaps

### 1. Retrieval Noise (FAIL)

**retrieval_noise**: relevantTop5=1, irrelevantTop5=4

The top-5 results for the retrieval quality query were:
1. "Decision: memoryNoise [...]" — relevant (it's about noise metrics)
2. "Fix: repository-only context routing is fixed" — marginally relevant
3. "Implementation detail: repository: payload.repository.full_name" — irrelevant
4. "Implementation detail: 3. run session-aware hybrid retrieval" — relevant text but low token overlap
5. "Task: contextBuildQuality [...]" — relevant but scored as irrelevant by token-overlap metric

**Root cause**: The token-overlap relevance heuristic (`countOverlap >= 2`) is too strict for claim titles that are structured differently from the query. The "Fix: repository-only context routing is fixed" result is topically relevant to "retrieval quality reduce noise" but doesn't share enough literal tokens.

**Recommendation**: Adjust the relevance scorer to use semantic similarity (even cosine on bag-of-words) rather than pure token intersection.

### 2. Continuation Claim Type Fit (FAIL)

**claimTypeFit avg = 0.343** (target: ≥0.7)

Examples:
- "Resume refactoring the retrieval pipeline" → got `open_question` × 2 (expected `task`/`decision`)
- "What bugs are still open on the worker?" → got `open_question` + `constraint` (expected `bug`/`fix`)
- "What was the last decision about postgres config?" → got `constraint` × 2 + `open_question` (expected `decision`/`fact`)

**Root cause**: The `mem_search` hybrid retrieval ranks by embedding similarity without filtering by claim type. The `types` parameter in the benchmark cases was not being passed — the runner queries without type constraints. Additionally, the queries are broad enough that tangentially-related claims from other projects/contexts (research instruments, pentesting tasks) appear.

**Recommendation**:
1. Pass `types` filter in `mem_search` to constrain by expected claim types
2. Improve embedding quality — current embeddings don't distinguish "postgres config decision" from generic "constraint" claims about other topics
3. Add project-scoped retrieval to avoid cross-project leakage

### 3. Repository-Only Context Fill Rate (FAIL)

**typedSectionFillRate = 0.125** (only `repositoryKnowledge`)

The `context_build` endpoint with a repository-only objective fills only 1 of 8 sections. This is the same as v2.2.

**Root cause**: `context_build` correctly routes to repository-only mode but only populates the `repositoryKnowledge` section. The other 7 sections (projectFacts, recentDecisions, etc.) are memory-backed and correctly empty for a pure repository query.

**Recommendation**: This may be a measurement problem, not a quality problem. A repository-only query *should* have 0 memory sections. The threshold needs adjustment for this case — measure section fill rate only against sections that *should* be filled for the given intent.

### 4. `context_compile_v2` Fill Rate (Low)

The new compiler (`context_compile_v2`) exists and responds, but fill rates are low:
- "Explain repository architecture" → 0.125 (repositoryKnowledge only)
- "Resume prior work on retrieval quality" → 0.125 (activeTasks only)
- "Belief gate decisions" → 0.25 (activeTasks, conflicts)
- "Debug worker OOM" → 0.50 (knownBugs, repositoryKnowledge, sessionContinuity, conflicts) — best
- "Budget too small" → 0.375 (no ProofGap emitted)

The P5 "budget too small" case did NOT produce a `ProofGap` as expected — it used 797/800 tokens and filled 3 sections. The compiler packed right up to the budget without reporting what was left out.

## Contradiction Detection

| Case | Hits | With conflicts | Authority resolution | Supersession respect |
|---|---|---|---|---|
| knowledge graph build performance | 6 | 1 (17%) | 100% | 100% |
| postgres shared_buffers settings | 6 | 3 (50%) | 100% | 100% |
| worker OOM fix approach | 3 | 0 (0%) | 50% | 100% |

Conflict surfacing is partial — some topics surface `activeConflictCount > 0` but not all conflicting topics do. Authority resolution is correct when conflicts are visible.

## Belief Gate Integrity (PASS)

All checks passed:
- `reasoningLeakCount` = 0 (no reasoning traces became claims)
- `turnContextLeakCount` = 0 (no turn context became claims)
- `modelDerivedDurableCount` = 0 (no model-derived authority in durable memory)
- Admitted authority classes: `user_confirmed` (2), `tool_verified` (2)

This validates the v2.2.1 belief gate design: reasoning and turn context are correctly hard-rejected at the claim extractor.

## Claim Distribution

The claim distribution check returned low counts (totalClaims=2) because the `types` filter + empty query returned minimal results from the search endpoint. This suite needs to use a direct database query or aggregate across multiple searches to get accurate distribution numbers.

## Summary

| Area | Status | Direction |
|---|---|---|
| Retrieval noise | Improved but below target | v2.2 (0) → v2.2.1 (1) ↑ |
| Continuation noise | Improved materially | v2.2 (0/5) → v2.2.1 (2/0) ↑↑ |
| Hybrid context build | **Passes threshold** | v2.2 (0.50) → v2.2.1 (0.75) ↑↑ |
| Memory context build | Improved | v2.2 (0.375) → v2.2.1 (0.50) ↑ |
| Repository search | No regression | Stable ✓ |
| Cross-layer separation | No regression | Stable ✓ |
| Belief gate | **Passes all checks** | New ✓ |
| Supersession correctness | **Passes** | New ✓ |
| Continuation type fit | Below target | New metric, 0.343 (target 0.7) |
| Contradiction detection | Partial | New metric, needs work |

### Next Steps

1. **Improve retrieval relevance scoring** — swap token-overlap for semantic similarity in the benchmark scorer
2. **Add `types` filter to continuation queries** — the runner should pass expected claim types to `mem_search`
3. **Implement ProofGap emission** in `context_compile_v2` when budget is exhausted before coverage
4. **Add project-scoped retrieval** to prevent cross-project claim leakage in continuation queries
5. **Fix claim distribution measurement** — use aggregated queries or direct DB counts
