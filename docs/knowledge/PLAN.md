# Knowledge Architecture Enhancement Plan

> Adapting Graphify's knowledge-base patterns to chum-memory-project.
> Date: 2026-04-07 | Status: DRAFT

---

## Current Architecture Assessment

### Purpose
chum-memory-project is a **cloud-native persistent memory platform for coding agents**. It provides a normalized MCP server that ingests session activity from Claude, Codex, Gemini, and other AI clients, stores raw and derived memory in PostgreSQL (with pgvector), and returns provenance-aware context packs for future sessions.

### Core Modules

| Layer | Package | Role |
|-------|---------|------|
| API | `apps/api` | MCP server (stdio + streamable HTTP), ingestion endpoints, search, context building |
| Web | `apps/web` | Express-based admin dashboard with D3 graph visualization |
| Worker | `services/worker` | Polling-based background job processor (Chroma sync, replay) |
| Contracts | `packages/contracts` | Zod-based request/response schemas (14+ types) |
| Auth | `packages/auth` | Token hashing (scrypt), scope resolution, validation |
| DB | `packages/db` | PostgreSQL client, migrations, worker jobs, context mgmt |
| Provider Adapters | `packages/provider-adapters` | Interface registry for Claude/Codex/Gemini (interface only) |
| Retrieval | `packages/retrieval` | Hybrid search, ranking, progressive disclosure, context packing |

### Data Model Summary

- **4 SQL migrations** covering: users/orgs/teams/projects, sessions/events/episodes, memories/provenance/edges/embeddings, worker jobs/replays
- **RLS enforcement** on all tenant tables
- **Memory types**: fact, decision, task, bug, summary, implementation_detail, change_log, risk
- **Edge types**: duplicates, supersedes, caused_by, depends_on, related_to, from_same_session
- **Embeddings**: pgvector(1536) with FNV-1a local hash (deterministic, no external LLM)

### Completeness: ~55-60%

### Strengths
1. Clean Zod contract design — all request/response types well-defined
2. Solid multi-tenant RLS model with scoped tokens
3. Episode/memory derivation logic is functional
4. Progressive disclosure search interface well-designed
5. Good separation between ingestion, retrieval, and worker concerns
6. Hybrid search (lexical + semantic) with multi-signal ranking

### Bottlenecks & Architectural Smells

| # | Issue | Severity | Impact |
|---|-------|----------|--------|
| 1 | **No two-pass extraction pipeline** — memories are derived from heuristic episode segmentation only; no structural extraction pass | HIGH | Misses structured relationships between entities |
| 2 | **FNV-1a hash embeddings** — deterministic but low-quality; no path to real embeddings | HIGH | Semantic search quality ceiling |
| 3 | **No evidence labeling** — memory edges lack confidence/evidence classification | HIGH | Cannot distinguish observed facts from inferred relationships |
| 4 | **Chroma integration is half-baked** — fire-and-forget sync with no reconciliation | MEDIUM | Silent divergence risk |
| 5 | **Worker is polling-based** — 5s poll interval, no queue backend | MEDIUM | Wasteful for small projects |
| 6 | **Provider adapters unimplemented** — interface only, no Claude/Codex/Gemini adapters | MEDIUM | Cannot normalize provider-specific payloads |
| 7 | **Memory supersession not automated** — `superseded_at` exists but never set programmatically | MEDIUM | Stale memories persist indefinitely |
| 8 | **Context packing is greedy** — no coverage enforcement or deduplication | MEDIUM | Suboptimal context quality |
| 9 | **No observability** — no structured logging, tracing, or metrics | MEDIUM | Blind to production issues |
| 10 | **Hardcoded ranking weights** — no evaluation framework | LOW | Cannot tune search quality |
| 11 | **Test coverage minimal** — 6 unit test files, zero integration tests | HIGH | Regressions undetectable |
| 12 | **Session replay unimplemented** — table and job type exist, no execution logic | LOW | Dead code |

### Missing Capabilities vs. Graphify

