# v2.2.2 Design: Cross-Layer Graph Fusion & Noise Elimination

## 1. Deep Repository Graph — Algorithms

### 1.1 Containment Edge Extraction

Current tree-sitter traversal visits class/struct bodies but doesn't emit parent→child edges. The fix is straightforward:

```text
extract_ast(source, language):
  // Existing: walk tree, collect symbols
  symbols = []
  stack = []  // track nesting: [(symbol_name, node_id)]

  visit(node):
    if node matches class|struct|trait|enum|interface:
      sym = AstSymbol(name=node.name, kind=Class|Struct|...)
      symbols.push(sym)
      stack.push((sym.id, node))

    if node matches function|method:
      sym = AstSymbol(name=node.name, kind=Function)
      symbols.push(sym)
      if stack.last() exists:
        parent = stack.last()
        // NEW: emit containment edge
        sym.qualified_name = f"{parent.name}.{sym.name}"
        edges.push(Edge(parent.id, sym.id, type="contains"))

    recurse into children
    if leaving class|struct scope: stack.pop()
```

**Impact on graph**: For a codebase with 500 classes averaging 5 methods each, this adds ~2,500 `contains` edges. These edges enable:
- `knowledge_query(neighbors, nodeId:"symbol:path:MyClass")` → returns all methods
- Community detection groups methods with their parent class (instead of orphaned)

### 1.2 Cross-File Call Resolution

Two-pass algorithm:

```text
Pass 1 — Build global symbol table:
  symbol_table: HashMap<String, Vec<(NodeId, FilePath)>> = {}
  for file in repository:
    for symbol in file.symbols:
      symbol_table[symbol.name].push((symbol.node_id, file.path))

Pass 2 — Resolve call sites:
  for file in repository:
    for call in file.call_sites:
      candidates = symbol_table[call.callee_name]
      if candidates.len() == 0:
        // External call (stdlib, dependency) — create module edge
        edge = Edge(call.caller, module:call.callee_name, type="calls", confidence=0.5)
      else if candidates.len() == 1:
        // Unique match — high confidence
        edge = Edge(call.caller, candidates[0].node_id, type="calls", confidence=1.0)
      else:
        // Ambiguous — resolve by import analysis
        imported = file.imports.filter(imp => imp.resolves_to(candidates))
        if imported.len() == 1:
          edge = Edge(call.caller, imported[0].node_id, type="calls", confidence=0.9)
        else:
          // Truly ambiguous — create edges to all with low confidence
          for candidate in candidates:
            edge = Edge(call.caller, candidate.node_id, type="calls", confidence=0.3)
            edge.tag = "AMBIGUOUS"
```

**Disambiguation heuristics** (ordered by reliability):
1. **Import path**: If file A imports `from module_b import process`, resolve `process()` to `module_b::process`
2. **Same-file preference**: If the callee exists in the same file, prefer it
3. **Directory locality**: Prefer symbols in the same directory
4. **Degree centrality**: Prefer the symbol with more callers (more likely to be the canonical one)

### 1.3 Confidence-Scored Edges

Inspired by Graphify's three-tier edge tagging:

```text
EdgeConfidence:
  EXTRACTED  = 1.0   // Directly parsed from AST (import, call in same scope, containment)
  RESOLVED   = 0.9   // Cross-file resolution via import path
  INFERRED   = 0.5   // Semantic similarity, single-candidate name match without import
  AMBIGUOUS  = 0.3   // Multiple candidates, no import to disambiguate
```

During retrieval, edges below a confidence threshold can be filtered:
- `knowledge_query(neighbors)` with `minConfidence=0.5` excludes ambiguous edges
- Community detection weights edges by confidence (low-confidence edges contribute less to modularity)

## 2. Session → Repository Cross-Edges

### 2.1 File-Change Anchoring Algorithm

During `build-knowledge-graph` job, after the session graph is built:

