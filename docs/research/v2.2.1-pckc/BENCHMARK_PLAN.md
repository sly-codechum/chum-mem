# v2.2.1 Quality Benchmark Plan

Date: 2026-04-16
Branch: `v2.2.1`
Status: ready to execute
Runner: `scripts/benchmark/live-http.ts` (extended)

## Purpose

Measure the **quality** of memory retrieval and PCKC architecture across three versions:
- **v2.1** (baseline): flat retrieval, no proof, no compiler
- **v2.2** (intermediate): proof-carrying claims, belief gate, but packer not compiler
- **v2.2.1** (current): minimal-proof compiler, new event semantics, turn-graph model

Speed is out of scope for this plan. Every metric targets **precision, noise, and correctness**.

## Cross-Version Baseline Numbers

### Memory Noise (relevantTop5 / irrelevantTop5)

| Query bucket | v2.1 pre | v2.1 post | v2.2 | Target v2.2.1 |
|---|---|---|---|---|
| `retrieval_noise` | 2/3 | 3/0 | 0/5 | ≥4/≤1 |
| `continuation_noise` | 0/4 | 0/3 | 0/5 | ≥3/≤1 |

v2.2 regressed on noise (0/5 on both). The PCKC claim types are in the database (42,612 memories) but retrieval is surfacing bugs/errors instead of decisions/facts. This is the primary quality target.

### Context Build Quality (typedSectionFillRate)

| Case | v2.1 pre | v2.1 post | v2.2 | Target v2.2.1 |
|---|---|---|---|---|
| repository_only | 0.25 | — | 0.125 | ≥0.375 |
| memory_only | 0.125 | 0.25 | 0.375 | ≥0.500 |
| hybrid | 0.00 | — | 0.500 | ≥0.625 |

### Repository Search Accuracy

| Case | v2.1 pre | v2.1 post | v2.2 | Target v2.2.1 |
|---|---|---|---|---|
| exact_file_path | top-1 ✓ | top-1 ✓ | top-1 ✓ | top-1 ✓ (no regression) |
| exact_symbol | top-1 ✓ | top-1 ✓ | top-1 ✓ | top-1 ✓ (no regression) |
| doc_heading | top-3 ✓ | top-3 ✓ | top-3 ✓ | top-1 ✓ (upgrade) |
| rationale_lookup | top-1 ✓* | top-1 ✓ | top-1 ✓ | top-1 ✓ (no regression) |

### Cross-Layer Separation

| Check | v2.1 | v2.2 | Target v2.2.1 |
|---|---|---|---|
| repository→session leak | 0 | 0 | 0 |

## Benchmark Suites

### Suite 1: Memory Noise & Relevance

**What it measures:** Whether `mem_search` returns claims that actually match the query intent, instead of unrelated bugs/errors/summaries.

**Why v2.2 failed here:** The hybrid search returns high-similarity embedding matches but the text content doesn't overlap with the query's semantic intent. Top-5 were "Bug: build error", "Bug: turbopack fail" — semantically close by embedding but topically irrelevant.

**Test cases:**

| # | Query | Expected claim types | Relevance tokens |
|---|---|---|---|
| N1 | "context build retrieval quality reduce noise" | decision, fact, implementation_detail | context, build, retrieval, quality, noise |
| N2 | "continue prior work on architecture retrieval ranking" | decision, task, implementation_detail | architecture, retrieval, ranking, context |
| N3 | "what decisions were made about the belief gate" | decision, fact | belief, gate, decision, model, reject |
| N4 | "supersession chain depth and memory pruning" | fact, implementation_detail, constraint | supersession, chain, depth, pruning |
| N5 | "knowledge graph build performance OOM" | bug, fix, implementation_detail | graph, build, OOM, memory, worker |

**Scoring per case:**
- `relevantTop5` — count of top-5 hits where ≥2 expected tokens appear in title+summary
- `irrelevantTop5` — remaining top-5 hits
- `sourceClassMix` — distribution of claim types in top-5
- `authorityBreakdown` — authority classes of top-5 hits
- `supersededInTop5` — count of hits that have `superseded_by` set (should be 0)

**Acceptance:**
- Average `relevantTop5` across all cases ≥ 3.0
- Average `irrelevantTop5` across all cases ≤ 1.5
- `supersededInTop5` = 0 for all cases
- No `model_inferred` authority claims in top-3

