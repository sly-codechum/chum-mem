# Compute Speed Optimization Plan

Date: 2026-04-09
Scope: `chum-memory-project`
Objective: preserve current external behavior while driving all online MCP/API reads to sub-1-second latency, with a practical target of `p95 < 800ms` and `p99 < 1000ms` for warm requests.

## Executive Summary

The current codebase has the right building blocks, but the online path still does too much work per request:

- `apps/api/src/index.ts` runs lexical and semantic retrieval sequentially inside `hybridSearch()`.
- `apps/api/src/index.ts` loads provenance during the search phase instead of deferring or serving a compact precomputed preview.
- `apps/api/src/index.ts` loads and parses the full `knowledge_snapshots.snapshot` JSON blob on every graph/report/query request.
- `packages/knowledge/src/query.ts` rebuilds adjacency structures per request.
- `packages/knowledge/src/pipeline.ts` and `services/worker/src/index.ts` rely on in-process caches, which disappear on restart and do not accelerate API reads.
- `infra/migrations/0001_initial_schema.sql` has an `ivfflat` index for embeddings, but there is no `hnsw` path yet.

The primary research-backed algorithmic recommendation is:

1. `HNSW` for approximate nearest-neighbor shortlist generation.
2. Exact re-rank on a tiny candidate set.
3. `W-TinyLFU` for hot-result admission and graph/report caching.
4. Precomputed graph read models so graph APIs do not deserialize the full snapshot on every request.

This is the fastest path to sub-second behavior without changing the product contract.

## Non-Negotiables

- Keep PostgreSQL + pgvector as the canonical semantic retrieval path.
- Keep tenant isolation and RLS guarantees intact.
- Keep provider behavior normalized behind existing adapters.
- Keep `mem_search`, `memory_get`, `memory_get_batch`, `context_build`, `knowledge_report`, `knowledge_query`, `knowledge_graph_export`, and `knowledge_communities` behavior-compatible by default.
- Do not move heavy write-side compute back onto the read path.

## Research Synthesis

### 1. ANN shortlist + exact rerank is the core speed lever

The best fit for this project is `HNSW` over pgvector, not a broader architectural rewrite. The reason is simple: the project already treats PostgreSQL as canonical, and HNSW is the best high-recall low-latency ANN choice available within that constraint.

Implication for this repo:

- replace “scan/filter/join/rank everything” with “ANN shortlist first, then exact rerank”
- keep `ivfflat` only as fallback or migration bridge
- use exact rerank on the top `K` candidates so relevance stays stable

### 2. Cache admission matters as much as raw cache existence

For repeated agent workflows, a naive LRU cache will thrash badly on scans, imports, and bursty session-specific queries. `W-TinyLFU` is the right policy because it preserves hit rate under mixed recency/frequency workloads with low overhead.

Implication for this repo:

- use `W-TinyLFU` semantics for the in-process L1 cache
- use a distributed L2 cache only for stable read artifacts
- invalidate by `projectId` and snapshot/version, never TTL-only

### 3. Graph APIs must stop reparsing full snapshots per request

The graph endpoints currently pay a large JSONB parse and in-memory graph reconstruction cost on every call. That can dominate response time even before query logic starts.

Implication for this repo:

- persist a “latest snapshot head” plus precomputed report/export/read models
- cache adjacency and searchable node index keyed by `snapshotId`
- treat `knowledge_report` as a generated artifact, not an on-demand computation

### 4. Full-text and vector retrieval should be two-stage and filter-aware

The current retrieval path blends results well, but the online SQL shape is still expensive. The query should push filters earlier, retrieve a compact vector shortlist first, and only join the rest of the memory payload after the shortlist is known.

Implication for this repo:

- semantic query should operate on `embeddings` first, not on a wider joined shape
- lexical and semantic execution should be concurrent
- provenance should be loaded only for the final displayed set or via a compact precomputed preview

## External Research Sources

Checked on 2026-04-09.

- pgvector official README: `https://github.com/pgvector/pgvector`
  - relevant topics: `HNSW`, `iterative_scan`, `halfvec`, `binary_quantize`, `subvector` indexing, exact rerank after approximate shortlist
- HNSW paper: `https://arxiv.org/abs/1603.09320`
- PostgreSQL text search indexes: `https://www.postgresql.org/docs/current/textsearch-indexes.html`
- PostgreSQL GIN docs: `https://www.postgresql.org/docs/current/gin.html`
- PostgreSQL materialized view refresh: `https://www.postgresql.org/docs/current/sql-refreshmaterializedview.html`
- Caffeine efficiency notes on `W-TinyLFU`: `https://github.com/ben-manes/caffeine/wiki/Efficiency`
- Fastify benchmark page: `https://fastify.dev/benchmarks/`

