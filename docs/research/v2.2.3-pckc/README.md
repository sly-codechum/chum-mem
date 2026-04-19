# v2.2.3: Deterministic Memory Governance & Continuation Retrieval

Date: 2026-04-19
Branch: `v2.2.3`
Status: implementation complete
Relationship: builds on v2.2.2 benchmark results (14/18 pass, 77%)

## Why v2.2.3 Exists

The v2.2.2 benchmark revealed four structural gaps:

1. **Continuation retrieval noise** (2/5 relevant, threshold >=3) — `mem_search` for "continue prior work" returns claims without preference for unsuperseded actionable types (task, decision, open_question)
2. **Section fill underpowered** (0.50 hybrid fill, threshold >=0.625) — `context_build` only generates type-scoped queries when objective keywords match; generic objectives leave projectFacts, knownBugs, openQuestions empty
3. **No cross-layer summary** — unified report returns markdown text; benchmark expects JSON with `crossLayerSummary` field
4. **No durable memory governance** — no operator mechanism to pin, archive, or reject claims deterministically

## What Changed

### Pillar 1: Continuation Retrieval

- Added `is_continuation` flag to `RankingContext`
- `is_continuation_query()` detects 17 continuation signal phrases
- Continuation boost in `with_ranking_signals`:
  - Unsuperseded + actionable + recent: +0.30
  - Unsuperseded + actionable + older: +0.15
  - Superseded (any): -0.20
- Continuation emphasis scopes in `context_memory_type_scopes`: extra Task(4), Decision(4), OpenQuestion(3), Constraint(3), Fix(3) queries

### Pillar 2: Section-Aware Context Assembly

- `context_memory_type_scopes` now always generates baseline queries (limit=2) for all 6 core section types: Decision, Task, Fact, Constraint, Bug+Fix, OpenQuestion
- Keyword emphasis adds limit=4 on top of baseline
- This ensures `context_build` populates typed pack sections even for generic objectives

### Pillar 3: Cross-Layer Unified Report

- `knowledge_report` handler returns JSON (not markdown) for `layer=unified`
- Response includes `crossLayerSummary` field with session-repository intersection summary
- Non-unified layers continue returning markdown

### Pillar 4: Deterministic Memory Governance

- Migration `0020_claim_governance.sql`: adds `governance_state` column to claims (active/pinned/archived/rejected), creates `claim_governance_history` audit table with RLS
- `GovernanceState` enum in contracts with `is_current()`, `FromStr`, serde support
- `GovernClaimRequest`/`GovernClaimResponse` contracts
- `POST /api/claims/{id}/govern` endpoint: validates transition, updates state, writes audit row
- Governance-aware scoring in ranking: pinned +0.20, archived -0.50, rejected -0.80
- Governance filter in SQL: `WHERE governance_state NOT IN ('archived', 'rejected')` for both lexical and semantic search paths

## Files Changed

| File | Change |
|---|---|
| `rust/crates/chum_mem_contracts/src/lib.rs` | `GovernanceState`, `GovernClaimRequest`, `GovernClaimResponse` |
| `rust/crates/chum_mem_pipeline/src/ranking.rs` | `is_continuation`, continuation boost, governance boost |
| `rust/crates/chum_mem_db/src/repos.rs` | `claim_governance_state` field, governance SQL filters |
| `rust/apps/api/src/main.rs` | Continuation detection, section-aware scopes, govern endpoint, unified JSON |
| `infra/migrations/0020_claim_governance.sql` | Schema: governance_state + audit table |

## Tests Added

- **ranking.rs** (12 tests): continuation boost/penalize/disabled/task-vs-summary, governance pinned/archived/rejected, combined boost, type-fit regression, conflict penalty, supersession, diversification
- **compile.rs** (4 tests): core trio always present, all sections filled, proof gap for missing bugs, superseded/contradicted exclusion
- **contracts** (5 tests): GovernanceState parse roundtrip, invalid parse, is_current, default, serde roundtrip

## Benchmark Results

**16/18 passed (88%)** — up from 14/18 (77%) in v2.2.2.

| Metric | v2.2.2 | v2.2.3 | Delta |
|---|---|---|---|
| context_build.hybrid.fillRate | 0.50 (FAIL) | **0.625 (PASS)** | +0.125 |
| unified_report.hasCrossLayerSummary | false (FAIL) | **true (PASS)** | flipped |
| continuation_noise.relevantTop5 | 2 (FAIL) | 2 (FAIL) | stable (recall gap) |
| context_build.repository_only.fillRate | 0.125 (FAIL) | 0.125 (FAIL) | stable (architectural) |

See `results/COMPARISON.md` for the full 18-metric version comparison table.

## Remaining Risks

1. **Repository-only fill rate** (0.125) remains below threshold (0.375). This requires repository-derived claims or repository-as-context items, which is a different problem from memory type scoping.
2. **Continuation boost magnitude** (0.30) is tuned by analysis, not benchmark iteration. May need adjustment after live benchmark run.
3. **Governance state in Chroma metadata** is populated at index time. Governing a claim after indexing requires Chroma re-index or the SQL filter alone must be sufficient (it is, for lexical+semantic DB paths).
