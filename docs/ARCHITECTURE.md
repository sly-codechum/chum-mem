# Architecture

> chum-mem: Cloud-native persistent memory platform for coding agents.

## System Overview

```
┌─────────────────────────────────────────────────┐
│                  Client Agents                   │
│         Claude  │  Codex  │  Gemini              │
└────────┬────────┴─────────┴──────────────────────┘
         │ MCP (stdio / Streamable HTTP)
┌────────▼────────────────────────────────────────┐
│              rust/apps/api (Axum)                 │
│  ┌──────────┐ ┌──────────┐ ┌──────────────────┐ │
│  │ Ingestion│ │  Search  │ │  Context Build   │ │
│  │ Pipeline │ │ (hybrid) │ │  (token budget)  │ │
│  └────┬─────┘ └────┬─────┘ └────────┬─────────┘ │
│       │             │                │           │
│  ┌────▼─────────────▼────────────────▼─────────┐ │
│  │              MCP Tool Layer                  │ │
│  │  session_start │ mem_search │ context_build  │ │
│  │  session_event │ memory_get │ health_check   │ │
│  │  session_end   │ graph_snapshot              │ │
│  │  knowledge_graph_export │ knowledge_report   │ │
│  │  knowledge_query │ knowledge_communities     │ │
│  │  repository_sync  (hook-driven incremental)  │ │
│  └──────────────────────────────────────────────┘ │
└────────┬────────────────────────────────────────┘
         │
┌────────▼────────────────────────────────────────┐
│           rust/crates/ (shared libs)             │
│                                                  │
│  contracts  config  db       pipeline   app      │
│  (serde)    (env)   (sqlx)   (graph+    (tracing │
│                              tree-sitter) signals)│
└────────┬────────────────────────────────────────┘
         │
┌────────▼────────────────────────────────────────┐
│            rust/apps/worker                      │
│  ┌────────────────────────────────────────────┐  │
│  │ Job Queue (polling, advisory locks)        │  │
│  │                                            │  │
│  │ build-knowledge-graph (session layer)      │  │
│  │ sync-chroma-index                          │  │
│  │ replay-failed-session                      │  │
│  └────────────────────────────────────────────┘  │
└────────┬────────────────────────────────────────┘
         │
┌────────▼────────────────────────────────────────┐
│            Infrastructure                        │
│  ┌──────────────┐  ┌─────────────┐              │
│  │ PostgreSQL   │  │ Chroma      │              │
│  │ + pgvector   │  │ (optional)  │              │
│  │ + RLS        │  │             │              │
│  └──────────────┘  └─────────────┘              │
└─────────────────────────────────────────────────┘
```

## Data Flow

### Ingestion Pipeline — hook-driven dual-layer sync

Both layers are populated from the client side via Claude Code plugin hooks, not by pulling files from a Docker-mounted filesystem. The `hook-dispatch.sh` script in the plugin runs on every Claude Code hook event, reads the hook payload from stdin, and fans out to two scripts: `sync.sh` (repository layer) and `session-sync.sh` (session layer).

#### Repository layer — incremental client-side sync

```
┌──────────── Host ────────────┐                    ┌─────── Docker ───────┐
│                              │                    │                      │
│  Claude Code hook fires      │                    │                      │
│         │                    │                    │                      │
│         ▼                    │                    │                      │
│  hook-dispatch.sh            │                    │                      │
│         │                    │                    │                      │
│         ▼                    │                    │                      │
│  sync.sh                     │                    │                      │
│    1. git ls-files           │                    │                      │
│    2. filter by rules        │                    │                      │
│    3. SHA-256 each file      │                    │                      │
│    4. diff vs manifest.tsv   │                    │                      │
│    5. if no change → exit    │                    │                      │
│    6. POST changed files ────┼───── HTTP ────────▶│  /api/knowledge/     │
│         │                    │  (files + hashes  │   repository-sync    │
│         │                    │   + removedPaths) │         │            │
│         ▼                    │                    │         ▼            │
│  Save manifest.tsv.tmp       │                    │  parse_file_batch()  │
│         │                    │                    │   tree-sitter (19    │
│         │                    │                    │   languages)         │
│         │                    │                    │         │            │
│         │                    │                    │         ▼            │
│         │                    │                    │  Remove stale nodes/ │
│         │                    │                    │  edges for changed + │
│         │                    │                    │  removed paths       │
│         │                    │                    │         │            │
│         │                    │                    │         ▼            │
│         │                    │                    │  Merge incremental   │
│         │                    │                    │  into existing repo  │
│         │                    │                    │  snapshot, re-run    │
│         │                    │                    │  leiden_clustering() │
│         │                    │                    │         │            │
│         │                    │                    │         ▼            │
│         │                    │                    │  Persist snapshot    │
│         │                    │                    │  (snapshot_type=     │
│         │                    │                    │   repository)        │
│         │◀───────────────────┼───── response ─────│         │            │
│         ▼                    │                    │                      │
│  Atomically mv manifest.tsv  │                    │                      │
└──────────────────────────────┘                    └──────────────────────┘
```