## Repo-Specific Bottleneck Assessment

### A. Retrieval hot path

Files:

- `apps/api/src/index.ts`
- `packages/retrieval/src/index.ts`
- `infra/migrations/0001_initial_schema.sql`

Current issues:

- lexical search and semantic search run sequentially in `hybridSearch()`
- both search branches hydrate provenance during the search phase
- semantic search calculates a full query embedding and runs a wider CTE than necessary
- result blending is fast enough; data access is the dominant cost
- `ivfflat` exists, but no `hnsw` index exists

### B. Graph read path

Files:

- `apps/api/src/index.ts`
- `packages/knowledge/src/query.ts`
- `infra/migrations/0005_knowledge_graph.sql`

Current issues:

- every `knowledge_report`, `knowledge_query`, `knowledge_graph_export`, and dashboard graph request loads the latest full snapshot JSON
- adjacency is rebuilt per request
- report generation is on-demand, not artifact-based
- no hot cache keyed by snapshot version

### C. Cache architecture

Files:

- `packages/knowledge/src/cache.ts`
- `packages/knowledge/src/pipeline.ts`
- `services/worker/src/index.ts`

Current issues:

- cache is currently process-local and ephemeral
- there is no explicit read-side cache for search responses, graph reports, or graph adjacency
- invalidation is not yet modeled as a first-class online concern

### D. Transport/runtime

Files:

- `apps/api/src/index.ts`
- `packages/db/src/client.ts`

Current issues:

- Express overhead is not the primary bottleneck, but it is not the fastest Node option either
- graph and retrieval work dominate today, so framework replacement should be later, not first

## Target Latency Budget

The plan should enforce a hard budget per request class:

| API Class | Target |
|---|---:|
| `mem_search` | `p95 < 500ms`, `p99 < 800ms` |
| `memory_get` / `memory_get_batch` | `p95 < 250ms` |
| `context_build` | `p95 < 800ms` |
| `knowledge_query` | `p95 < 300ms` |
| `knowledge_report` | `p95 < 150ms` |
| `knowledge_graph_export` | `p95 < 250ms` for cached latest snapshot |
| `knowledge_communities` | `p95 < 150ms` |
| `project_import` | `< 1s` only for no-op or mostly-cached imports; cold rebuild remains background-class work |

Important engineering note:

- A strict `< 1s` guarantee for cold full repository rebuilds or full graph regeneration with identical synchronous semantics is not credible at scale.
- The practical online SLO must apply to read APIs and warmed incremental write APIs.
- Heavy derivation stays asynchronous, while online reads become sub-second through precomputation and caching.

## Target Architecture

### 1. Retrieval path

New online sequence:

1. validate/auth
2. run lexical and semantic shortlist queries concurrently
3. semantic shortlist uses `HNSW` on `embeddings`
4. exact rerank top `K`
5. fuse lexical + semantic
6. apply session/graph/ranking features
7. fetch only final memory rows needed for response
8. fetch provenance preview from a compact precomputed table
9. cache final response by normalized query key

### 2. Graph path

New online sequence:

1. resolve latest `snapshotId`
2. fetch cached `GraphReadModel` by `snapshotId`
3. answer `knowledge_query` from prebuilt adjacency/search structures
4. serve `knowledge_report` from persisted markdown artifact
5. serve `knowledge_graph_export` from persisted node-link artifact

### 3. Cache hierarchy

- L1: process-local `W-TinyLFU` cache for hottest keys
- L2: Redis or equivalent shared cache for cross-process reads
- invalidation:
  - `session_end`
  - `project_import`
  - knowledge snapshot write
  - memory insert/update/supersede

## Phased Execution Plan

## Phase 0: Baseline and Instrumentation

Goal: get exact latency decomposition before changing behavior.

Work:

- add per-tool latency spans for:
  - auth
  - db transaction open
  - lexical search
  - semantic search
  - provenance load
  - graph snapshot load
  - graph report generation
  - serialization
- add benchmark harness:
  - fixed-seed dataset
  - hot-cache run
  - cold-cache run
  - concurrency sweep
- capture `p50`, `p95`, `p99`, and CPU profile for each MCP tool
- add `EXPLAIN (ANALYZE, BUFFERS)` fixtures for hot SQL

Deliverables:

- `docs/research/compute-speed/baseline.md`
- reproducible benchmark script
- latency dashboard per endpoint

Exit criteria:

- every online API has a measured latency decomposition
- top 3 bottlenecks are proven with traces, not guessed