| Graphify Feature | chum-memory Status | Gap |
|-----------------|-------------------|-----|
| Deterministic structural extraction (AST/tree-sitter) | Heuristic episode segmentation | No code-aware extraction |
| Semantic/LLM-assisted extraction | Template-based memory derivation | No LLM enrichment pass |
| Evidence labels (EXTRACTED/INFERRED/AMBIGUOUS) | No confidence classification on edges | Missing entirely |
| graph.json persistence | `memory_edges` table + `graph_snapshot` MCP tool | Partial — DB-only, no exportable format |
| GRAPH_REPORT.md generation | None | Missing entirely |
| Interactive graph.html | D3 dashboard (partial) | Exists but not wired to APIs |
| Incremental cache by content hash | Session event idempotency only | No content-addressed caching for analysis |
| Community detection (Leiden) | None | Missing entirely |
| Multimodal input (code/docs/images) | Session events only | Domain-appropriate — not needed |

### Best Insertion Points

1. **Structural extraction** → New `packages/knowledge` module between ingestion and memory derivation
2. **Semantic extraction** → Extension of `session_end` pipeline in `apps/api/src/ingestion.ts`
3. **Evidence labeling** → Extend `memory_edges` schema + add to `packages/contracts`
4. **Graph persistence** → New export job in `services/worker` + new MCP tool
5. **Report generation** → New `packages/knowledge/report.ts` module
6. **Incremental caching** → Content hash on `session_events` + cache table
7. **Community detection** → Graph-native clustering in `packages/knowledge/cluster.ts`

---

## Enhancement Plan

### Phase 2A: New Abstractions & Module Structure

```
packages/
  knowledge/                      ← NEW: Core knowledge architecture
    package.json
    tsconfig.json
    src/
      index.ts                    ← Public API
      schema.ts                   ← KnowledgeNode, KnowledgeEdge, EvidenceLevel types
      structural-extractor.ts     ← Pass 1: Deterministic extraction from session events
      semantic-extractor.ts       ← Pass 2: LLM-assisted relationship inference
      graph-builder.ts            ← Merge extracted + inferred into unified graph
      graph-persistence.ts        ← Export to graph.json node-link format
      cluster.ts                  ← Community detection (Leiden-inspired)
      report-generator.ts         ← Generate KNOWLEDGE_REPORT.md
      cache.ts                    ← Content-addressed analysis cache
      index.test.ts               ← Unit tests
```

### Phase 2B: Schema Changes

#### New Contract Types (`packages/contracts/src/knowledge.ts`)

```typescript
// Evidence level for every edge/fact
enum EvidenceLevel {
  EXTRACTED = 'extracted',   // Directly observed in source data
  INFERRED = 'inferred',    // Reasoned with confidence from patterns
  AMBIGUOUS = 'ambiguous',  // Flagged for review
}

// Knowledge node (entity in the graph)
interface KnowledgeNode {
  id: string;
  label: string;
  type: 'memory' | 'session' | 'episode' | 'file' | 'concept' | 'decision' | 'task';
  sourceType: 'session_event' | 'memory' | 'derived';
  sourceId: string;
  metadata: Record<string, unknown>;
  communityId?: number;
}

// Knowledge edge (relationship)
interface KnowledgeEdge {
  source: string;
  target: string;
  relation: string;  // calls, references, supersedes, caused_by, co_occurs, etc.
  evidence: EvidenceLevel;
  weight: number;    // 1.0 for EXTRACTED, 0.8 for INFERRED, 0.5 for AMBIGUOUS
  sourceFile?: string;
  metadata?: Record<string, unknown>;
}

// Full knowledge graph export format (compatible with NetworkX node-link)
interface KnowledgeGraph {
  version: string;
  generatedAt: string;
  projectId: string;
  nodes: KnowledgeNode[];
  edges: KnowledgeEdge[];
  communities: CommunityInfo[];
  statistics: GraphStatistics;
}
```

#### Database Migration (`infra/migrations/0005_knowledge_graph.sql`)