Key design points:

- **No Docker mount required.** The API runs in a container and never touches the host filesystem. File contents are streamed over HTTP and parsed in-memory with tree-sitter.
- **Content-addressed incremental cache** — `.chum-cache/manifest.tsv` tracks `<relative_path>\t<sha256>` pairs. Only files whose hashes changed since the last successful sync are sent. The manifest is written atomically (`manifest.tsv.tmp` → `mv`) so a failed POST never corrupts the cache.
- **Rules fetched once** — `GET /api/knowledge/sync-rules` returns the ignore lists and max file size from the server's `SyncRules` so the client filter stays in lock-step with the Rust pipeline constants. The response is cached in `.chum-cache/sync-rules.json`.
- **Graceful degradation** — if the API is unreachable, sync.sh exits non-zero with a message on stderr and the hook still emits its JSON output so the user's turn continues.

#### Session layer — full lifecycle via hooks

```
hook-dispatch.sh  →  session-sync.sh   Reads hook_event_name from stdin,
                                       dispatches by type:

  SessionStart    →  POST /v1/ingest/session/start
                     saves chum-mem sessionId to
                     .chum-cache/session-<claude-id>.json

  UserPromptSubmit → session_event_append (type=prompt,   message=prompt)
  PostToolUse      → session_event_append (type=tool_result,
                       toolName, filePath, command, metadata)
  Notification     → session_event_append (type=annotation, message)
  PreCompact       → session_event_append (type=summary,  trigger+instructions)
  SubagentStop     → session_event_append (type=summary,  agentId, lastMessage)
  Stop             → session_event_append (type=response) + session_end
                     deletes session state file
  SessionEnd       → session_end (idempotent safety net)
```

`session_end` triggers the worker pipeline that derives memories and builds the session-layer knowledge graph:

```
session_end
    │
    ▼
┌─────────────────────────────────────────────┐
│ deriveSessionEpisodes                        │
│   conversation / implementation / debugging  │
│                                              │
│ deriveMemoriesFromSession                    │
│   memories + provenance links                │
│                                              │
│ buildSessionCompletionJobPlan                │
│   enqueue build-knowledge-graph worker job   │
└───────────┬──────────────────────────────────┘
            │
            ▼
┌─────────────────────────────────────────────┐
│ Worker: build-knowledge-graph                │
│ (snapshot_type=session)                      │
│                                              │
│ Pass 1: Structural  — extract_structural()   │
│ Pass 2: Semantic    — extract_semantic()     │
│ build_knowledge_graph() + leiden_clustering()│
│ persist_snapshot(snapshot_type=session)      │
└──────────────────────────────────────────────┘
```

The `SessionEventPayload` schema is fixed (`{message, toolName, command, exitCode, filePath, diffStat, metadata}`); the original hook JSON is preserved verbatim in `raw_payload` for auditing.

### Retrieval Pipeline

```
mem_search(query) → lexical (tsvector GIN) ─┐
                  → semantic (pgvector)  ────┤
                                             │
                                  mergeHybridResults()
                                             │
                                  rankHybridResults()
                                    │ lexical score (0.28)
                                    │ semantic score (0.24)
                                    │ session relevance (0.18)
                                    │ graph proximity (0.30)
                                    │ recency (0.08)
                                    │ importance (0.08)
                                    │ confidence (0.06)
                                    │ - freshness penalty (0.10)
                                    │ - superseded penalty (0.10)
                                             │
                                  progressiveDisclosure()
                                    │ overview (top 5)
                                    │ related (top 12)
                                    │ full (all)
```

## Crate Dependencies (Rust)