```text
anchor_session_to_repository(session_graph, repo_graph):
  for event in session.events:
    if event.type == "file_change":
      repo_node = repo_graph.find("file:" + event.file_path)
      if repo_node:
        // Link file to session
        add_edge(repo_node, session.node, type="touched_by",
                 weight=1.0, metadata={event_time, change_type})

        // Link file to claims derived from this event's episode
        for claim in session.claims_in_episode(event.episode_id):
          add_edge(claim.node, repo_node, type="grounded_in",
                   weight=0.8, confidence=INFERRED)
```

### 2.2 Symbol Mention Extraction

```text
link_claims_to_symbols(claims, repo_symbol_table):
  // Build name set from repo
  symbol_names = set(repo_symbol_table.keys())
  // Filter out common words that happen to be symbol names (< 3 chars, common English)
  symbol_names = symbol_names.filter(name => name.len() >= 3 && !COMMON_WORDS.has(name))

  for claim in claims:
    text = claim.title + " " + claim.summary
    tokens = tokenize(text)

    for token in tokens:
      if symbol_names.has(token):
        candidates = repo_symbol_table[token]
        // Prefer symbols in files touched by this session
        session_files = session.touched_files()
        local_candidates = candidates.filter(c => session_files.has(c.file))

        target = local_candidates.first() ?? candidates.first()
        if target:
          add_edge(claim.node, target.node_id, type="references",
                   weight=0.7, confidence=INFERRED)
```

### 2.3 Repository-Scoped Retrieval

The key insight: instead of searching ALL claims, search only claims connected to the current repository.

```text
repository_scoped_search(query, repo_id, limit):
  // Step 1: Find the repository's file nodes
  repo_files = repo_graph.nodes_of_type("file")

  // Step 2: Traverse to connected sessions
  connected_sessions = set()
  for file in repo_files:
    for edge in file.edges("touched_by"):
      connected_sessions.add(edge.target)

  // Step 3: Collect claim IDs from connected sessions
  candidate_claim_ids = set()
  for session in connected_sessions:
    for claim in session.claims():
      candidate_claim_ids.add(claim.id)

  // Step 4: Vector search within candidates only
  results = vector_search(query, filter=candidate_claim_ids, limit=limit)
  return results
```

**SQL implementation** (efficient single query):

```sql
WITH repo_sessions AS (
  SELECT DISTINCT ke.target_node_id AS session_node
  FROM knowledge_edges ke
  WHERE ke.source_node_id LIKE 'file:%'
    AND ke.edge_type = 'touched_by'
    AND ke.project_id = $1
),
session_memories AS (
  SELECT DISTINCT ke2.target_node_id AS memory_id
  FROM knowledge_edges ke2
  JOIN repo_sessions rs ON ke2.source_node_id = rs.session_node
  WHERE ke2.edge_type IN ('contains_memory', 'derived_from')
)
SELECT m.*
FROM memories m
JOIN session_memories sm ON m.id::text = sm.memory_id
ORDER BY m.embedding <=> $2  -- vector similarity to query
LIMIT $3;
```

## 3. Hierarchical Leiden & Typed Retrieval

### 3.1 Hierarchical Leiden Algorithm

Replace `assign_communities_with_budget` (greedy modularity) with recursive Leiden:

```text
hierarchical_leiden(graph, max_levels=3, min_community_size=5):
  levels = []

  level_0 = leiden(graph, resolution=0.5)  // coarse
  levels.push(level_0)

  for level in 1..max_levels:
    level_n = {}
    for community in levels[level-1]:
      subgraph = graph.subgraph(community.nodes)
      if subgraph.node_count() > min_community_size * 2:
        sub_communities = leiden(subgraph, resolution=1.0 + level * 0.5)
        for sc in sub_communities:
          level_n[f"{community.id}.{sc.id}"] = sc
      else:
        level_n[community.id] = community  // leaf — can't subdivide
    levels.push(level_n)

  return levels
```

**Storage**: Each node gets a `community_path` like `"3.7.12"` (level-0 community 3, level-1 sub-community 7, level-2 leaf 12). This enables:
- `knowledge_communities(level=0)` → 10-30 coarse clusters for architecture overview
- `knowledge_communities(level=2)` → 200+ fine clusters for targeted search
- Community routing: embed query → find matching community at level 1 → search within