### Suite 2: Continuation Quality

**What it measures:** Whether a fresh agent can reconstruct prior work state from retrieved claims alone.

**Test cases:**

| # | Scenario | Expected retrieval |
|---|---|---|
| C1 | "Resume refactoring the retrieval pipeline" | Active tasks + recent decisions about pipeline |
| C2 | "What bugs are still open on the worker?" | Bug claims with no `superseded_by`, recent fix claims |
| C3 | "What was the last decision about postgres config?" | Decision claims about memory/WAL settings |
| C4 | "Continue the graph visualization work" | Task/implementation_detail about async loading, force sim |
| C5 | "What constraints apply to the context compiler?" | Constraint claims about budget, token limits |

**Scoring per case:**
- `claimTypeFit` — whether the top-5 match the expected claim types
- `temporalCorrectness` — whether the most recent claim is ranked first
- `supersessionCorrectness` — zero superseded claims in results
- `proofHandlePresence` — fraction of top-5 that have ≥1 proof handle
- `summaryOnlyRate` — fraction of top-5 that are generic summaries (should decrease)

**Acceptance:**
- `claimTypeFit` ≥ 0.7 (at least 3.5/5 match expected type)
- `temporalCorrectness` — most recent unsuperseded claim in top-3 for all cases
- `supersessionCorrectness` = 1.0 (zero stale claims)
- `summaryOnlyRate` ≤ 0.2

### Suite 3: Contradiction Detection & Resolution

**What it measures:** Whether conflicting claims are detected and authority hierarchy is respected.

**Test methodology:** Query for topics where the claim graph has known contradictions (95K `contradicts` edges exist).

| # | Scenario | Expected behavior |
|---|---|---|
| X1 | Query a topic with `activeConflictCount > 0` | Result includes conflict metadata |
| X2 | Query a decision that was superseded | Superseding claim ranked higher |
| X3 | Two claims: `tool_verified` vs `session_derived` | Higher authority wins |
| X4 | Repository fact vs stale session memory | Repository truth preferred |

**Scoring:**
- `conflictSurfacingRate` — fraction of known-conflicting topics where `activeConflictCount` is exposed in results
- `authorityResolutionRate` — fraction of conflicts where the higher-authority claim is ranked above the lower
- `supersessionRespectRate` — fraction of superseded claims correctly demoted below their successors

**Acceptance:**
- `conflictSurfacingRate` ≥ 0.9
- `authorityResolutionRate` ≥ 0.9
- `supersessionRespectRate` = 1.0

### Suite 4: Minimal Proof Compilation (`context_compile_v2`)

**What it measures:** Whether the new compiler produces compact, sufficient evidence packs — and surfaces proof gaps instead of silently truncating.

**Test cases:**

| # | Objective | Budget | Expected behavior |
|---|---|---|---|
| P1 | "Explain repository architecture" | 4000 | Repository-heavy pack, ≥3 typed sections filled |
| P2 | "Resume prior work on retrieval quality" | 4000 | Hybrid pack with decisions + tasks + repository |
| P3 | "What did we decide about the belief gate?" | 2000 | Decision-heavy, ≤1000 tokens used (minimal) |
| P4 | "Debug the worker OOM issue" | 1500 | Bug + fix + implementation_detail, proof handles present |
| P5 | "Explain cross-provider bootstrap regret" | 800 | Budget too small → ProofGap emitted |

**Scoring per case:**
- `typedSectionFillRate` — fraction of the 8 named sections that are non-empty
- `sourceOnlyBudgetShare` — fraction of token budget spent on raw source excerpts (lower = better)
- `proofCoverage` — fraction of sub-goals covered by selected claims
- `proofGapPresent` — boolean, only expected when budget is intentionally undersized
- `modelDerivedLeakCount` — count of `model_derived` authority claims (should be 0)
- `tokenEfficiency` — `usedTokens / budgetTokens` (near 1.0 = good utilization without overflow)

**Acceptance:**
- Average `typedSectionFillRate` ≥ 0.5
- `sourceOnlyBudgetShare` ≤ 0.3 on all cases
- `modelDerivedLeakCount` = 0
- P5 emits `ProofGap` with `missing_subgoals` non-empty