```sql
-- Evidence level enum
CREATE TYPE evidence_level AS ENUM ('extracted', 'inferred', 'ambiguous');

-- Add evidence_level to memory_edges
ALTER TABLE memory_edges ADD COLUMN evidence evidence_level NOT NULL DEFAULT 'extracted';
ALTER TABLE memory_edges ADD COLUMN weight NUMERIC(3,2) NOT NULL DEFAULT 1.0;
ALTER TABLE memory_edges ADD COLUMN metadata JSONB DEFAULT '{}';

-- Knowledge cache (content-addressed)
CREATE TABLE knowledge_cache (
  content_hash TEXT PRIMARY KEY,
  project_id UUID NOT NULL REFERENCES projects(id),
  source_type TEXT NOT NULL,  -- 'session_event', 'memory', 'episode'
  source_id UUID NOT NULL,
  extracted_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  result JSONB NOT NULL,      -- cached extraction result
  expires_at TIMESTAMPTZ      -- optional TTL
);
CREATE INDEX idx_knowledge_cache_project ON knowledge_cache(project_id);

-- Community assignments
CREATE TABLE knowledge_communities (
  id SERIAL PRIMARY KEY,
  project_id UUID NOT NULL REFERENCES projects(id),
  community_id INTEGER NOT NULL,
  label TEXT,
  cohesion_score NUMERIC(5,4),
  node_count INTEGER NOT NULL DEFAULT 0,
  computed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  UNIQUE(project_id, community_id)
);

-- Node community membership
ALTER TABLE memories ADD COLUMN community_id INTEGER;

-- Graph snapshots (persistent exports)
CREATE TABLE knowledge_snapshots (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  project_id UUID NOT NULL REFERENCES projects(id),
  snapshot JSONB NOT NULL,    -- Full KnowledgeGraph JSON
  node_count INTEGER NOT NULL,
  edge_count INTEGER NOT NULL,
  community_count INTEGER NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
```

### Phase 2C: Pipeline Integration

The knowledge extraction pipeline runs as a **post-ingestion job** triggered on `session_end`:

```
session_end
  → deriveSessionEpisodes()         [existing]
  → deriveMemoriesFromSession()     [existing]
  → NEW: structuralExtraction()     [Pass 1 — deterministic]
      Extract entities from session events:
        - file_change events → file nodes + change edges
        - tool_call/tool_result → tool nodes + invocation edges
        - command events → command nodes
        - test_result events → test nodes + pass/fail edges
        - error events → error nodes + caused_by edges
      All edges labeled EXTRACTED (weight=1.0)
  → NEW: semanticExtraction()       [Pass 2 — pattern-based inference]
      Analyze across derived memories:
        - Co-occurrence in same episode → related_to (INFERRED)
        - Sequential debugging episodes → caused_by (INFERRED)
        - Same file touched across sessions → continuity (INFERRED)
        - Contradicting decisions → supersedes (AMBIGUOUS)
      Edges labeled INFERRED (weight=0.8) or AMBIGUOUS (weight=0.5)
  → NEW: buildKnowledgeGraph()
      Merge structural + semantic extractions
      Persist edges to memory_edges with evidence labels
  → NEW: detectCommunities()         [optional, periodic]
      Run on full project graph
      Assign community_id to memories
  → enqueueWorkerJobs()             [existing — add new job types]
```

### Phase 2D: New Worker Job Types

| Job Type | Priority | Trigger | Description |
|----------|----------|---------|-------------|
| `build-knowledge-graph` | 60 | session_end | Run structural + semantic extraction |
| `detect-communities` | 40 | Every 10 sessions or on-demand | Leiden-inspired clustering |
| `generate-knowledge-report` | 30 | On-demand via MCP tool | Produce KNOWLEDGE_REPORT.md |
| `export-knowledge-snapshot` | 50 | On-demand or periodic | Persist graph.json to knowledge_snapshots |

### Phase 2E: New MCP Tools

| Tool | Description |
|------|-------------|
| `knowledge_graph_export` | Export full knowledge graph as JSON (node-link format) |
| `knowledge_report` | Generate human-readable architecture/knowledge report |
| `knowledge_query` | Query the knowledge graph: shortest path, neighbors, community members |
| `knowledge_communities` | List detected communities with cohesion scores |

### Phase 2F: Proposed Folder Changes Summary

```diff
  packages/
+   knowledge/                     # NEW package
+     src/schema.ts                # Types: KnowledgeNode, KnowledgeEdge, EvidenceLevel
+     src/structural-extractor.ts  # Pass 1: deterministic entity extraction
+     src/semantic-extractor.ts    # Pass 2: pattern-based inference
+     src/graph-builder.ts         # Merge and persist unified graph
+     src/graph-persistence.ts     # Export to JSON/snapshot
+     src/cluster.ts               # Community detection
+     src/report-generator.ts      # KNOWLEDGE_REPORT.md generation
+     src/cache.ts                 # Content-addressed extraction cache
    contracts/src/
+     knowledge.ts                 # Knowledge-specific Zod schemas
      index.ts                     # Re-export knowledge contracts
    retrieval/src/
~     index.ts                     # Integrate evidence-weighted ranking
  apps/api/src/
~   ingestion.ts                   # Add knowledge extraction to session_end
~   routes.ts                      # Add knowledge MCP tools
  services/worker/src/
~   index.ts                       # Handle new job types
  infra/migrations/
+   0005_knowledge_graph.sql       # Evidence labels, cache, communities, snapshots
  docs/
+   knowledge/PLAN.md              # This document
+   ARCHITECTURE.md                # Updated architecture overview
+   KNOWLEDGE_MODEL.md             # Knowledge graph schema documentation
```

