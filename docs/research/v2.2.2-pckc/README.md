# v2.2.2: Cross-Layer Graph Fusion & Noise Elimination

Date: 2026-04-16
Branch: `v2.2.2` (planning)
Status: research & design
Relationship: builds on v2.2.1 benchmark results (7/12 pass, 58%)

## Why v2.2.2 Exists

The v2.2.1 benchmark revealed five structural problems that cannot be fixed by tuning parameters:

1. **Retrieval noise** (1/5 relevant) — `mem_search` returns claims from unrelated projects/contexts because there's no project-scoped graph constraint on retrieval
2. **Continuation type mismatch** (0.343 fit) — searching for "worker bugs" returns `open_question` and `constraint` claims instead of `bug`/`fix` because embeddings don't encode claim type
3. **Repository layer too shallow** — no call flow, no containment (method→class), no type info, no data flow; agents still fall back to Grep/Glob for "what calls X" questions
4. **Session–repository disconnect** — session claims ("we refactored function X") have zero edges to repository nodes (`symbol:path:X`); the two layers are structurally disjoint
5. **God Node bias** — session nodes and external module nodes dominate degree distribution, biasing community detection and hub_nodes queries

v2.2.2 is the architecture that solves these as one unified problem: **cross-layer graph fusion with typed retrieval**.

## Research Basis

### Academic Literature

| Paper | Key technique | Relevance to v2.2.2 |
|---|---|---|
| **GraphRAG** (Microsoft, 2404.16130) | Hierarchical Leiden communities → community summaries → map-reduce global queries | Our community detection is flat; GraphRAG's hierarchy would let us query at different abstraction levels. Their self-reflection entity extraction doubles recall. |
| **NeuroPath** (NeurIPS 2025, 2511.14096) | Goal-directed semantic path tracking + pruning over KG | 16.3% recall improvement + 22.8% token reduction by pruning irrelevant subgraph paths. Directly applicable to our `mem_search` noise problem — prune paths that don't connect to the query's semantic goal. |
| **GraphRAG Survey** (2408.08921) | Taxonomy: indexing (graph/text/vector/hybrid), retrieval (node/triplet/path/subgraph), generation (pre/mid/post) | We currently do node-level retrieval only. The survey shows path and subgraph retrieval achieve higher precision. Our architecture should support subgraph extraction. |
| **MiniRAG** (2501.06713) | Semantic-aware heterogeneous graph indexing | Combines text chunks + named entities in unified structure. 25% storage of standard approaches. Applicable: our repository and session nodes should be in one heterogeneous graph, not two separate layers. |

### Graphify (safishamsi/graphify) — Prior Art Analysis

Graphify implements several techniques we need:

| Feature | Graphify | chum-mem current | Gap |
|---|---|---|---|
| **God Nodes** | Ranked by degree centrality, reported as architectural anchors | `hub_nodes` query exists but doesn't filter by node type or distinguish structural from noise hubs | Need to distinguish "high-degree because architecturally central" from "high-degree because session/import hub" |
| **Edge confidence** | `EXTRACTED` (1.0) / `INFERRED` (scored) / `AMBIGUOUS` (flagged) | All edges are unweighted or binary | Need confidence-scored edges to filter noise during retrieval |
| **Cross-file call graphs** | Resolves callee names to matching symbols across files | Call edges are name-only, cross-file resolution creates orphaned edge targets | Need proper cross-file call resolution |
| **Rationale comments** | `# WHY:`, `# NOTE:` tags → rationale nodes with `rationale_for` edges | Captures `IMPORTANT`, `TODO`, `FIXME`, etc. but no structural edge to the explained symbol | Need rationale→symbol containment edges |
| **Semantic similarity** | Jaccard token overlap → `semantically_similar_to` edges | Same technique, capped at 500 files | Sufficient |
| **Leiden clustering** | graspologic Leiden with hierarchy | Greedy modularity, single level | Need hierarchical Leiden for multi-level community queries |
| **Hyperedges** | 3+ node relationships (auth flow, protocol impl) | Not supported | Would help represent code flows |
| **Multi-modal** | Code + docs + PDFs + images + audio transcripts | Code + docs only | Audio/image not needed yet |
| **Token reduction** | 71.5× fewer tokens per query | Not measured | Target: ≥50× for repository queries |

## Architecture: Three Pillars

### Pillar 1: Deep Repository Graph

**Problem**: The AST parser captures symbols but not relationships between them.

**Solution**: Full structural extraction with containment, call flow, and type edges.

#### 1a. Containment edges (method → class)

