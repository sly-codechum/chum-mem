# v2.2.3 Benchmark Plan

Date: 2026-04-20
Branch: `v2.2.3`
Status: post-project-scoping, pending re-benchmark

## Baseline

v2.2.2 benchmark: 14/18 pass (77%)
v2.2.3 pre-project-scoping: 16/18 pass (88%)

## What Changed (code-level)

### Phase 1: Governance & Continuation (already benchmarked)

- **Continuation retrieval**: `is_continuation` flag on `RankingContext`, `is_continuation_query()` detects 17 continuation signal phrases. Unsuperseded actionable claims boosted (+0.30 recent, +0.15 older), superseded claims penalized (-0.20). Continuation emphasis scopes add extra Task(4), Decision(4), OpenQuestion(3), Constraint(3), Fix(3) queries.
- **Section-aware context assembly**: `context_memory_type_scopes` always generates baseline queries (limit=2) for all 6 core section types (Decision, Task, Fact, Constraint, Bug+Fix, OpenQuestion). Keyword emphasis adds limit=4 on top.
- **Cross-layer unified report**: `knowledge_report` returns JSON (not markdown) for `layer=unified`, with `crossLayerSummary` field containing session-repository intersection summary.
- **Deterministic memory governance**: Migration `0020_claim_governance.sql` adds `governance_state` column (active/pinned/archived/rejected) and `claim_governance_history` audit table with RLS. `POST /api/claims/{id}/govern` endpoint validates transitions, updates state, writes audit row.
- **Governance-aware ranking**: pinned +0.20, archived -0.50, rejected -0.80. SQL filter excludes archived/rejected from default search paths.

### Phase 2: Multi-Project Scoping (new, needs benchmarking)

- **Dynamic project resolution**: `.chum-mem` file and `POST /v1/projects/resolve` endpoint for runtime project discovery
- **Repository layer strictly per-project**: `projectId` required on repository queries, no global fallback
- **Session layer per-project with global fallback**: session queries filter by project first, fall back to global project when no project-specific snapshot exists
- **Memory search per-project with global fallback**: `mem_search` filters by project first, falls back to global project
- **Project-scoped graph view**: merged session snapshots scoped to project
- **Hook auto-exports `CHUM_MEM_PROJECT_ID`**: plugin hooks receive project context automatically

## Benchmark Steps

### Step 1: Verify Project Scoping

- `knowledge_query` on repository layer should work with `projectId`
- `knowledge_query` on repository layer should fail without `projectId`
- Session layer should fall back to global project when no project-specific snapshot exists
- `mem_search` should fall back to global project

### Step 2: Re-run Full Benchmark

```bash
npx tsx scripts/benchmark/live-http.ts \
  --base-url=http://localhost:63001 \
  --quality-only \
  --git-branch=v2.2.3 \
  --output=docs/research/v2.2.3-pckc/results/benchmark-v223-post-scoping.json
```

### Step 3: Expected Improvements

| Metric | v2.2.2 | Pre-scoping v2.2.3 | Expected Post-scoping | Mechanism |
|---|---|---|---|---|
| retrieval_noise.relevantTop5 | 4 | 4 | 4 (hold) | Project scoping narrows result set but same data |
| retrieval_noise.irrelevantTop5 | 1 | 1 | 1 (hold) | Project filter may reduce noise further |
| continuation_noise.relevantTop5 | 2 | 2 | 2 (hold) | Recall gap, not affected by project scoping |
| context_build.repository_only.fillRate | 0.125 | 0.125 | 0.125 (hold) | Architectural limit, deferred to v2.3 |
| context_build.hybrid.fillRate | 0.50 | 0.625 | 0.625 (hold) | Section baselines already solved this |
| repository.exact_file_path.top1 | true | true | true (hold) | Project scope adds filter, doesn't change ranking |
| repository.exact_symbol.top1 | true | true | true (hold) | Same as above |
| cross_layer.leak_count | 0 | 0 | 0 (hold) | Project boundaries strengthen isolation |
| continuation.claimTypeFit.avg | 1.000 | 1.000 | 1.000 (hold) | Type-fit unaffected by project scoping |
| continuation.supersededInTop5.total | 0 | 0 | 0 (hold) | Supersession logic unchanged |
| belief_gate.reasoning_leak | 0 | 0 | 0 (hold) | Belief gate unchanged |
| typed_search.avgPrecision | 1.000 | 1.000 | 1.000 (hold) | Type filtering unchanged |
| unified_report.hasCrossLayerSummary | false | true | true (hold) | Unified JSON structure unchanged |

Key: project scoping shouldn't regress anything. The main validation is that all 16 existing passes hold with project-scoped queries active.

### Step 4: New Benchmark Cases

1. **Project scoping validation**: `knowledge_query(repository)` succeeds with `projectId`
2. **Session fallback**: `knowledge_query(session)` returns data even without project-specific session snapshot
3. **Memory fallback**: `mem_search` returns results via global fallback
4. **Governance field presence**: `governanceState` field present in `mem_search` results

### Step 5: Regression Checks

All 16 existing passes must hold, especially:
- `repository.exact_file_path.top1` = true
- `repository.exact_symbol.top1` = true
- `cross_layer.leak_count` = 0
- `belief_gate.reasoning_leak` = 0
- `continuation.supersededInTop5.total` = 0
- `typed_search.avgPrecision` >= 0.8

## Success Criteria

Target: 18/20 pass (90%), up from 16/18 (88%)

- All 16 prior passes maintained (no regression)
- 2 new project-scoping + governance metrics pass
- 2 known FAILs remain (`continuation_noise` recall gap, `repository_only` fill rate -- architectural, deferred to v2.3)