### Phase 2G: Dependency Changes

| Action | Package | Reason |
|--------|---------|--------|
| ADD | None required | Community detection implemented with graph-native algorithm (no graspologic/Python needed) |
| KEEP | `postgres` | Primary store for graph data |
| KEEP | `zod` | Schema validation for knowledge types |
| DEPRECATE (future) | `chromadb` | pgvector + knowledge graph replaces Chroma's role |

**Design decision**: Implement community detection as a simple graph-native algorithm in TypeScript (modularity-based greedy clustering) rather than importing Python's graspologic. This keeps the stack unified and avoids FFI complexity. The quality tradeoff is acceptable for the typical graph sizes in this domain (hundreds to low thousands of nodes).

### Phase 2H: Migration Strategy

1. **Non-breaking**: All changes are additive — new tables, new columns with defaults, new job types
2. **Backward compatible**: Existing MCP tools (`mem_search`, `memory_get`, `context_build`) continue working unchanged
3. **Incremental rollout**: Knowledge extraction is a new pipeline stage that writes to new tables; existing memory derivation is untouched
4. **Cache warming**: First run after migration processes all existing sessions; subsequent runs are incremental

### Phase 2I: Risks

| Risk | Likelihood | Mitigation |
|------|-----------|------------|
| Community detection quality on small graphs | HIGH | Skip clustering below 20 nodes; revisit algorithm if needed |
| Semantic extraction false positives | MEDIUM | Conservative inference rules; AMBIGUOUS label for uncertain edges |
| Knowledge graph JSON too large for MCP response | MEDIUM | Pagination + streaming; limit default export to 1000 nodes |
| Migration 0005 on existing data | LOW | Additive schema changes; no data migration required |
| Worker job backlog on large session history | MEDIUM | Process only new sessions; batch backfill as opt-in command |

### Phase 2J: Test Strategy

| Test Type | Target | Files |
|-----------|--------|-------|
| Unit | Structural extractor — entity extraction from event types | `packages/knowledge/src/structural-extractor.test.ts` |
| Unit | Semantic extractor — inference rules | `packages/knowledge/src/semantic-extractor.test.ts` |
| Unit | Graph builder — merge logic | `packages/knowledge/src/graph-builder.test.ts` |
| Unit | Community detection — clustering | `packages/knowledge/src/cluster.test.ts` |
| Unit | Report generator — output format | `packages/knowledge/src/report-generator.test.ts` |
| Unit | Cache — content addressing | `packages/knowledge/src/cache.test.ts` |
| Integration | Full pipeline: session → episodes → memories → knowledge graph | `apps/api/src/knowledge-pipeline.test.ts` |
| Integration | Knowledge MCP tools | `apps/api/src/knowledge-tools.test.ts` |
| Existing | Ensure existing tests still pass | All current test files |

### Phase 2K: Backward Compatibility

- **MCP tools**: All existing tools (`session_start`, `mem_search`, `context_build`, etc.) unchanged
- **API routes**: All existing HTTP endpoints unchanged
- **Database**: Additive migration — no existing columns modified or removed
- **Contracts**: New `knowledge.ts` module added; existing contracts untouched
- **Retrieval**: Ranking formula gains optional `evidence_weight` signal but defaults to current behavior
- **Worker**: New job types added; existing `sync-chroma-index` and `replay-failed-session` unchanged

---

## Phase 3: Implementation Checklist

### 3.1 — Knowledge Schema (`packages/knowledge/src/schema.ts`)
- [ ] Define `EvidenceLevel` enum
- [ ] Define `KnowledgeNode` interface
- [ ] Define `KnowledgeEdge` interface
- [ ] Define `KnowledgeGraph` export format
- [ ] Define `CommunityInfo` interface
- [ ] Define `GraphStatistics` interface

