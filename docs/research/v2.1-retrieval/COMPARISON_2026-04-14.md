# v2.1 Retrieval Comparison

Date: 2026-04-14  
Branch: `v2.1`  
Benchmark harness: `scripts/benchmark/live-http.ts`  
Docker target: `docker compose` services `postgres`, `chroma`, `api`, `worker`, `web`  
Baseline artifact: `docs/research/v2.1-retrieval/results/baseline-pre-change-2026-04-14.json`  
Post-change artifact: `docs/research/v2.1-retrieval/results/post-change-2026-04-14.json`

## Reproduction

1. `docker compose build api worker web`
2. `docker compose up -d postgres chroma api worker web`
3. wait for `http://localhost:65301/ready`
4. sync the current repository through `POST /api/knowledge/repository-sync`
5. ingest the controlled validation fixture through:
   - `POST /v1/ingest/session/start`
   - `POST /v1/ingest/session/event`
   - `POST /v1/ingest/session/end`
6. run:

```bash
pnpm tsx scripts/benchmark/live-http.ts \
  --base-url=http://127.0.0.1:65301 \
  --project-id=00000000-0000-0000-0000-000000000003 \
  --iterations=15 \
  --concurrency=8 \
  --concurrency-iterations=5 \
  --git-branch=v2.1 \
  --request-timeout-ms=30000 \
  --output=docs/research/v2.1-retrieval/results/post-change-2026-04-14.json \
  --verbose=true
```

## Dataset

- Repository corpus: current checked-out `chum-memory` worktree synced into the repository graph.
- Repository sync result at validation time:
  - `filesAdded`: 366
  - `totalFiles`: 1096
  - `nodeCount`: 14414
  - `edgeCount`: 38646
  - `communityCount`: 182
- Session fixture: controlled Codex session on branch `v2.1` covering:
  - retrieval-intent routing
  - repository-only context packs
  - hybrid context separation

## Before/After Latency

### Sequential p50

| Endpoint | Baseline | Post-change | Result |
|---|---:|---:|---|
| `mem_search` | 7.0ms | 5.8ms | faster |
| `context_build` | 6.8ms | 279.4ms | slower |
| `knowledge_query(search)` | 910.8ms | 773.7ms | faster |
| `knowledge_report` | 1978.5ms | 805.3ms | faster |

### Concurrent p50

| Endpoint | Baseline | Post-change | Result |
|---|---:|---:|---|
| `mem_search` | 23.4ms | 11.1ms | faster |
| `knowledge_query(hub_nodes)` | 12675.4ms | 11566.5ms | faster, still slow |
| `knowledge_report` | 10748.6ms | 10023.1ms | faster, still slow |

## Context Build Quality

| Case | Baseline | Post-change | Evidence |
|---|---|---|---|
| Repository-only | `knownBugs` + `implementationNotes`; source-only budget share `1.00` | `repositoryKnowledge` only; source-only budget share `0.00` | repository-only routing now avoids memory retrieval |
| Memory-only | `knownBugs`; fill rate `0.125`; used `1117` tokens | `knownBugs` + `sessionContinuity`; fill rate `0.25`; used `509` tokens | continuity is now surfaced, but durable-decision extraction is still weak |
| Hybrid | no typed sections; source-only budget share `0.97` | `repositoryKnowledge` + `sessionContinuity`; source-only budget share `0.44` | hybrid pack now keeps repository evidence and continuity separate |

## Retrieval Noise

| Query bucket | Baseline | Post-change | Result |
|---|---|---|---|
| `retrieval_noise` | relevant top-5 `2`, irrelevant top-5 `3` | relevant top-5 `3`, irrelevant top-5 `0` | improved materially |
| `continuation_noise` | relevant top-5 `0`, irrelevant top-5 `4` | relevant top-5 `0`, irrelevant top-5 `3` | slight improvement only |

## Repository Search Accuracy

| Case | Baseline | Post-change | Result |
|---|---|---|---|
| Exact file path | top-1 exact | top-1 exact | preserved |
| Exact symbol | top-1 exact | top-1 exact | preserved |
| Doc heading | top-3 hit; top hit was `CONTEXT_BUILD_SEARCH_LIMIT` | top-3 hit; top hit is `context_build` symbol | improved ranking locality |
| Rationale lookup | top-1 exact, but hit came from vendored `pdf-lib` node | top-1 exact from `rust/crates/chum_mem_pipeline/src/ast_parser.rs` | improved source locality |

## Cross-Layer Separation

| Check | Baseline | Post-change | Result |
|---|---|---|---|
| Repository/session leak count | `0` | `0` | preserved |
| Repository top node types | `["symbol"]` | `["symbol","symbol","symbol","symbol","symbol"]` | repository layer remains dominant |

## Live MCP Examples

Manual JSON-RPC against `POST /mcp` after `initialize`:

### Repository search

Query:

```json
{
  "name": "knowledge_query",
  "arguments": {
    "layer": "repository",
    "query": "search",
    "text": "rust/apps/api/src/main.rs"
  }
}
```

Top result:

```json
{
  "id": "file:rust/apps/api/src/main.rs",
  "label": "main.rs",
  "type": "file"
}
```

### Repository-only context pack

Query:

```json
{
  "name": "context_build",
  "arguments": {
    "provider": "codex",
    "projectId": "00000000-0000-0000-0000-000000000003",
    "objective": "Explain repository architecture and the roles of perform_search, perform_context_build, and build_context_pack",
    "repoUrl": "file:///Workspace/chum-memory",
    "branch": "v2.1",
    "maxTokenBudget": 1200
  }
}
```

Observed result:

```json
{
  "retrievalIntent": "repository_only",
  "repositoryKnowledge": 10,
  "sessionContinuity": 0,
  "sources": 0,
  "usedTokens": 310
}
```

### Hybrid context pack

Observed result:

```json
{
  "retrievalIntent": "hybrid",
  "repositoryKnowledge": 10,
  "sessionContinuity": 15,
  "sources": 9,
  "usedTokens": 864
}
```

This is the main architectural change: repository evidence and continuity memory now coexist in separate labeled sections instead of collapsing into a provenance-heavy flat pack.

## Conclusions

- Confirmed improvement:
  - repository-only context routing is fixed
  - hybrid context packs are now source-labeled
  - retrieval noise dropped for the broad repository/context query
  - repository search still returns exact file and symbol hits and now ranks rationale/doc-heading results more usefully
- Confirmed tradeoff:
  - `context_build` is materially slower because it now performs graph-backed evidence assembly instead of a cheap memory-search-only pack
- Remaining gaps:
  - memory-only retrieval is still too dependent on summary/continuity artifacts instead of atomic decision/task memories
  - graph-heavy endpoints remain slow under concurrency
  - the Codex plugin transport reported decode failures during this run, but manual `/mcp` initialize and `tools/call` requests succeeded against the rebuilt server, so that issue needs separate client-side investigation

## Files Changed In This Pass

- `scripts/benchmark/live-http.ts`
- `packages/contracts/src/context.ts`
- `packages/contracts/src/memory.ts`
- `rust/crates/chum_mem_contracts/src/lib.rs`
- `rust/crates/chum_mem_pipeline/src/context.rs`
- `rust/crates/chum_mem_pipeline/src/ranking.rs`
- `rust/crates/chum_mem_pipeline/src/repository.rs`
- `rust/crates/chum_mem_pipeline/src/knowledge.rs`
- `rust/apps/api/src/main.rs`
