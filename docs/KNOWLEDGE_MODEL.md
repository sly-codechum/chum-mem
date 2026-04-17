# Knowledge Model

> Schema and operating model for chum-mem's knowledge graph layer.

## Overview

The knowledge graph is a persistent, evidence-labeled graph of entities and relationships extracted from coding agent sessions. It enables structured querying of project knowledge beyond flat memory search.

## Two-Pass Pipeline

### Graph Layers

The knowledge graph is split into two isolated layers:

- **Repository layer**: AST-extracted code structure built from the codebase itself using tree-sitter parsing across 19 languages. Contains symbols, imports, call graphs, rationale comments, and documentation structure.
- **Session layer**: Interaction history derived from coding agent session events. Contains sessions, episodes, file changes, tool calls, commands, tests, and errors.

Each layer is stored and queried independently via the `snapshot_type` field on graph snapshots.

### Pass 1: Structural Extraction (Deterministic)

Extracts entities directly observed in session events. Every edge is labeled **EXTRACTED** (weight=1.0).

> **Repository layer note**: For the repository layer, structural extraction uses tree-sitter AST parsing (not regex). Supported languages (19): Python, TypeScript, TSX, JavaScript, Go, Rust, Java, C, C++, Ruby, C#, Kotlin, Scala, PHP, Swift, Lua, Zig, Elixir, Julia.

| Event Type | Node Type | Edge Relation |
|-----------|-----------|---------------|
| `file_change` | `file` | `session → file` (modifies) |
| `tool_call` | `tool` | `session → tool` (calls) |
| `tool_result` | `tool` | `session → tool` (produces) |
| `command` | `command` | `session → command` (calls) |
| `test_result` | `test` | `session → test` (produces) |
| `error` | `error` | `error → session` (caused_by) |

Co-occurring files within the same session receive `co_occurs` edges.

### Pass 2: Semantic Extraction (Pattern-Based Inference)

Infers relationships from episode and memory structure. Edges are labeled **INFERRED** (weight=0.8) or **AMBIGUOUS** (weight=0.5).

| Pattern | Edge Relation | Evidence |
|---------|---------------|----------|
| Debugging follows implementation | `caused_by` | INFERRED |
| Implementation follows debugging | `caused_by` (fix) | INFERRED |
| Same files across sessions | `continuity` | INFERRED |
| Content similarity >= 0.5 with at least 4 non-generic shared tokens | `related_to` | INFERRED |
| Content similarity 0.28-0.5 with at least 4 non-generic shared tokens | `related_to` | AMBIGUOUS |

## Evidence Levels

Every edge in the knowledge graph carries an evidence classification:

| Level | Description | Weight | When Used |
|-------|-------------|--------|-----------|
| **EXTRACTED** | Directly observed in source data | 1.0 | Structural pass — imports, function calls, file changes |
| **INFERRED** | Reasoned from patterns with confidence | 0.8 | Semantic pass — causal chains, strong content similarity with distinctive overlap |
| **AMBIGUOUS** | Flagged for review, uncertain | 0.5 | Moderate content similarity with distinctive overlap |

## Node Types

| Type | Source | Description |
|------|--------|-------------|
| `session` | Ingestion | A coding agent session |
| `episode` | Derivation | A segmented work episode within a session |
| `file` | Events | A file touched during a session |
| `tool` | Events | A tool invoked during a session |
| `command` | Events | A shell command executed |
| `test` | Events | A test execution result |
| `error` | Events | An error encountered |
| `memory` | Derivation | A derived memory entry |
| `decision` | Derivation | A decision-type memory |
| `task` | Derivation | A task-type memory |
| `concept` | Derivation | A fact or summary memory |
| `symbol` | Repository | A code symbol (function, class, struct, trait, interface, enum) |
| `module` | Repository | An external module/package dependency |
| `rationale` | Repository | A WHY/NOTE/IMPORTANT comment in code |
| `section` | Repository | A heading/section in documentation |
| `document` | Repository | A documentation file (.md, .txt, .rst) |

## Edge Relations

