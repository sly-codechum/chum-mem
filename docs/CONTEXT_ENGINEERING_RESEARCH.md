# chum-mem context engineering research notes

## 1. Why the current implementation will context-rot

Observed from the current runtime paths:

- `[apps/api/src/ingestion.ts](./apps/api/src/ingestion.ts)` derives only coarse summary, bug, and implementation-detail memories from the full session.
- `[apps/api/src/index.ts](./apps/api/src/index.ts)` already persists provenance and can load session-linked evidence, but ranking does not use session relevance as a first-class score.
- `[packages/retrieval/src/index.ts](./packages/retrieval/src/index.ts)` merges lexical hits with Chroma results but does not use the stored graph or explicit freshness and supersession penalties.
- `[services/worker/src/index.ts](./services/worker/src/index.ts)` uses polling sync into Chroma, which introduces lag and drift between the canonical store and the semantic index.

That combination creates three failure modes:

1. large session blobs dominate search because compaction is too coarse
2. retrieval loses thread continuity because `session_id` is exposed in provenance but not scored as a primary relevance signal
3. stale or superseded memories can remain highly ranked because invalidation is not part of ranking

## 2. Research-backed architecture implications

### Long context alone is not the answer

- "Lost in the Middle: How Language Models Use Long Contexts"
  - link: [arXiv](https://arxiv.org/abs/2307.03172)
  - implication for `chum-mem`: do not solve retrieval by simply sending more retrieved text. The system must place the smallest, highest-value evidence in the prompt and keep important evidence near the final packed context boundary.

### Memory should be hierarchical and externalized

- "MemGPT: Towards LLMs as Operating Systems"
  - link: [arXiv](https://arxiv.org/abs/2310.08560)
  - implication for `chum-mem`: explicitly separate hot context, episodic memory, and durable semantic memory. Retrieval should page information in and out instead of treating the prompt as the only memory layer.
- "LongMem: Empowering Large Language Models with Long-Term Memory"
  - link: [arXiv](https://arxiv.org/abs/2306.07174)
  - implication for `chum-mem`: retrieval should reuse stored long-term memory instead of reprocessing long histories. Session episodes are the correct intermediate structure between raw events and durable memory.

### Pure vector retrieval is not enough

- "From RAG to Memory: Non-Parametric Continual Learning for Large Language Models" (HippoRAG 2)
  - link: [arXiv](https://arxiv.org/abs/2502.14802)
  - implication for `chum-mem`: combine vector retrieval with graph-based and associative retrieval. Session and memory edges should participate directly in candidate generation and reranking.
- "RAPTOR: Recursive Abstractive Processing for Tree-Organized Retrieval"
  - link: [arXiv](https://arxiv.org/abs/2401.18059)
  - implication for `chum-mem`: generate hierarchical summaries over episodes and cross-session clusters so context packs can retrieve compact abstractions before expanding to raw evidence.

### Retrieval should be query-aware and self-correcting

- "Self-RAG: Learning to Retrieve, Generate, and Critique through Self-Reflection"
  - link: [arXiv](https://arxiv.org/abs/2310.11511)
  - implication for `chum-mem`: retrieval should depend on query intent. A debugging query should bias toward recent failures, same-file sessions, and unsuperseded fixes; a planning query should bias toward decisions and reflection memories.

### Agent memory needs reflection and consolidation

- "Generative Agents: Interactive Simulacra of Human Behavior"
  - link: [arXiv](https://arxiv.org/abs/2304.03442)
  - implication for `chum-mem`: add reflection memories that periodically consolidate repeated session patterns into higher-value summaries. This is the right place to fight context rot over time.

## 3. Architecture decisions for chum-mem

### A. Use PostgreSQL + pgvector as the canonical retrieval substrate

Keep the primary search path in PostgreSQL:

- lexical retrieval from `search_vector`
- semantic retrieval from `public.embeddings`
- graph expansion from `memory_edges` and new `session_edges`

Chroma can remain optional, but it should not be the source of truth for core ranking.

### B. Add episode compaction before durable memory extraction

Introduce `session_episodes`:

- each episode covers a coherent span inside one session
- each episode stores summary, type, files, symbols, errors, and outcome
- durable memories are extracted from episodes, not from the whole session blob

This is the main fix for context rot caused by over-broad summaries.

### C. Make `session_id` a ranking feature, not just provenance

Every retrieval candidate should be scored using:

- exact same `session_id`
- same branch or commit lineage
- adjacent sessions touching the same files
- sessions connected by shared error signatures or memory edges

This is required for "continue where I left off" reliability.

### D. Treat freshness and supersession as first-class

Add retrieval penalties and invalidation rules:

- old active-task memories decay quickly
- durable decisions decay slowly
- a later successful fix can supersede an earlier bug memory
- contradictory memories remain visible but must surface uncertainty

This is the main fix for stale but semantically similar memories beating newer ones.

### E. Build context packs for coverage, not only top-k similarity

A good context pack should include:

- one or more high-confidence facts or decisions
- same-session or same-branch continuity when applicable
- unsuperseded bug/task state
- short provenance excerpts for verification

The pack should maximize useful coverage under budget, not maximize raw similarity score.

## 4. Immediate implementation roadmap

### Phase 1

- move primary semantic retrieval to PostgreSQL `embeddings`
- add `session_id` and freshness-aware ranking features to `packages/retrieval`
- return matched `session_id` values in search hits

### Phase 2

- add `session_episodes`
- derive episode summaries and episode-local metadata
- extract memories from episodes instead of whole-session aggregates

### Phase 3

- add `session_edges`
- compute graph-aware candidate expansion and reranking
- add supersession and contradiction handling

### Phase 4

- add reflection memories and periodic summary regeneration
- add offline retrieval evaluation with precision, recall, MRR, and continuation-success metrics

## 5. Evaluation metrics

Track at least:

- retrieval precision at 5
- MRR for exact-memory lookup tasks
- continuation hit rate
  - the right prior session is present in the top results
- stale-memory rate
  - a superseded memory appears above the active memory
- context-pack usefulness
  - the packed context leads to fewer follow-up retrieval calls and fewer user corrections