## Phase 1: No-Regret Hot-Path Fixes

Goal: remove obvious wasted work before deeper algorithm changes.

Work:

- run lexical and semantic retrieval concurrently in `hybridSearch()`
- stop hydrating full provenance during shortlist generation
- move provenance to:
  - final top results only, or
  - precomputed `memory_provenance_preview`
- rewrite semantic SQL to shortlist on `embeddings` first, then join memory rows
- eliminate unnecessary `row_number()` work if `memory_id, model` is already unique
- cache parsed latest graph snapshot head in memory
- cache adjacency and searchable node index keyed by `snapshotId`
- cache `knowledge_report` output

Expected gain:

- `2x` to `5x` improvement for search and graph reads before schema changes

Exit criteria:

- warm `knowledge_report` and `knowledge_query` requests are already sub-second
- `mem_search` p95 materially drops without any user-visible contract change

## Phase 2: pgvector Search Redesign

Goal: make semantic retrieval genuinely fast.

Primary algorithm:

- `HNSW` shortlist + exact rerank

Schema work:

- add new migration, for example `0008_latency_online_path.sql`
- create `HNSW` index on embeddings:
  - `using hnsw (embedding vector_cosine_ops)`
- keep `ivfflat` during migration, then benchmark whether to retain it
- add supporting btree indexes for frequent filters:
  - `embeddings(project_id, model, memory_id)`
  - `memories(project_id, created_at desc)`
  - `sessions(id, provider, repo_url, branch)` or equivalent filter support

Query redesign:

- semantic stage 1:
  - pull top `K` candidate `memory_id`s from `embeddings`
  - push `projectId` and model filters as early as possible
- semantic stage 2:
  - exact rerank candidate set
  - join only needed memory/session columns
- lexical stage:
  - keep GIN-backed FTS
  - tune `GIN` behavior for predictable reads
- fusion:
  - preserve current ranking semantics where possible
  - benchmark whether `RRF` improves stability without extra cost; do not switch by default unless it clearly helps

Optional scale-up path if corpus growth demands it:

- `halfvec` expression index
- `binary_quantize` shortlist then exact rerank
- `subvector` prefilter then full-vector rerank

Expected gain:

- semantic lookup becomes bounded by shortlist size instead of corpus size
- online search should become comfortably sub-second on current dataset

Exit criteria:

- `mem_search` p95 < `500ms` on warm data
- semantic stage contributes less than `200ms` p95

## Phase 3: Graph Read Model and Artifactization

Goal: make graph APIs cheap.

Schema/API work:

- add `knowledge_snapshot_heads`
  - latest snapshot pointer per project
- add `knowledge_snapshot_artifacts`
  - `report_markdown`
  - `node_link_json`
  - compressed adjacency/read model
- optionally add `knowledge_node_search`
  - normalized searchable node text
- optionally add `knowledge_edges_compact`
  - adjacency rows optimized for fast neighbor/path queries

Implementation work:

- on snapshot write, generate read artifacts once in worker/background phase
- `knowledge_report()` returns persisted artifact, not regenerated markdown
- `knowledge_query()` uses cached adjacency/read model, not per-request rebuild
- `knowledge_graph_export()` returns persisted node-link export
- `knowledge_communities()` reads precomputed rows directly

Expected gain:

- graph APIs move from “load giant JSON + recompute” to “lookup by snapshot id”

Exit criteria:

- `knowledge_report` p95 < `150ms`
- `knowledge_query` p95 < `300ms`
- `knowledge_graph_export` p95 < `250ms`

## Phase 4: Cache Hierarchy With Explicit Invalidation

Goal: keep hot paths hot under real agent traffic.

Cache strategy:

- L1:
  - process-local `W-TinyLFU`
  - target objects:
    - latest snapshot head
    - graph read model
    - knowledge report markdown
    - search response fragments
- L2:
  - Redis or equivalent
  - same key space, smaller payload count, versioned by snapshot/query hash

Key design:

- `search:{tenant}:{project}:{normalizedQuery}:{filtersHash}:{snapshotVersion}`
- `graph-report:{tenant}:{project}:{snapshotId}`
- `graph-read-model:{tenant}:{project}:{snapshotId}`

Invalidation:

- `session_end` invalidates search + graph keys for the project
- `project_import` invalidates graph keys and latest snapshot head
- memory supersession invalidates affected search keys
- no TTL-only freshness model for correctness-sensitive paths

Expected gain:

- repeated agent workflows collapse toward low-millisecond service times

Exit criteria:

- hot-cache hit rate > `80%` for graph/report reads
- repeat-query p95 < `100ms` for hottest stable endpoints