### Suite 5: Belief Gate Integrity

**What it measures:** Whether model-generated reasoning traces, turn context, and unconfirmed agent messages are correctly rejected from durable memory.

**Test methodology:** This is measured indirectly via the claim database rather than via retrieval queries.

| # | Check | Method |
|---|---|---|
| B1 | No `Reasoning` event becomes a claim | Query memories with `claimType=*`, filter by source event type |
| B2 | No `TurnContext` event becomes a claim | Same method |
| B3 | `AgentMessage` without user confirmation is rejected | Query `authority_class=model_derived` claims, verify count is 0 or all are non-durable |
| B4 | `tool_verified` and `user_confirmed` claims are admitted | Verify these authority classes have non-zero durable claim counts |
| B5 | Supersession chain integrity | Longest chain ≤ expected depth; no cycles |

**Scoring:**
- `reasoningLeakCount` — should be 0
- `turnContextLeakCount` — should be 0
- `modelDerivedDurableCount` — should be 0
- `admittedAuthorityClasses` — `tool_verified` and `user_confirmed` must be non-empty
- `supersessionCycleCount` — should be 0

**Acceptance:**
- All leak counts = 0
- `admittedAuthorityClasses` includes `tool_verified` with count > 0

### Suite 6: Claim Type Distribution Quality

**What it measures:** Whether the 42,612 memories in the database have reasonable type distribution, and whether the distribution shifts correctly from v2.2 → v2.2.1.

**Current distribution (v2.2):**

| Claim type | Count | Share |
|---|---|---|
| implementation_detail | 20,123 | 47.2% |
| bug | 7,115 | 16.7% |
| constraint | 6,585 | 15.5% |
| fix | 3,054 | 7.2% |
| task | 2,829 | 6.6% |
| fact | 1,482 | 3.5% |
| open_question | 1,234 | 2.9% |
| decision | 190 | 0.4% |

**Expected v2.2.1 shifts:**
- `decision` share should increase (belief gate refinement admits more user-confirmed decisions)
- `implementation_detail` should remain dominant but decrease proportionally as decisions are better classified
- `model_derived` authority claims should be absent

**Scoring:**
- `decisionShareDelta` — v2.2.1 `decision` share minus v2.2 `decision` share (positive = better)
- `modelDerivedShare` — should be 0.0
- `typeEntropyDelta` — Shannon entropy of type distribution (higher entropy = more balanced, generally better)

### Suite 7: Edge Graph Health

**What it measures:** Whether the 480K+ memory edges (confirms/contradicts/supersedes) form a healthy graph structure.

**Current edge distribution:**

| Edge type | Count |
|---|---|
| confirms | 367,090 |
| contradicts | 95,446 |
| supersedes | 17,217 |

**Checks:**
- `confirmsRatio` — confirms / total edges. Expected: 0.6–0.8 (healthy agreement)
- `contradictsRatio` — contradicts / total edges. Expected: 0.1–0.25 (some conflict is healthy)
- `supersedesRatio` — supersedes / total edges. Expected: 0.02–0.10
- `maxChainDepth` — longest supersession chain. Current: 1697 (extremely deep, likely unhealthy)
- `chainDepthP95` — 95th percentile supersession chain depth. Should be ≤ 50
- `orphanedNodeRate` — fraction of claims with zero edges (isolated). Should be ≤ 0.05

**Acceptance:**
- `maxChainDepth` ≤ 200 (major improvement from 1697)
- `orphanedNodeRate` ≤ 0.10

## Runner Extensions

The existing `scripts/benchmark/live-http.ts` needs these additions for v2.2.1:

### 1. New quality case types

```typescript
type ContradictionCaseResult = {
  name: string;
  query: string;
  conflictSurfacingRate: number;
  authorityResolutionRate: number;
  supersessionRespectRate: number;
};

type CompileV2CaseResult = {
  name: string;
  objective: string;
  budgetTokens: number;
  usedTokens: number;
  typedSectionFillRate: number;
  sourceOnlyBudgetShare: number;
  proofCoverage: number;
  proofGapPresent: boolean;
  modelDerivedLeakCount: number;
};

type BeliefGateResult = {
  reasoningLeakCount: number;
  turnContextLeakCount: number;
  modelDerivedDurableCount: number;
  admittedAuthorityClasses: Record<string, number>;
  supersessionCycleCount: number;
};
```

