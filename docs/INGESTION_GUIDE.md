# Session Ingestion Guide

## Quick Start

```bash
# Import all Claude sessions (auto-confirm)
pnpm sessions:import --yes

# Import from specific project roots
pnpm sessions:import --roots ~/.claude/projects --yes

# Shorthand for --yes
pnpm sessions:import -yes
```

## Commands

| Command | Description |
|---------|-------------|
| `pnpm sessions:import --yes` | Import new sessions from default roots |
| `pnpm sessions:import --roots <path> --yes` | Import from specific directories |
| `pnpm sessions:import --fresh --yes` | Re-ingest ALL sessions (including already completed) |
| `pnpm sessions:import --dry-run` | Preview what would be imported without sending data |

## Flags

| Flag | Default | Description |
|------|---------|-------------|
| `--yes` / `-yes` | `false` | Skip confirmation prompt |
| `--fresh` | `false` | Force re-ingestion of already-completed sessions |
| `--roots <paths>` | `~/.claude/projects` | Comma-separated directories to scan |
| `--concurrency <n>` | `8` | Number of parallel session imports |
| `--batch-size <n>` | `25` | Events sent per HTTP batch |
| `--max-files <n>` | unlimited | Cap on number of session files to process |
| `--max-events <n>` | unlimited | Cap on events per session |
| `--from <date>` | none | Only import sessions after this date (ISO 8601) |
| `--to <date>` | none | Only import sessions before this date (ISO 8601) |
| `--dry-run` | `false` | Parse and report without sending to server |
| `--server <url>` | `http://localhost:65301` | MCP server URL |
| `--project <id>` | `default` | Project ID for scoping |

## Incremental vs Fresh Ingestion

### Default (incremental)

```bash
pnpm sessions:import --yes
```

The importer **skips sessions that are already completed** on the server. This is the normal mode -- fast, safe, and idempotent. Run it as often as you want.

### Fresh re-ingestion (`--fresh`)

```bash
pnpm sessions:import --fresh --yes
```

Use `--fresh` when:

1. **Schema changed** -- After running a new migration that changes how memories or episodes are derived, you want the new derivation logic applied to old sessions.
2. **Knowledge pipeline upgraded** -- The `session_end` handler now triggers `build-knowledge-graph`. To rebuild the knowledge graph from historical sessions, re-ingest them so each triggers the pipeline.
3. **Data corruption** -- If session data got into a bad state and you need a clean slate.
4. **First time enabling knowledge graph** -- If you had sessions ingested before the knowledge pipeline existed, run `--fresh` once so each `session_end` fires the graph builder.

With `--fresh`, the server receives a new `session_start` for each session. If the server returns a duplicate/completed status, the importer proceeds anyway instead of skipping.

### Recommended workflow for existing data

If you already have ingested sessions and just added the knowledge pipeline:

```bash
# Step 1: Make sure the API and worker are running
pnpm dev

# Step 2: Re-ingest all sessions to trigger knowledge graph builds
pnpm sessions:import --fresh --yes

# Step 3: Monitor the worker logs -- each session_end enqueues a
# build-knowledge-graph job that the worker processes automatically
```

The worker will process `build-knowledge-graph` jobs in the background. Each job runs the two-pass extraction pipeline (structural + semantic), builds the knowledge graph, detects communities, and persists a snapshot.

## Performance Tuning

The importer runs with **8 concurrent session imports** and **25 events per batch** by default. For large imports:

```bash
# High throughput on a fast machine
pnpm sessions:import --concurrency 16 --batch-size 50 --fresh --yes

# Conservative for limited resources
pnpm sessions:import --concurrency 4 --batch-size 10 --yes
```

### How it works

1. **Phase 1 (Parse)**: All session files are discovered and parsed in parallel using a worker pool. File I/O is the bottleneck here.
2. **Phase 2 (Import)**: Parsed sessions are imported concurrently. Each session does: `session_start` -> batched `session_event_append` calls -> `session_end`.
3. **Duplicate detection**: Sessions with matching `externalSessionId` that are already completed are skipped (unless `--fresh`).

### Throughput

Expect ~5-15 sessions/second on a local server depending on session size and machine specs. The final output shows exact throughput:

```
Done! 142 imported, 8 skipped, 0 failed in 12.4s (11.5 sessions/sec)
```

## Supported Providers

| Provider | File Pattern | Default Root |
|----------|-------------|--------------|
| Claude | `*.jsonl` | `~/.claude/projects` |
| Codex | `*.jsonl` | `~/.codex/sessions` |
| Gemini | `*.json` | `~/.gemini/sessions` |

## After Ingestion

Once sessions are ingested, the knowledge graph builds automatically via worker jobs. You can query it using MCP tools:

- `knowledge_report` -- Human-readable report with hubs, communities, gaps
- `knowledge_query` -- Query hub nodes, shortest paths, neighbors
- `knowledge_communities` -- List communities with cohesion scores
- `graph_snapshot` -- Full graph data for visualization
- `knowledge_graph_export` -- NetworkX-compatible JSON export