Current: `symbol:path:process` is a flat node. No edge to `symbol:path:DataProcessor` even if `process` is a method of `DataProcessor`.

Proposed: Add `contains` edges from class/struct/trait/enum nodes to their child method/field/const nodes. Tree-sitter already parses the nesting — we just need to emit the edge.

```
symbol:path:DataProcessor --contains--> symbol:path:DataProcessor.process
symbol:path:DataProcessor --contains--> symbol:path:DataProcessor.validate
```

This immediately solves "what methods does DataProcessor have?" without Grep.

#### 1b. Cross-file call resolution

Current: `extract_call_sites` emits callee names as strings. If `file_a.rs` calls `process()`, it creates a `calls` edge to a name-matched symbol — but only within the same file. Cross-file calls create dangling edge targets.

Proposed: Two-pass call resolution:
1. First pass: extract all symbols into a global symbol table `HashMap<String, Vec<NodeId>>`
2. Second pass: resolve call sites against the global table. Ambiguous matches (multiple symbols with same name) get `AMBIGUOUS` confidence; unique matches get `EXTRACTED` confidence.

#### 1c. Arrow functions and const assignments

Current: JS/TS `const foo = () => {}` and `export const handler = ...` are invisible. Only `function` declarations are captured.

Proposed: Add tree-sitter queries for `variable_declarator` where the value is an `arrow_function` or `function`. This is a 20-line change in the TS grammar handler.

#### 1d. Type edges

Current: No type information extracted. Parameter types, return types, generic constraints are discarded.

Proposed: Extract `type_of` edges from function signatures:
```
symbol:path:process --returns--> type:Result<Vec<Node>>
symbol:path:process --param--> type:&GraphDataStore
```

This enables "what functions return Result?" and "what takes a GraphDataStore?" queries.

#### 1e. Doc comment population

Current: `AstSymbol.doc_comment` field exists but is always `None`.

Proposed: Populate from tree-sitter `comment` nodes immediately preceding symbol definitions. This is straightforward — the adjacent-comment detection already exists in the rationale extractor.

### Pillar 2: Session → Repository Cross-Edges

**Problem**: Session claims and repository nodes live in separate graphs with no structural connection. A claim "we refactored function X" has no edge to `symbol:path:X`.

**Solution**: Automatic cross-layer edge injection during session ingestion.

#### 2a. File-change anchoring

Every `file_change` session event already records the file path. During `build-knowledge-graph`, for each file_change event:
1. Look up `file:<path>` in the repository graph
2. If found, create a `touched_by` edge: `file:<path> --touched_by--> session:<id>`
3. Create `modified_in` edges from the session's memory claims to the file node

This immediately links sessions to the files they modified.

#### 2b. Symbol mention extraction

During claim extraction, scan claim text for symbol names that match the repository symbol table:
1. Build symbol name set from repository graph: `{process, DataProcessor, GraphEngine, ...}`
2. For each claim, find matching symbol mentions
3. Create `references` edges: `claim:<id> --references--> symbol:path:X`

This is the "billion dollar algorithm" the user mentioned — but it's actually straightforward: exact-match against the symbol table, then optional fuzzy match for close variants.

#### 2c. Repository-scoped retrieval

Currently `mem_search` returns claims from ALL projects because there's no structural filter. With session→repository edges, we can:
1. Start from the current repository's file nodes
2. Traverse `touched_by` edges to find relevant sessions
3. Traverse session→claim edges to find relevant claims
4. **Only these claims are candidates for retrieval**

This is the GraphRAG "subgraph extraction" pattern applied to our heterogeneous graph.

### Pillar 3: Hierarchical Community Detection & Typed Retrieval

**Problem**: Flat community detection produces communities dominated by God Nodes (session hubs, import hubs).

**Solution**: Hierarchical Leiden + type-aware hub filtering + typed embedding retrieval.

#### 3a. Hierarchical Leiden

Replace single-level greedy modularity with hierarchical Leiden (same algorithm GraphRAG uses):
- Level 0: Coarse communities (~10-30 communities)
- Level 1: Medium communities (~50-200)
- Level 2: Fine-grained communities (~200-1000)