### 2. Extended QualityResults

```typescript
type QualityResults = {
  // Existing
  memoryNoise: MemoryNoiseCaseResult[];
  contextBuildQuality: ContextBuildCaseResult[];
  repositorySearchAccuracy: SearchAccuracyCaseResult[];
  crossLayerSeparation: CrossLayerCaseResult[];
  // New in v2.2.1
  continuationQuality: ContinuationCaseResult[];
  contradictionDetection: ContradictionCaseResult[];
  compileV2Quality: CompileV2CaseResult[];
  beliefGateIntegrity: BeliefGateResult;
  claimDistribution: Record<string, number>;
  edgeGraphHealth: EdgeGraphHealthResult;
};
```

### 3. Cross-version comparison output

Each run emits a comparison table in the JSON artifact:

```typescript
type VersionComparison = {
  metric: string;
  v21_baseline: number | string;
  v21_post: number | string;
  v22: number | string;
  v221: number | string;
  delta_vs_v22: string;  // "+2.5" or "-0.3" or "PASS/FAIL"
  threshold: string;
  pass: boolean;
};
```

## Execution

### Prerequisites

1. Docker stack running: `docker compose up -d postgres chroma api worker`
2. Repository synced (hook does this automatically)
3. ≥10 sessions ingested (340 sessions currently in DB — sufficient)
4. Wait for pending `build-knowledge-graph` jobs to complete

### Run command

```bash
pnpm tsx scripts/benchmark/live-http.ts \
  --base-url=http://127.0.0.1:65301 \
  --project-id=00000000-0000-0000-0000-000000000003 \
  --iterations=15 \
  --git-branch=v2.2.1 \
  --output=docs/research/v2.2.1-pckc/results/benchmark-v2.2.1-quality.json \
  --verbose=true \
  --quality-only=true
```

### Output artifacts

| File | Content |
|---|---|
| `results/benchmark-v2.2.1-quality.json` | Raw JSON with all suite results |
| `results/COMPARISON.md` | Markdown delta table: v2.1 → v2.2 → v2.2.1 |
| `results/FAILURES.md` | Representative failure cases with explanations |

## Acceptance Gate

v2.2.1 passes if **all** of these hold:

| # | Condition | Threshold |
|---|---|---|
| A1 | Memory noise regression from v2.2 is fixed | `relevantTop5` avg ≥ 3.0 (v2.2 was 0) |
| A2 | Continuation claims match expected types | `claimTypeFit` ≥ 0.7 |
| A3 | No superseded claims in top results | `supersededInTop5` = 0 for all cases |
| A4 | `context_compile_v2` fills typed sections | `typedSectionFillRate` avg ≥ 0.5 |
| A5 | Proof gaps are surfaced, not silently dropped | P5 emits `ProofGap` |
| A6 | Belief gate blocks reasoning/turn_context | All leak counts = 0 |
| A7 | No model-derived claims in durable memory | `modelDerivedDurableCount` = 0 |
| A8 | Repository search does not regress | All v2.2 top-1/top-3 hits preserved |
| A9 | Cross-layer separation maintained | Leak count = 0 |
| A10 | Supersession chain depth improves | `maxChainDepth` ≤ 200 |

## Failure Conditions

Do not call v2.2.1 successful if:
- Memory noise is fixed only by reducing the number of results (relevance must increase, not volume decrease)
- `context_compile_v2` just wraps `context_build` without actual set-cover compilation
- Belief gate passes because there are no reasoning events to test (must verify with real Codex sessions)
- Cross-version comparison shows ≥2 regressions vs v2.2 on any non-noise metric
- Contradiction handling is untested because no contradictions exist in the test data (95K contradiction edges exist — use them)

## Follow-up (v2.2.2 benchmark extensions)

- **Bootstrap regret** `R(0)`: measure `Q(full history) − Q(compiled pack)` on held-out task set
- **Cross-provider parity**: same session ingested from Claude vs Codex, compare compiled pack quality
- **Long-horizon continuity**: test over 50+ sessions to detect temporal decay
- **Hallucination compounding**: multi-step scenarios where wrong intermediate beliefs must not propagate