```
chum_mem_contracts
chum_mem_config
chum_mem_db         ← contracts, config
chum_mem_pipeline   ← contracts (+ tree-sitter grammars for 17 languages)
chum_mem_app        ← config
api                 ← db, pipeline, contracts, config, app
worker              ← db, pipeline, contracts, config, app
```

## Knowledge Architecture

See [KNOWLEDGE_MODEL.md](./KNOWLEDGE_MODEL.md) for the full schema.

### Two Isolated Graph Layers

The knowledge graph is split into two layers stored as separate snapshots (`snapshot_type` column):

```
┌─────────────────────────────────────────────┐
│          Repository Layer                    │
│  snapshot_type = "repository"                │
│                                              │
│  Source: tree-sitter AST extraction          │
│  19 languages: Python, TS, Go, Rust, Java,  │
│    C, C++, Ruby, C#, Kotlin, Scala, PHP,    │
│    Swift, Lua, Zig, Elixir, Julia, TSX, JS  │
│                                              │
│  Nodes: files, symbols, modules, rationale   │
│  Edges: imports, defines, calls,             │
│         semantically_similar_to              │
│                                              │
│  Measured: 27ms p50 query latency            │
└─────────────────────────────────────────────┘

┌─────────────────────────────────────────────┐
│          Session Layer                       │
│  snapshot_type = "session"                   │
│                                              │
│  Source: session event ingestion             │
│  Nodes: sessions, episodes, memories,        │
│         files-touched, errors, commands      │
│  Edges: modifies, calls, caused_by,          │
│         co_occurs, continuity, related_to    │
│                                              │
│  Measured: 709ms p50 query latency           │
│  (larger graph — tens of thousands of nodes) │
└─────────────────────────────────────────────┘
```

Layers never merge with each other. `repository_sync` (via hook) writes to the repository layer; `session_end` triggers worker jobs that write to the session layer.

### Key Design Decisions

1. **Layer isolation** — repository graphs contain only code structure; session graphs contain only interaction history. Queries target one layer at a time via the `layer` parameter, producing focused results without cross-contamination.

2. **Tree-sitter AST extraction** — the repository layer uses tree-sitter for deterministic code parsing across 19 languages, replacing the previous regex-based approach. This enables proper call graph extraction, scoped symbol resolution, and language-aware import detection.

3. **Evidence labels on every edge** — enables confidence-weighted retrieval and transparent auditing of how knowledge was derived. Evidence ratios are meaningful per-layer (e.g., repository: 57% EXTRACTED / 43% INFERRED reflects AST vs semantic similarity).

4. **Leiden community detection** — replaced greedy modularity with the Leiden algorithm, which guarantees well-connected communities through a refinement phase after each local moving pass.

5. **Hook-driven client-side sync** — repository ingestion runs from the Claude Code plugin hook, not from a Docker filesystem mount. The client walks the git tree, hashes files, diffs against a local manifest, and POSTs only the delta. This removes the `./:/workspace:ro` mount from the container, works for projects anywhere on the host, and keeps the steady-state turn cost near zero (~100ms, no API call) when nothing changed.

6. **Content-addressed incremental caching** — `.chum-cache/manifest.tsv` on the client avoids reparsing unchanged files on the server; `hook-dispatch.sh` uses the same mechanism for session events keyed by an idempotency hash so retries are safe.

7. **Worker-based async processing** — knowledge extraction runs as a background job after session completion, not blocking the ingestion response.

## Multi-Tenancy

All data is scoped by `(organization_id, team_id, project_id)`. PostgreSQL Row-Level Security enforces tenant isolation at the database level via `set_config` variables set in every transaction.

## Configuration

Environment variables (validated by Zod):

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `DATABASE_URL` | Yes | — | PostgreSQL connection string |
| `CHUM_MEM_ORGANIZATION_ID` | Yes | — | Tenant org ID |
| `CHUM_MEM_TEAM_ID` | Yes | — | Tenant team ID |
| `CHUM_MEM_PROJECT_ID` | No | — | Default project scope |
| `MCP_PORT` | No | 65301 | MCP server port |
| `WEB_PORT` | No | 65300 | Dashboard port |
| `CHROMA_URL` | No | — | Optional Chroma URL |
| `WORKER_POLL_INTERVAL_MS` | No | 5000 | Worker poll interval |