Each level gets a community summary (like GraphRAG's map-reduce summaries, but we use claim text instead of LLM-generated summaries).

#### 3b. God Node damping

Before community detection, apply degree-based damping:
1. Compute node degree distribution
2. Nodes above 95th percentile degree are "hubs"
3. During Leiden, reduce hub edge weights by `1 / log(degree)` so they don't dominate community assignment
4. Hub nodes still appear in communities but don't force unrelated nodes into the same community

This is inspired by TF-IDF: a node that connects to everything is less informative for community structure, just as a word appearing in every document is less informative for search.

#### 3c. Typed embedding index

Current: All claims share one embedding space. A query for "worker bugs" matches "constraint about workers" because the embeddings are close.

Proposed: Partition the embedding index by claim type:
- `index:bug` — only bug claims
- `index:decision` — only decision claims  
- `index:task` — only task claims
- etc.

When `mem_search` receives `types: ["bug", "fix"]`, it searches only those partitions. This is a simple but high-impact change.

#### 3d. Community-aware retrieval

Instead of flat top-k vector search, use a two-stage retrieval:
1. **Community routing**: Embed the query, find the top-3 matching communities
2. **Intra-community search**: Within those communities, do vector search for the top-k claims

This naturally filters noise — a query about "postgres config" routes to the database-infrastructure community and never sees claims from the graph-visualization community.

## Implementation Plan

### Phase 1: Quick wins (reduce noise immediately)

| Change | Impact | Effort |
|---|---|---|
| Typed embedding partitions (3c) | Fixes continuation type mismatch (0.343 → ~0.8) | 1 day |
| Repository-scoped `mem_search` (2c) | Fixes cross-project noise | 1 day |
| Arrow function extraction (1c) | Captures JS/TS const exports | 2 hours |
| Doc comment population (1e) | Enriches symbol context | 2 hours |
| Containment edges (1a) | Enables "methods of class X" queries | 4 hours |

### Phase 2: Cross-layer fusion

| Change | Impact | Effort |
|---|---|---|
| File-change anchoring (2a) | Links sessions to modified files | 1 day |
| Symbol mention extraction (2b) | Links claims to code symbols | 2 days |
| Cross-file call resolution (1b) | Enables "what calls X" without Grep | 2 days |
| Type edges (1d) | Enables type-based queries | 1 day |

### Phase 3: Graph intelligence

| Change | Impact | Effort |
|---|---|---|
| Hierarchical Leiden (3a) | Multi-level community queries | 2 days |
| God Node damping (3b) | Better community quality | 1 day |
| Community-aware retrieval (3d) | Noise reduction through graph routing | 3 days |
| NeuroPath-style path pruning | Prune irrelevant subgraph paths | 3 days |

## Success Criteria (from v2.2.1 benchmark failures)

| Metric | v2.2.1 | Target v2.2.2 | Mechanism |
|---|---|---|---|
| retrieval_noise.relevantTop5 | 1 | ≥4 | Repository-scoped retrieval + community routing |
| retrieval_noise.irrelevantTop5 | 4 | ≤1 | Typed partitions + path pruning |
| continuation.claimTypeFit | 0.343 | ≥0.8 | Typed embedding partitions |
| context_build.repository_only.fillRate | 0.125 | ≥0.5 | Deep repo graph fills more sections |
| compile_v2.proofGapPresent (P5) | false | true | Implement ProofGap emission |
| "what calls X" without Grep | impossible | works | Cross-file call resolution |
| "methods of class Y" | impossible | works | Containment edges |
| Cross-project noise in mem_search | present | eliminated | Repository-scoped retrieval |

## Open Questions

1. **Should sessions become nodes in the repository graph, or should we maintain separate graphs with cross-edges?** Cross-edges are simpler and preserve layer isolation. A unified graph is more powerful but harder to reason about.

2. **How should we handle symbol name collisions across files?** `process` could be a method in 5 different classes. The cross-file call resolver needs disambiguation — likely via import path analysis or file proximity in the dependency graph.

3. **Should community summaries be LLM-generated (like GraphRAG) or constructed from claim text?** LLM summaries are higher quality but require an LLM call during graph build. Claim-text summaries are instant but may be noisy.

4. **What's the right Leiden resolution parameter?** Too low → too few communities (everything merges). Too high → too many (every file is its own community). Need to tune on the actual graph.

## Related Work

- `../v2.2.1-pckc/BENCHMARK_PLAN.md` — benchmark that motivated this research
- `../v2.2.1-pckc/results/COMPARISON.md` — v2.2.1 results showing the gaps
- `../v2.2-pckc/GAP_ANALYSIS.md` — the packer-vs-compiler gap
- https://github.com/safishamsi/graphify — God Nodes, Leiden, cross-file call graphs
- GraphRAG (arXiv 2404.16130) — hierarchical communities, map-reduce queries
- NeuroPath (arXiv 2511.14096) — goal-directed path pruning, 16.3% recall improvement
- GraphRAG Survey (arXiv 2408.08921) — taxonomy of graph-based RAG approaches
- MiniRAG (arXiv 2501.06713) — heterogeneous graph indexing for retrieval