### 3.2 God Node Damping

```text
damp_hub_edges(graph, percentile=95):
  degrees = [node.degree for node in graph.nodes]
  threshold = percentile(degrees, 95)

  for node in graph.nodes:
    if node.degree > threshold:
      damping = 1.0 / ln(node.degree)
      for edge in node.edges:
        edge.community_weight = edge.weight * damping
      node.metadata["is_hub"] = true
      node.metadata["hub_type"] = classify_hub(node)

classify_hub(node):
  if node.type == "session": return "session_hub"
  if node.type == "module" && node.label in COMMON_MODULES: return "import_hub"
  if node.id.startswith("file:") && node.degree > 50: return "central_file"
  return "domain_hub"  // This is a genuine architectural anchor (God Node)
```

**Key distinction**: Only `domain_hub` type should appear in `hub_nodes` results. `session_hub` and `import_hub` are structural artifacts, not architectural insights.

### 3.3 Typed Embedding Partitions

Instead of one flat Chroma collection, create per-type collections:

```text
Current:  chroma.collection("memories")  → all 42K claims in one index

Proposed: chroma.collection("memories_bug")       → 7,115 bug claims
          chroma.collection("memories_decision")   → 190 decision claims
          chroma.collection("memories_task")        → 2,829 task claims
          chroma.collection("memories_fix")         → 3,054 fix claims
          chroma.collection("memories_constraint")  → 6,585 constraint claims
          chroma.collection("memories_fact")        → 1,482 fact claims
          chroma.collection("memories_impl_detail") → 20,123 implementation_detail claims
          chroma.collection("memories_open_q")      → 1,234 open_question claims
          chroma.collection("memories_all")         → all 42K (fallback)
```

When `mem_search(types=["bug","fix"])` is called:
1. Search `memories_bug` and `memories_fix` collections in parallel
2. Merge results by score
3. Return top-k

When `mem_search()` is called without types:
1. Search `memories_all` (backward compatible)

**Why this works**: The v2.2.1 benchmark showed `claimTypeFit = 0.343` because a query for "worker bugs" matched `constraint` and `open_question` claims with similar embeddings. By partitioning, we guarantee that a `types=["bug"]` query only returns bugs.

### 3.4 Community-Aware Retrieval

```text
community_aware_search(query, types, limit):
  // Step 1: Embed query
  query_embedding = embed(query)

  // Step 2: Find matching communities (using community summary embeddings)
  community_scores = []
  for community in level_1_communities:
    score = cosine_similarity(query_embedding, community.summary_embedding)
    community_scores.push((community, score))
  top_communities = community_scores.sort_by_score().take(3)

  // Step 3: Within top communities, do typed vector search
  candidates = []
  for community in top_communities:
    community_claim_ids = community.claim_ids()
    results = typed_vector_search(query, types, filter=community_claim_ids, limit=limit)
    candidates.extend(results)

  // Step 4: Re-rank by combined score (vector similarity × community relevance × authority)
  for candidate in candidates:
    candidate.final_score = (
      candidate.vector_score * 0.5 +
      candidate.community_score * 0.3 +
      authority_weight(candidate.authority_class) * 0.2
    )

  return candidates.sort_by_final_score().take(limit)
```

## 4. NeuroPath-Inspired Path Pruning

For multi-hop queries ("what depends on X and was affected by bug Y"):

```text
goal_directed_search(query, start_node, max_hops=3):
  // Parse query into sub-goals
  sub_goals = parse_sub_goals(query)  // e.g., ["depends on X", "affected by bug Y"]

  // BFS from start_node, but prune paths that don't advance toward a sub-goal
  frontier = [(start_node, [], set())]  // (node, path, covered_goals)
  results = []

  for hop in 0..max_hops:
    next_frontier = []
    for (node, path, covered) in frontier:
      for neighbor in node.neighbors():
        // Score how much this edge advances toward uncovered goals
        edge_text = f"{node.label} --{edge.type}--> {neighbor.label}"
        advancement = score_goal_advancement(edge_text, sub_goals - covered)

        if advancement > PRUNE_THRESHOLD:
          new_covered = covered | goals_covered_by(neighbor, sub_goals)
          next_frontier.push((neighbor, path + [edge], new_covered))

          if new_covered == sub_goals:
            results.push((path + [edge], hop + 1))  // Complete path found

    frontier = next_frontier

  return results.sort_by_shortest_path()
```

