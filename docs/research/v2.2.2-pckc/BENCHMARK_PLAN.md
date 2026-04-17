# v2.2.2 Benchmark Plan

Date: 2026-04-16
Branch: `v2.2.2`
Status: implementation complete, pending data reprocessing

## Baseline

v2.2.1 benchmark: 7/12 pass (58%)

## What Changed (code-level)

### AST Parser (ast_parser.rs)
- **Containment edges**: `parent_name` on `AstSymbol`, emitting `contains` edges from class→method
- **Arrow function extraction**: JS/TS `const foo = () => {}` now captured as Function symbols
- **Doc comment population**: `doc_comment` field populated from preceding comment nodes

### Repository Graph (repository.rs)
- **Cross-file call resolution**: Two-pass global symbol table resolves `inferred` call edges → `resolved` (0.9) or `ambiguous` (0.3)
- **Qualified symbol IDs**: `symbol:path:Class.method` instead of `symbol:path:method`
- **Containment edge emission**: `parent_id → child_id` with `contains` relation

### Knowledge Graph (knowledge.rs)
- **File-change anchoring**: `touched_by` edges (file→session, reverse of modifies)
- **Symbol mention extraction**: Memory claims → file nodes via basename matching
- **God Node damping**: 95th percentile degree nodes get `1/ln(degree)` weight reduction
- **Hub type classification**: `hub_nodes` query filters to `domain_hub` and `central_file` only
- **Hierarchical communities**: Level-0 + level-1 sub-clustering for communities ≥20 nodes
- **CommunityInfo extended**: `community_path` and `level` fields

### Ranking (ranking.rs)
- **Type-fit boost**: `+0.25` for matching requested types, `-0.15` for non-matching
- **`requested_types`** field on RankingContext

### DB (repos.rs)
- **Claim-type SQL filter**: `claim_type` filter applied in addition to `memory_type`

### API (main.rs)
- **Unified knowledge_report**: `layer=unified` merges repository + session reports
- **Cross-layer summary**: Most modified files, active decisions, open tasks, known bugs, architectural hubs
- **Type passthrough**: `requested_types` wired from search input to ranking context

## Benchmark Steps

### Step 1: Trigger Reprocessing

The code changes affect graph construction and ranking, not data at rest. To see improvements:

```bash
# Force knowledge graph rebuild for the project
curl -X POST http://localhost:63001/mcp -H 'Content-Type: application/json' -d '{
  "jsonrpc": "2.0",
  "method": "tools/call",
  "params": {
    "name": "repository_sync",
    "arguments": {
      "files": [],
      "mergeWithExisting": false
    }
  },
  "id": 1
}'
```

### Step 2: Re-run Benchmark

```bash
npx tsx scripts/benchmark/live-http.ts \
  --base-url=http://localhost:63001 \
  --quality-only \
  --output=docs/research/v2.2.2-pckc/results/benchmark-v222.json
```

### Step 3: Expected Improvements

| Metric | v2.2.1 | Expected v2.2.2 | Mechanism |
|--------|--------|-----------------|-----------|
| retrieval_noise.relevantTop5 | 1 | ≥3 | Repository-scoped retrieval + type-fit boost |
| retrieval_noise.irrelevantTop5 | 4 | ≤2 | Type-fit penalty (-0.15) demotes wrong types |
| continuation.claimTypeFit | 0.343 | ≥0.7 | claim_type SQL filter + type-fit boost (+0.25) |
| context_build.repository_only.fillRate | 0.125 | ≥0.3 | Deeper repo graph (containment, calls, doc comments) |
| hub_nodes quality | mixed types | domain_hub only | God Node classification + filtering |
| "what calls X" | impossible | works | Cross-file call resolution |
| "methods of class Y" | impossible | works | Containment edges |

### Step 4: New Benchmark Cases to Add

For the next benchmark script update:

1. **Containment query**: `knowledge_query(neighbors, nodeId:"symbol:path:ClassName")` → should return method children
2. **Cross-file call query**: `knowledge_query(neighbors, nodeId:"symbol:path:fn")` → should show callers from other files
3. **Typed search precision**: `mem_search(query, types:["bug"])` → verify 100% bug type in results
4. **Hub quality**: `knowledge_query(hub_nodes)` → verify no session_hub or import_hub types
5. **Community hierarchy**: `knowledge_communities()` → verify level-0 and level-1 communities exist
6. **Unified report**: `knowledge_report(layer:unified)` → verify cross-layer summary present

### Step 5: Regression Checks

Ensure these still pass:
- repository.exact_file_path.top1 = true
- repository.exact_symbol.top1 = true
- cross_layer.leak_count = 0
- belief_gate.reasoning_leak = 0
- continuation.supersededInTop5.total = 0

## Success Criteria

Target: 10/12 pass (83%), up from 7/12 (58%)

Critical improvements:
- `claimTypeFit` from 0.343 → ≥0.7 (type partition + boost)
- `retrieval_noise` relevance from 1/5 → ≥3/5 (type-fit + cross-layer)
- Hub nodes return only architectural hubs (not session/import noise)
