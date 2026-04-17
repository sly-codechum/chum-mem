# Compute Speed Benchmark Report

Date: 2026-04-09 (legacy run), 2026-04-13 (hook-driven sync addendum)
Scope: `chum-memory`
Status: executed on the live Docker Compose stack

## 2026-04-13 addendum — hook-driven repository + session sync

Added after replacing the docker-mount `project_import` path with a
client-side hook (`hook-dispatch.sh` → `sync.sh` + `session-sync.sh`).
All measurements on the full `chum-memory` repository
(339 tracked source files) on Apple M1 Pro, 8 cores, 16 GB RAM,
release build inside Docker Compose.

### Host-side hook scripts

These are what the Claude Code plugin actually runs before every turn.
Times include shell startup, jq, python, curl, and network round-trip.

| Script / Event | n | min | p50 | p95 | max | Notes |
|---|---|---|---|---|---|---|
| `sync.sh` steady-state (no changes) | 5 | 103ms | **108ms** | 116ms | 116ms | Pure local — git ls-files + SHA-256 + manifest diff, zero API calls. |
| `sync.sh` incremental (1 file changed) | 3 | 562ms | **567ms** | 603ms | 603ms | Hash + diff + POST + server merge + re-cluster. |
| `sync.sh` cold full sync (339 files) | 3 | 2350ms | **2421ms** | 2695ms | 2695ms | First run only; builds the manifest from scratch. |
| `hook-dispatch.sh` UserPromptSubmit | 10 | 228ms | **241ms** | 255ms | 255ms | session-sync event POST + sync.sh no-op in sequence. |
| `hook-dispatch.sh` PostToolUse | 10 | 153ms | **161ms** | 164ms | 164ms | session-sync only (no sync.sh). |
| `hook-dispatch.sh` Notification | 10 | 153ms | **161ms** | 180ms | 180ms | session-sync only. |

### HTTP endpoints — new sync paths

Warm sequential, 20 iterations each, measured with `curl -w '%{time_total}'`.

| Endpoint | n | min | p50 | p95 | max |
|---|---|---|---|---|---|
| `GET /api/knowledge/sync-rules` | 20 | 7.7ms | **11.8ms** | 29.7ms | 29.7ms |
| `POST /api/knowledge/repository-sync` (empty no-op) | 20 | 383.6ms | **389.4ms** | 436.8ms | 436.8ms |

The "empty no-op" case still re-runs Leiden clustering on the existing snapshot, which dominates the cost. The hook's `sync.sh` normally avoids hitting this endpoint at all when nothing changed (early-exits on the client in ~108ms). The endpoint itself only gets hit when there's real work.

### HTTP endpoints — read APIs on current graph (3.4K nodes)

Warm sequential, 20 iterations each. Graph is ~20× larger than the 2026-04-09 baseline, which explains the slower per-query numbers — we are spending more time loading and scanning a bigger snapshot.

| Endpoint | n | min | p50 | p95 | max |
|---|---|---|---|---|---|
| `POST /api/knowledge/query` (search) | 20 | 69.3ms | **81.1ms** | 122.9ms | 122.9ms |
| `POST /api/knowledge/query` (hub_nodes) | 20 | 77.2ms | **81.2ms** | 105.9ms | 105.9ms |
| `GET /api/knowledge/report?layer=repository` | 20 | 75.8ms | **81.3ms** | 90.2ms | 90.2ms |
| `POST /api/search` (hybrid mem_search) | 20 | 12.0ms | **17.6ms** | 57.5ms | 57.5ms |
| `GET /health` | 20 | 7.0ms | **12.0ms** | 26.3ms | 26.3ms |

### Per-turn cost model (measured 2026-04-13)

```
Steady-state turn (no file changes):
  hook-dispatch.sh UserPromptSubmit ≈ 241ms
    │
    ├─ session-sync.sh post prompt  ≈ 161ms
    └─ sync.sh NO_CHANGES            ≈ 108ms   (local; no API call)
  Claude work + 2 MCP queries in parallel   +80ms (knowledge_query + mem_search)
  ─────────────────────────────────────────────────────
  Total hook + discovery overhead          ≈ 320ms

Turn with 1 file touched:
  hook-dispatch.sh UserPromptSubmit ≈ 241ms  (as above, sync.sh sees NO_CHANGES
                                               because edits happen AFTER the hook)
  ...Claude edits the file...
  Next turn's hook fires:
    sync.sh incremental (1 file)   ≈ 567ms
    session-sync.sh post prompt    ≈ 161ms
  ─────────────────────────────────────────────────────
  Next turn hook overhead          ≈ 728ms
```

The key insight: repository sync cost is amortized — the expensive cold sync only happens once (~2.4s), every turn after that is either free (~108ms, no changes) or cheap (~567ms for one file).