This is NeuroPath's core insight applied to our graph: instead of retrieving all neighbors and letting the LLM filter, we prune at the graph level based on semantic goal advancement.

## 5. Unified Knowledge Report

The `knowledge_report` endpoint currently returns repository OR session data. With cross-layer edges, it should return a unified view:

```text
knowledge_report(layer="unified"):
  repo = knowledge_report(layer="repository")
  session = knowledge_report(layer="session")

  return {
    repository: {
      files: repo.files,
      symbols: repo.symbols,
      communities: repo.communities,
      hub_nodes: repo.hub_nodes.filter(type="domain_hub")  // Exclude session/import hubs
    },
    sessions: {
      recent: session.recent_sessions,
      active_decisions: session.decisions.filter(unsuperseded),
      open_tasks: session.tasks.filter(uncompleted),
      known_bugs: session.bugs.filter(unresolved)
    },
    cross_layer: {
      most_modified_files: files_by_touch_count(),
      sessions_per_module: sessions_grouped_by_community(),
      claims_per_file: claims_linked_to_files(),
      god_nodes: hub_nodes.filter(type="domain_hub").take(10)
    }
  }
```

This is the "billion dollar" view: for each hub file, you see the sessions that touched it and the decisions/bugs/tasks linked to it. The agent gets full context in one call.

## 6. Files Touched

| Path | Change |
|---|---|
| `rust/crates/chum_mem_pipeline/src/ast_parser.rs` | Containment edges, arrow functions, doc comments, qualified names |
| `rust/crates/chum_mem_pipeline/src/repository.rs` | Two-pass call resolution, confidence scoring, type edges |
| `rust/crates/chum_mem_pipeline/src/knowledge.rs` | Cross-layer edge injection, hierarchical Leiden, God Node damping |
| `rust/crates/chum_mem_pipeline/src/ranking.rs` | Community-aware retrieval, typed partition search |
| `rust/crates/chum_mem_db/src/repos.rs` | Repository-scoped SQL, cross-layer queries |
| `rust/apps/api/src/main.rs` | Unified knowledge_report, typed mem_search routing |
| `rust/apps/worker/src/main.rs` | Cross-layer edge injection in build-knowledge-graph job |
| `scripts/benchmark/live-http.ts` | New test cases for cross-layer, call resolution, containment |

## 7. Research References

1. **GraphRAG** — Edge, D. et al. (2024). "From Local to Global: A Graph RAG Approach to Query-Focused Summarization." arXiv:2404.16130. *Hierarchical Leiden communities, map-reduce global queries.*

2. **NeuroPath** — Li, J. et al. (2025). "NeuroPath: Neurobiology-Inspired Path Tracking and Reflection for Semantically Coherent Retrieval." NeurIPS 2025. arXiv:2511.14096. *Goal-directed path pruning, 16.3% recall improvement, 22.8% token reduction.*

3. **GraphRAG Survey** — Peng, B. et al. (2024). "Graph Retrieval-Augmented Generation: A Survey." arXiv:2408.08921. *Taxonomy of graph indexing (graph/text/vector/hybrid), retrieval (node/triplet/path/subgraph), generation.*

4. **MiniRAG** — Fan, T. et al. (2025). "MiniRAG: Towards Extremely Simple Retrieval-Augmented Generation." arXiv:2501.06713. *Semantic-aware heterogeneous graph indexing, 25% storage.*

5. **Graphify** — Shamsi, S. (2025). https://github.com/safishamsi/graphify. *God Nodes, Leiden clustering, EXTRACTED/INFERRED/AMBIGUOUS edge tags, 71.5× token reduction, cross-file call graphs.*