### 3.2 — Structural Extractor (`packages/knowledge/src/structural-extractor.ts`)
- [ ] Extract file nodes from `file_change` events
- [ ] Extract tool nodes from `tool_call`/`tool_result` events
- [ ] Extract command nodes from `command` events
- [ ] Extract test nodes from `test_result` events
- [ ] Extract error nodes from `error` events
- [ ] Create EXTRACTED edges between co-occurring entities
- [ ] Content-hash each event for cache key

### 3.3 — Semantic Extractor (`packages/knowledge/src/semantic-extractor.ts`)
- [ ] Co-occurrence inference: entities in same episode → `related_to` (INFERRED)
- [ ] Causal inference: debugging after implementation → `caused_by` (INFERRED)
- [ ] Continuity inference: same file across sessions → `continuity` (INFERRED)
- [ ] Contradiction detection: conflicting decisions → `supersedes` (AMBIGUOUS)
- [ ] Temporal clustering: events within time windows

### 3.4 — Graph Builder (`packages/knowledge/src/graph-builder.ts`)
- [ ] Merge structural + semantic extraction results
- [ ] Deduplicate nodes by ID
- [ ] Merge duplicate edges (take highest evidence level)
- [ ] Persist to `memory_edges` with evidence labels
- [ ] Update `memories.community_id` after clustering

### 3.5 — Graph Persistence (`packages/knowledge/src/graph-persistence.ts`)
- [ ] Export to NetworkX-compatible node-link JSON format
- [ ] Save to `knowledge_snapshots` table
- [ ] Support incremental graph updates (merge with previous snapshot)

### 3.6 — Community Detection (`packages/knowledge/src/cluster.ts`)
- [ ] Implement greedy modularity-based clustering
- [ ] Handle disconnected components (each is own initial cluster)
- [ ] Split oversized communities (>25% of total, min 10 nodes)
- [ ] Compute cohesion scores per community
- [ ] Identify bridge nodes (high inter-community connectivity)

### 3.7 — Report Generator (`packages/knowledge/src/report-generator.ts`)
- [ ] Summary statistics section (nodes, edges, communities, evidence distribution)
- [ ] Hub nodes section (most connected entities)
- [ ] Communities section (cluster labels, cohesion, representative members)
- [ ] Knowledge gaps section (isolated nodes, thin communities, high ambiguity)
- [ ] Cross-session patterns section (recurring files, tools, error patterns)

### 3.8 — Cache (`packages/knowledge/src/cache.ts`)
- [ ] SHA-256 content hashing for session events
- [ ] Cache check before extraction
- [ ] Cache write after extraction
- [ ] Cache invalidation on re-ingestion

### 3.9 — Database Migration (`infra/migrations/0005_knowledge_graph.sql`)
- [ ] Add `evidence_level` enum
- [ ] Add `evidence`, `weight`, `metadata` columns to `memory_edges`
- [ ] Create `knowledge_cache` table
- [ ] Create `knowledge_communities` table
- [ ] Add `community_id` to `memories`
- [ ] Create `knowledge_snapshots` table
- [ ] Add appropriate indexes

### 3.10 — Knowledge Contracts (`packages/contracts/src/knowledge.ts`)
- [ ] Zod schemas for all knowledge types
- [ ] Request/response types for new MCP tools
- [ ] Re-export from `packages/contracts/src/index.ts`

### 3.11 — Pipeline Integration (`apps/api/src/ingestion.ts`)
- [ ] Add `buildKnowledgeFromSession()` call after memory derivation
- [ ] Wire structural + semantic extractors
- [ ] Enqueue `build-knowledge-graph` worker job

### 3.12 — MCP Tools (`apps/api/src/routes.ts`)
- [ ] `knowledge_graph_export` tool
- [ ] `knowledge_report` tool
- [ ] `knowledge_query` tool
- [ ] `knowledge_communities` tool

### 3.13 — Worker Jobs (`services/worker/src/index.ts`)
- [ ] Handle `build-knowledge-graph` job type
- [ ] Handle `detect-communities` job type
- [ ] Handle `generate-knowledge-report` job type
- [ ] Handle `export-knowledge-snapshot` job type

---

## Phase 4: Production Hardening