### What changed since 2026-04-09

| Change | Impact |
|---|---|
| Removed `./:/workspace:ro` Docker mount | API no longer needs host filesystem access. Repository ingestion works for arbitrary project paths. |
| Removed `project_import` MCP tool and REST route | Dead code path — was broken anyway after the mount removal (filesystem canonicalize failed inside the container). |
| Added `repository_sync` endpoint + `parse_file_batch()` | New in-memory tree-sitter path that takes file contents from the request body. |
| Added `sync.sh`, `session-sync.sh`, `hook-dispatch.sh` | Client-side scripts shipped with the plugin. Use `${CLAUDE_PLUGIN_ROOT}` to locate themselves in any project. |
| Session layer auto-populates from hooks | Previously nothing populated the session layer unless you called `session_*` tools manually. Now every Claude Code event is captured. |
| Graph grew ~20× (171 → 3401 nodes) | This repo is now ingesting itself. Query latencies rose accordingly (~27ms → ~81ms for knowledge_query) because there is simply more data to scan. |

---

## Legacy 2026-04-09 run (kept for trend comparison)

### Summary

The optimized stack is already below `1s` for every measured online read API in this run.

Key result:

- all measured warm sequential APIs stayed below `321ms p95`
- all measured `C=8` benchmarked APIs stayed below `521ms p95`
- `mem_search` is already extremely fast at `13.56ms p95` sequential and `98.91ms p95` at `C=8`

Important caveats:

- this run benchmarked the direct HTTP wrappers (`/health`, `/api/search`, `/api/memory/*`, `/api/context/build`, `/api/knowledge/*`) because the MCP transport path is currently failing in the live stack with `PayloadTooLargeError` visible in `docker compose logs api`
- the live database still shows `ivfflat` on `public.embeddings`; `HNSW` is not present yet, so the implementation is fast but not fully aligned with the intended index strategy from `PLAN.md`

Raw artifact:

- [live-http-2026-04-09.json](./docs/research/compute-speed/results/live-http-2026-04-09.json)

Benchmark runner used:

- [live-http.ts](./scripts/benchmark/live-http.ts)

## Environment

### Host

- CPU: `Apple M1 Pro`
- Cores: `8`
- RAM: `16 GB`

### Runtime

- Docker Compose services up:
  - `api`
  - `postgres`
  - `chroma`
  - `web`
- API Node.js: `v22.22.2`

### PostgreSQL

- `shared_buffers = 128MB`
- `work_mem = 4MB`
- `effective_cache_size = 4GB`

### Vector Index State

Current `public.embeddings` indexes:

- `embeddings_vector_idx` uses `ivfflat (embedding vector_cosine_ops) WITH (lists='100')`
- no `HNSW` index was found in `pg_indexes`

### Service Health Snapshot

Measured from `/health` during this run:

- `totalMemories = 42243`
- `totalSessions = 199`
- `totalProjects = 1`
- `queue.total = 628`
- `queue.pending = 156`
- `queue.running = 2`

This means the benchmark was not run on a completely idle system.

## Method

- runner: `pnpm tsx scripts/benchmark/live-http.ts --iterations=15 --concurrency=8 --concurrency-iterations=5`
- base URL: `http://127.0.0.1:65301`
- project: `00000000-0000-0000-0000-000000000003`
- warm benchmark methodology:
  - one warm-up request per endpoint
  - then `15` measured sequential iterations
- concurrency benchmark methodology:
  - `C=8`
  - `5` rounds
  - `40` measured requests per selected endpoint
- query used for search/context runs:
  - `knowledge graph snapshot communities report export latency performance retrieval cache`

## Sequential Results

These are warm host-side HTTP latencies.

| Endpoint | p50 | p95 | p99 | Status |
|---|---:|---:|---:|---|
| `health_check` | `143.75ms` | `191.22ms` | `191.22ms` | `200` |
| `mem_search` | `8.46ms` | `13.56ms` | `13.56ms` | `200` |
| `memory_get` | `3.54ms` | `6.04ms` | `6.04ms` | `200` |
| `memory_get_batch` | `4.78ms` | `6.84ms` | `6.84ms` | `200` |
| `context_build` | `10.17ms` | `16.62ms` | `16.62ms` | `200` |
| `knowledge_query_hub_nodes` | `67.38ms` | `207.27ms` | `207.27ms` | `200` |
| `knowledge_query_search` | `107.91ms` | `114.32ms` | `114.32ms` | `200` |
| `knowledge_report` | `3.13ms` | `5.48ms` | `5.48ms` | `200` |
| `knowledge_graph_export` | `301.11ms` | `320.50ms` | `320.50ms` | `200` |
| `knowledge_communities` | `2.07ms` | `3.25ms` | `3.25ms` | `200` |