| Relation | Meaning | Typical Direction |
|----------|---------|-------------------|
| `modifies` | Session/tool modified a file | session/tool → file |
| `calls` | Session called a tool/command | session → tool/command |
| `produces` | Session produced a test result | session → test |
| `caused_by` | Error caused by session; debugging caused by implementation | error → session; episode → episode |
| `references` | Error references a file | error → file |
| `co_occurs` | Files modified together in same session | file ↔ file |
| `contains` | Session contains an episode | session → episode |
| `related_to` | Content similarity between memories | memory ↔ memory |
| `continuity` | Sessions share file context | session → session |
| `supersedes` | Memory replaces an older one | memory → memory |
| `depends_on` | Entity depends on another | any → any |
| `from_same_session` | Entities from same session | any ↔ any |
| `consumes` | Entity consumes output from another | any → any |
| `imports` | File imports another file or module | file → file/module |
| `defines` | File defines a symbol | file → symbol |
| `calls` | File calls a function (call graph) | file → symbol |
| `explains` | Rationale comment explains a file | rationale → file |
| `semantically_similar_to` | Token overlap between files | file ↔ file |

## Community Detection

Communities are detected using the Leiden clustering algorithm:

1. **Phase 1 — Local moving**: Reassign nodes to neighboring communities by modularity gain
2. **Phase 2 — Refinement**: Ensure communities are well-connected (key improvement over Louvain)
3. **Phase 3 — Aggregation**: Build super-node network from communities and recurse
4. Compute cohesion scores (intra-community edge density)
5. Identify bridge nodes (high inter-community connectivity)

Graphs with fewer than 3 nodes are assigned to a single community.

To keep the graph queryable, semantic memory-similarity edges are also degree-limited per memory so one generic summary cannot become a bridge into most of the graph.

## Graph Persistence

### JSON Format (node-link)

Compatible with NetworkX `json_graph.node_link_data()`:

```json
{
  "directed": true,
  "graph": { "version": "1.0.0", "generatedAt": "...", "projectId": "..." },
  "nodes": [{ "id": "file:src/main.ts", "label": "main.ts", "type": "file", "community": 0 }],
  "links": [{ "source": "session:abc", "target": "file:src/main.ts", "relation": "modifies", "evidence": "extracted", "weight": 1.0 }],
  "communities": [...],
  "statistics": { "nodeCount": 10, "edgeCount": 15, ... }
}
```

### Database Tables

- `knowledge_snapshots`: Full graph JSON snapshots per project (`snapshot_type`: "repository" or "session" — isolates code structure from interaction history)
- `knowledge_communities`: Community metadata with cohesion scores
- `knowledge_cache`: Content-addressed extraction cache
- `memory_edges.evidence`: Evidence level on existing edge table
- `memories.community_id`: Community assignment on memory records

## Content-Addressed Caching

Extraction results are cached by a hash of the session event content:

1. Serialize event IDs, types, payloads, and timestamps
2. Hash with dual-pass multiplicative hash (similar to cyrb53)
3. Cache key: `{projectId}:{contentHash}`
4. On re-run, if hash matches, skip extraction and use cached result

## Report Generation

The knowledge report (`KNOWLEDGE_REPORT.md`) includes:

1. **Summary Statistics**: Node/edge counts, evidence distribution, density
2. **Hub Nodes**: Most connected entities by degree
3. **Communities**: Clusters with cohesion scores and representative nodes
4. **Knowledge Gaps**: Isolated nodes, high ambiguity, thin communities
5. **Cross-Session Patterns**: Continuity links, hot files

## MCP Tools

| Tool | Description |
|------|-------------|
| `knowledge_graph_export` | Export full knowledge graph as JSON |
| `knowledge_report` | Generate human-readable knowledge report |
| `knowledge_query` | Query graph: hub nodes, shortest path, neighbors, communities |
| `knowledge_communities` | List detected communities with cohesion scores |

All knowledge tools accept an optional `layer` parameter ("repository" or "session") to target a specific graph layer.

## How to Extend

### Adding a New Extractor

1. Create a function in `rust/crates/chum_mem_pipeline/src/structural-extractor.rs` for deterministic extraction or `semantic-extractor.rs` for inference
2. Return `{ nodes: KnowledgeNode[], edges: KnowledgeEdge[] }`
3. Set appropriate evidence level on all edges
4. Add unit tests
5. Document the extraction rule in this file

### Adding a New Edge Relation

1. Add the relation to the edge relation enum in `rust/crates/chum_mem_pipeline/src/knowledge.rs`
2. Use it in your extractor
3. Document when it applies in the Edge Relations table above