## Phase 5: Write Path and Derivation Decoupling

Goal: keep writes fast without regressing freshness.

Work:

- keep `session_start` and `session_event_append` lightweight and batch-friendly
- ensure `session_end` only performs the minimum synchronous durability work
- move graph/report/export regeneration fully behind worker completion
- preserve read-after-write via:
  - session-local overlay cache, or
  - latest committed memory rows plus previous snapshot until new snapshot is ready
- batch inserts for provenance and edges where possible
- avoid repeated JSON parse/serialize work inside worker hot loops

Expected gain:

- ingestion APIs become stable under load and stop competing with reads

Exit criteria:

- write-heavy workflows do not degrade read SLOs
- worker backlog remains bounded under benchmark concurrency

## Phase 6: Runtime and Transport Polish

Goal: finish the last 10-20% after data-path bottlenecks are fixed.

Work:

- benchmark Express vs Fastify on real handlers, not synthetic assumptions
- only migrate if handler overhead remains material after Phases 1-5
- ensure DB pool sizing matches concurrent read/write load
- review serialization and compression overhead for large JSON graph exports
- prewarm hottest caches on process boot

Expected gain:

- modest but real improvement after algorithmic fixes

Exit criteria:

- framework overhead is no longer a measurable top-3 cost

## Concrete Schema and Contract Changes

Affected packages/apps:

- `apps/api`
- `services/worker`
- `packages/retrieval`
- `packages/knowledge`
- `packages/db`
- `packages/contracts`
- `infra/migrations`

Planned schema changes:

- add `HNSW` index for `public.embeddings`
- add btree support indexes for vector shortlist filtering
- add `knowledge_snapshot_heads`
- add `knowledge_snapshot_artifacts`
- add `memory_provenance_preview` or equivalent compact preview table/materialized view
- optionally add compact graph read tables if in-memory artifacts are still too slow

Planned API/contract changes:

- default behavior unchanged
- optional internal-only fields may be added to metrics/diagnostics
- optional future non-breaking flags:
  - `includeProvenancePreview`
  - `responseMode=compact|full`
  - only if later needed; not required for phase 1

## Security and Isolation Implications

- all new cache keys must include tenant and project scope
- no cache artifact may be shared across tenant boundaries
- any persisted read model must carry the same `organization_id`, `team_id`, and `project_id` semantics as source rows
- invalidation must be scoped by tenant/project to avoid cross-tenant cache poisoning
- graph artifact tables must get RLS policies in the same migration as table creation
- if Redis is introduced, it must not become an authority for tenant scope; it is only a cache of already-authorized views

## Testing Strategy

### Benchmark tests

- per-tool latency benchmark
- warm and cold cache benchmark
- concurrency benchmark at `1`, `8`, `32`, `64` concurrent requests
- large graph snapshot benchmark

### Correctness tests

- result equivalence before/after query rewrite
- exact-session ranking preservation
- same query before/after cache hit returns identical contract output
- graph report/query/export parity before/after artifactization

### Database tests

- `EXPLAIN ANALYZE` snapshots checked into docs/artifacts
- HNSW recall-vs-latency benchmark
- GIN lexical regression benchmark

### Security tests

- RLS tests for all new artifact tables
- cache key scoping tests
- cross-project leakage tests for graph and search caches

## Rollout Order

1. Instrument first and establish real baseline.
2. Ship Phase 1 code-path cleanup behind feature flags.
3. Add `HNSW` path and benchmark against existing `ivfflat`.
4. Ship graph read-model/artifact path.
5. Add L1 `W-TinyLFU` cache with explicit invalidation.
6. Add shared L2 cache only after L1 behavior is proven.
7. Re-benchmark and only then consider Fastify/runtime polish.

## Success Criteria

The plan is complete only when all of the following are true:

- `mem_search`, `knowledge_report`, `knowledge_query`, `knowledge_graph_export`, `knowledge_communities`, `memory_get`, and `memory_get_batch` meet the stated online SLOs
- retrieval quality remains behavior-compatible on benchmark queries
- graph API outputs remain behavior-compatible
- no tenant isolation regression exists in new artifact or cache layers
- benchmark scripts and traces exist in-repo for future regressions

## Recommended Immediate First Implementation Sprint

If execution starts now, the highest-value first sprint is:

1. instrument `hybridSearch()` and graph endpoints
2. parallelize lexical + semantic retrieval
3. remove eager provenance loading from shortlist generation
4. add `HNSW` index and ANN-first semantic shortlist query
5. cache latest graph snapshot head + report artifact

That sprint should already remove most of the current `>5s` pain if the observed bottlenecks match the code-path analysis above.