## Concurrency Results

Selected endpoints at `C=8`.

| Endpoint | p50 | p95 | p99 | Status |
|---|---:|---:|---:|---|
| `mem_search` | `24.16ms` | `98.91ms` | `99.93ms` | `200` |
| `memory_get_batch` | `9.55ms` | `13.82ms` | `14.33ms` | `200` |
| `knowledge_query_hub_nodes` | `463.42ms` | `520.07ms` | `520.74ms` | `200` |
| `knowledge_report` | `3.90ms` | `6.21ms` | `7.14ms` | `200` |

## SLO Assessment

### Against the sub-1-second goal

Pass:

- every measured online read API in this run
- every measured `C=8` endpoint in this run

### Against the tighter warm p95 targets from `PLAN.md`

Pass:

- `mem_search`
- `memory_get`
- `memory_get_batch`
- `context_build`
- `knowledge_query`
- `knowledge_report`
- `knowledge_communities`

Miss:

- `knowledge_graph_export`
  - target: `< 250ms p95`
  - measured: `320.50ms p95`

Conditional concern:

- `knowledge_query_hub_nodes` at `C=8`
  - still sub-second
  - but materially slower than the single-request path

## Trace Findings

### `mem_search`

Representative server trace:

- `search_concurrent = 4ms` in the sampled sequential trace
- `load_provenance_final = 1ms`
- sampled server-side total = `5ms`

At `C=8`:

- sampled server-side total = `41ms`
- `search_concurrent = 33ms`
- `load_provenance_final = 6ms`

Interpretation:

- the search path is no longer the main latency problem
- the optimization around concurrent search and deferred final provenance load is working

### `knowledge_query_hub_nodes`

Representative sequential trace:

- `load_cached_snapshot = 0-1ms`
- `run_query = 68-97ms`

Representative `C=8` trace:

- `load_cached_snapshot = 253ms`
- `run_query = 58ms`
- sampled server-side total = `312ms`

Interpretation:

- the current concurrency bottleneck is snapshot/cache loading under parallel graph queries
- the query algorithm itself is not the dominant problem here

## Findings

### 1. The online stack already meets the headline sub-1-second goal

That part is done for the measured read APIs.

### 2. `mem_search` is no longer the bottleneck

`mem_search` is already deep into double-digit milliseconds on warm requests. That is better than the original target by a wide margin.

### 3. `knowledge_graph_export` is now the heaviest measured read endpoint

At `320.50ms p95`, it is still under `1s`, but it is the only measured endpoint currently missing the tighter warm target. This is consistent with large JSON serialization and payload size.

### 4. Graph query concurrency is the current scaling weakness

`knowledge_query_hub_nodes` rises to `520.07ms p95` at `C=8`. The trace points to snapshot/cache loading rather than graph traversal itself.

### 5. The live database is still on `ivfflat`, not `HNSW`

This matters because the plan specifically called for `HNSW` as the primary ANN strategy. The current benchmark proves the system is already fast, but it also proves that the intended vector index migration is not yet reflected in the live schema.

### 6. MCP transport still needs separate repair

The API logs show `PayloadTooLargeError: request entity too large` on the live stack. The thin HTTP wrappers benchmark cleanly, but the MCP transport path should be fixed before declaring the plugin path production-ready.

## Recommended Next Actions

1. Add and benchmark the intended `HNSW` index on `public.embeddings`, then compare recall and latency against the current `ivfflat` baseline.
2. Reduce `knowledge_graph_export` cost, likely by compression, lighter export materialization, or a cached serialized artifact.
3. Investigate snapshot/cache loading contention in graph queries under concurrency.
4. Fix the MCP transport body-size path so plugin and direct HTTP results converge.

## Commands Run

```bash
docker compose ps
curl -sS -m 5 -w '\nHTTP:%{http_code} TOTAL:%{time_total}\n' http://127.0.0.1:65301/health
docker compose exec -T api node -v
docker compose exec -T postgres psql -U chum_mem -d chum_mem -c "show shared_buffers; show work_mem; show effective_cache_size; select indexname, indexdef from pg_indexes where schemaname='public' and tablename='embeddings';"
pnpm tsx scripts/benchmark/live-http.ts --iterations=15 --concurrency=8 --concurrency-iterations=5 --output=docs/research/compute-speed/results/live-http-2026-04-09.json
```

## Limitations

- This pass benchmarked the direct HTTP wrappers because the MCP transport path is currently failing in the live stack.
- This pass measured warm online behavior and moderate concurrency, not cold-start restart behavior.
- This pass did not run recall benchmarking between `ivfflat` and `HNSW` because `HNSW` is not deployed in the current database state.
