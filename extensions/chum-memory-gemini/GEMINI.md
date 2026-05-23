# Chum Memory Gemini Extension

This extension gives you MCP tools for persistent memory and a knowledge graph. **You MUST call these tools — do not just describe what you would do.**

## Tool-call rules (mandatory)

When the user mentions memory, recall, search, session, ingestion, knowledge graph, or uses `/chum-memory`:

1. **Always call MCP tools directly.** Do not summarize what you could do — execute the tool call.
2. Start with `knowledge_report(layer:"unified")` so the session reads the compact repository digest, session communities, and cross-layer summary before raw retrieval.
3. Use a repository-layer `knowledge_query` to build structural awareness of relevant components, architecture, and relationships. Use `knowledge_communities` after that when cluster structure matters.
4. Then call `mem_search` (mode=hybrid, disclosureLevel=overview, limit=5).
   Include `sessionId` when continuing the same workstream.
5. If results are relevant, call `memory_get_batch` with selected IDs.
6. Use `context_build` only when you need token-bounded context packing.
7. For session ingestion: call `session_start`, then `session_event_append` for each event, then `session_end`.

If a tool call fails, report the error. Do not silently skip tool usage.

## Knowledge graph — auto-updated on every session end

When you call `session_end`, it automatically:
1. Derives episode segments and searchable memories
2. **Enqueues a `build-knowledge-graph` job** that runs a two-pass extraction pipeline:
   - **Structural pass**: Extracts entities (files, tools, commands, tests, errors) from events. All edges `EXTRACTED` (weight=1.0).
   - **Semantic pass**: Infers causal chains, continuity, and content similarity. Edges `INFERRED` (weight=0.8) or `AMBIGUOUS` (weight=0.5).
3. Detects communities in the knowledge graph
4. Persists a snapshot for future queries

You do NOT need to manually trigger knowledge graph builds — they happen on every `session_end`.

## MCP server selection

Use the single MCP server name `chum-memory`.
The install profile decides its URL:
- `local` -> `http://localhost:65301/mcp`
- `production` -> `https://api.mcp.codechum.com/mcp`

## Available MCP tools

### Memory & Retrieval
| Tool | Purpose |
|------|---------|
| `health_check` | Verify backend connectivity, migrations, and queue state |
| `mem_search` | Hybrid memory search with session-aware ranking |
| `memory_get` | Fetch single memory by ID |
| `memory_get_batch` | Fetch multiple memories by ID |
| `context_build` | Build compact context from retrieval results |

### Session Ingestion
| Tool | Purpose |
|------|---------|
| `session_start` | Start a provider session for ingestion |
| `session_event_append` | Append events to an active session |
| `session_end` | End session and trigger memory derivation + knowledge graph build |

### Knowledge Graph
| Tool | Purpose |
|------|---------|
| `graph_snapshot` | Knowledge graph nodes/edges for visualization |
| `knowledge_graph_export` | Full graph export as NetworkX-compatible JSON |
| `knowledge_report` | Human-readable report: hubs, communities, gaps, cross-session patterns |
| `knowledge_query` | Graph queries: hub_nodes, shortest_path, neighbors, communities |
| `knowledge_communities` | List communities with cohesion scores and bridge nodes |

## Evidence levels

Every edge in the knowledge graph carries a confidence classification:

| Level | Weight | Meaning |
|-------|--------|---------|
| `EXTRACTED` | 1.0 | Directly observed in events |
| `INFERRED` | 0.8 | Reasoned from patterns |
| `AMBIGUOUS` | 0.5 | Uncertain, flagged for review |

Always indicate evidence levels when presenting knowledge graph results.

## Ingestion expectations

When persisting sessions, keep structured event context:

- user/assistant messages
- tool calls and tool outputs
- command text and command outputs
- file paths and diff stats
- error messages
- provenance links from derived memory to raw session events

## Current runtime model

- PostgreSQL + pgvector is the durable retrieval substrate.
- Chroma is an active typed semantic retrieval source in the v2.2.3 hybrid path, not a fallback.
- `session_end` derives episode-based memories, builds the knowledge graph, and may enqueue replay or sync follow-up work.
- `health_check` includes migration state, worker queue state, and entity counts.

## Project architecture anchors

Use these runtime code paths as implementation truth:

- `apps/api`
- `services/worker`
- `packages/knowledge` (knowledge pipeline: extraction, graph, clustering, reports)
- `packages/contracts`
- `packages/provider-adapters`
- `packages/retrieval`
- `packages/db`