### 4.1 — Refactoring Targets
- [ ] Extract ingestion business logic from `apps/api/src/ingestion.ts` into `packages/ingestion/`
- [ ] Extract job planning from `apps/api/src/job-planning.ts` into `packages/jobs/`
- [ ] Consolidate ranking weight constants into `packages/retrieval/src/config.ts`
- [ ] Remove dead session replay code or implement it fully

### 4.2 — Evidence-Weighted Retrieval
- [ ] Add `evidence_weight` signal to ranking formula
- [ ] Weight EXTRACTED edges at 1.0, INFERRED at 0.8, AMBIGUOUS at 0.5
- [ ] Add to blended score: `+0.06 * evidenceWeight`
- [ ] Adjust existing weights to sum to 1.0

### 4.3 — Typing Improvements
- [ ] Add strict return types to all public functions in `packages/knowledge/`
- [ ] Add `@param` JSDoc to complex functions
- [ ] Enable `strictNullChecks` in new package

### 4.4 — Error Handling
- [ ] Wrap extraction pipeline in try/catch with structured error logging
- [ ] Add circuit breaker for community detection on large graphs
- [ ] Validate knowledge graph before snapshot persistence

### 4.5 — Observability
- [ ] Add structured JSON logging to knowledge pipeline stages
- [ ] Log extraction timing: structural_ms, semantic_ms, merge_ms, cluster_ms
- [ ] Log graph growth metrics: nodes_added, edges_added, communities_changed

### 4.6 — Test Coverage
- [ ] All unit tests from Phase 3 checklist
- [ ] Integration test: full session → knowledge graph pipeline
- [ ] Snapshot test: graph export JSON format stability
- [ ] Regression test: existing `mem_search` behavior unchanged

---

## Phase 5: Developer Experience

### 5.1 — Documentation
- [ ] `docs/ARCHITECTURE.md` — Updated architecture with knowledge layer
- [ ] `docs/KNOWLEDGE_MODEL.md` — Schema docs, evidence levels, query patterns
- [ ] Generated `KNOWLEDGE_REPORT.md` — Example output from report generator

### 5.2 — CLI/MCP Workflows

**Build knowledge graph for a project:**
```
→ session_end (auto-triggers knowledge extraction)
→ knowledge_graph_export (export current graph)
→ knowledge_report (generate human-readable report)
```

**Query the knowledge graph:**
```
→ knowledge_query { query: "what files are most connected?", projectId: "..." }
→ knowledge_communities { projectId: "..." }
→ graph_snapshot (existing — enhanced with evidence labels)
```

**Inspect a specific relationship:**
```
→ memory_get { id: "..." }  (existing — now includes evidence level)
→ knowledge_query { query: "shortest path between X and Y" }
```

### 5.3 — Extension Guide

To add a new knowledge extractor:

1. Create `packages/knowledge/src/extractors/my-extractor.ts`
2. Implement `KnowledgeExtractor` interface:
   ```typescript
   interface KnowledgeExtractor {
     name: string;
     extract(events: SessionEvent[]): { nodes: KnowledgeNode[]; edges: KnowledgeEdge[] };
   }
   ```
3. Register in `packages/knowledge/src/structural-extractor.ts` or `semantic-extractor.ts`
4. Add unit test in `packages/knowledge/src/extractors/my-extractor.test.ts`

To add a new evidence level rule:
1. Add inference logic in `semantic-extractor.ts`
2. Document the rule and its confidence rationale in `KNOWLEDGE_MODEL.md`

---

## Follow-Up Improvements (Post-MVP)

1. **LLM-assisted semantic extraction** — Use Claude to infer higher-order relationships from memory content (currently pattern-based only)
2. **Real embeddings pipeline** — Replace FNV-1a with actual embedding model for pgvector
3. **Automated memory supersession** — Detect when new decisions contradict old ones and mark `superseded_at`
4. **Knowledge graph diffing** — Compare snapshots across time to surface architectural drift
5. **Interactive graph.html** — Full vis.js/D3 interactive explorer exported as static HTML
6. **Provider adapter implementations** — Normalize Claude/Codex/Gemini payloads into canonical events
7. **Chroma deprecation path** — Replace with pgvector-only semantic search once embeddings quality is sufficient
8. **Ranking weight tuning** — A/B framework for evaluating ranking formula changes
9. **Cross-project knowledge linking** — Shared concepts across team projects
10. **WebSocket live graph updates** — Real-time graph visualization as sessions stream in
