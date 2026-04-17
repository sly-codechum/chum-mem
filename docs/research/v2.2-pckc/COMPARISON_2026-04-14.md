# v2.2 PCKC Comparison

Date: 2026-04-14  
Branch: `v2.2`  
Benchmark harness: `scripts/benchmark/live-http.ts`  
Docker target: `docker compose` services `postgres`, `chroma`, `api`, `worker`, `web`  
Baseline artifact: `docs/research/v2.2-pckc/results/pre-change-live.json`  
Post-change artifact: `docs/research/v2.2-pckc/results/post-change-live.json`

## Reproduction

1. `docker compose build api worker web`
2. `docker compose up -d postgres chroma api worker web`
3. wait for `http://127.0.0.1:65301/ready`
4. sync the current repository:

```bash
bash plugins/chum-memory/scripts/sync.sh /Workspace/chum-memory http://127.0.0.1:65301
```

5. run the benchmark:

```bash
pnpm tsx scripts/benchmark/live-http.ts \
  --base-url=http://127.0.0.1:65301 \
  --project-id=00000000-0000-0000-0000-000000000003 \
  --iterations=8 \
  --concurrency=6 \
  --concurrency-iterations=3 \
  --git-branch=v2.2 \
  --output=docs/research/v2.2-pckc/results/post-change-live.json
```

## Dataset

- Repository corpus: current checked-out `chum-memory` worktree after live repository sync.
- Repository sync result after the final v2.2 runtime change:
  - `filesAdded`: 1
  - `filesUnchanged`: 1106
  - `totalFiles`: 1107
  - `nodeCount`: 14709
  - `edgeCount`: 39181
  - `communityCount`: 182
- Session fixture corpus used for live validation:
  - old repository-debugging decision
  - new repository-debugging constraint
  - wrong chroma-timeout hypothesis
  - verified correction and fix
  - proof-carrying task plus open question

## Before/After Latency

### Sequential p50

| Endpoint | Baseline | Post-change | Result |
|---|---:|---:|---|
| `mem_search` | `5.9ms` | `5.4ms` | slightly faster |
| `context_build` | `265.7ms` | `285.1ms` | slightly slower |
| `knowledge_query(hub_nodes)` | `245.4ms` | `256.0ms` | slightly slower |
| `knowledge_report` | `265.1ms` | `250.3ms` | faster |

### Concurrent p50

| Endpoint | Baseline | Post-change | Result |
|---|---:|---:|---|
| `mem_search` | `9.1ms` | `10.0ms` | slightly slower |
| `knowledge_query(hub_nodes)` | `2877.4ms` | `2848.5ms` | slightly faster |
| `knowledge_report` | `2546.3ms` | `3030.6ms` | slower |

## Context Build Quality

| Case | Baseline | Post-change | Result |
|---|---|---|---|
| Repository-only | `repositoryKnowledge`; `310` tokens | `repositoryKnowledge`; `227` tokens | same section quality, lower token use |
| Memory-only | `knownBugs` + `sessionContinuity`; source-only share `0.92` | `knownBugs`; source-only share `1.00` | regressed on benchmark query |
| Hybrid | `repositoryKnowledge` + `sessionContinuity`; source-only share `0.44`; `721` tokens | `repositoryKnowledge` + `sessionContinuity`; source-only share `0.00`; `194` tokens | materially better token discipline |

## Retrieval Noise

| Query bucket | Baseline | Post-change | Result |
|---|---|---|---|
| `retrieval_noise` | relevant top-5 `3`, irrelevant top-5 `0` | relevant top-5 `3`, irrelevant top-5 `0` | unchanged quality, much lower latency |
| `continuation_noise` | relevant top-5 `0`, irrelevant top-5 `3` | relevant top-5 `0`, irrelevant top-5 `3` | unchanged, still poor |

## Repository Search Accuracy

| Case | Baseline | Post-change | Result |
|---|---|---|---|
| Exact file path | top-1 exact | top-1 exact | preserved |
| Exact symbol | top-1 exact | top-1 exact | preserved |
| Doc heading | top-3 hit | top-3 hit | preserved |
| Rationale lookup | top-1 exact | top-1 exact | preserved and faster |

## Cross-Layer Separation

| Check | Baseline | Post-change | Result |
|---|---|---|---|
| Repository/session leak count | `0` | `0` | preserved |
| Repository top node types | symbol-dominant | symbol-dominant | preserved |

## Live Validation

Manual JSON-RPC against `POST /mcp` after `initialize` confirmed the new proof-carrying pieces on the server side.

### 1. Atomic claim retrieval with proof handles

Query:

```json
{
  "name": "mem_search",
  "arguments": {
    "query": "repository debugging knowledge_query first grep fallback",
    "mode": "hybrid",
    "disclosureLevel": "full",
    "limit": 5,
    "provider": "codex"
  }
}
```

Observed top hit:

```json
{
  "title": "Constraint: repository debugging must use knowledge_query first and grep only as fallback.",
  "type": "constraint",
  "authorityClass": "user_confirmed",
  "verificationStatus": "user_confirmed",
  "proofHandles": 1
}
```

### 2. Belief-gated correction beats earlier hypothesis

Query:

```json
{
  "name": "mem_search",
  "arguments": {
    "query": "chroma timeout debugging drift summary-heavy session memory",
    "mode": "hybrid",
    "disclosureLevel": "full",
    "limit": 5,
    "provider": "codex"
  }
}
```

Observed top hit:

```json
{
  "title": "Fact: Verified result: debugging drift persists with chroma disabled; the issue is summary-heavy session memory, not chroma timeout.",
  "type": "fact",
  "authorityClass": "test_verified",
  "verificationStatus": "verified",
  "proofHandles": 1
}
```

This is the strongest v2.2 runtime improvement: an unverified hypothesis no longer dominates the verified correction.

### 3. Memory-only residual gap

Query:

```json
{
  "name": "context_build",
  "arguments": {
    "objective": "What is the latest verified decision and constraint for repository debugging search behavior and what open questions remain?",
    "provider": "codex",
    "maxTokenBudget": 1400,
    "retrievalIntent": "memory_only"
  }
}
```

Observed result:

```json
{
  "recentDecisions": [
    "Decision: use grep search first for repository debugging."
  ],
  "constraints": [],
  "openQuestions": [],
  "proofHandles": 1
}
```

The pack is now proof-carrying, but it still misses the newer constraint/open-question on this benchmark-like continuation query. This confirms that v2.2 improved belief gating and proof attachment more than continuation retrieval quality.

## Conclusions

- Confirmed improvements:
  - proof handles now propagate into live search hits
  - atomic claims for `constraint`, `fact`, and `open_question` are being derived and stored
  - verified corrections can outrank earlier hypotheses
  - hybrid context packs are much more token-efficient
  - repository-only packs remain clean and cheaper than baseline
- Confirmed regressions or weak areas:
  - the benchmark memory-only objective regressed in typed-section quality
  - continuation retrieval is still not good enough
  - `knowledge_report` concurrency worsened in this run
- Architectural interpretation:
  - v2.2 successfully moved the runtime closer to a proof-carrying claim system
  - it did **not** yet solve the hardest part of PCKC, which is semantic claim selection for ambiguous continuation questions

## Remaining v2.2 Gaps

1. Claim-level continuation retrieval still needs better semantic routing or entity-level claim linking.
2. Cross-claim supersession between old decisions and newer constraints is still too weak.
3. The plugin MCP transport still reports decode failures intermittently, even though direct JSON-RPC to `/mcp` succeeds.
4. Graph-heavy concurrent reads remain a major latency bottleneck.
