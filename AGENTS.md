# AGENTS.md

This repository is built with Codex using parallel agents. Treat this file as the authoritative operating guide for any agent working in the repo.

## Product summary

`chum-mem` is a cloud-native persistent memory system for coding agents. It supports Claude, Codex, and Gemini clients and uses PostgreSQL, pgvector, and Chroma-backed hybrid retrieval. The current architecture is the v2.2.2 PCKC runtime:

- claims are the unit of memory
- proof is the unit of trust
- compiled minimal proof sets are the unit of context
- repository and session graphs are first-class retrieval surfaces
- retrieval is hybrid: lexical + pgvector + Chroma

The system supports organizations, teams, members, projects, and personal API tokens used by local clients to connect to the MCP server.

## Primary goals

- implement a secure multi-tenant backend
- support provider-normalized ingestion and retrieval
- provide an MCP-first memory service with optional admin surfaces
- make the answering model more reliable as knowledge grows, not less reliable
- preserve reproducible agent work and low-friction handoff

## Source of truth

Use these files first:

1. `docs/INSTRUCTION.md`
2. `docs/ARCHITECTURE_SPEC.md`
3. this `AGENTS.md`
4. `docs/research/v2.2.2-pckc/README.md`
5. `docs/research/v2.2.2-pckc/DESIGN.md`
6. `docs/research/v2.2.2-pckc/results/COMPARISON.md`
7. relevant skill in `.codex/skills/`
8. relevant prompt in `.codex/prompts/`

If research docs, benchmarks, and runtime code disagree, prefer:

1. architecture spec
2. current runtime code
3. benchmark artifacts
4. older research notes

When you discover a real mismatch, update docs in the same task or leave a precise follow-up note.

## Working rules

- make focused changes with clear scope
- do not rewrite architecture without updating the spec
- preserve tenant isolation in all data-access code
- prefer explicit types and runtime validation
- add tests for any non-trivial behavior
- do not store plaintext API tokens
- do not trust caller-supplied tenant identifiers
- keep provider-specific logic behind adapters
- preserve provenance for memory derivation and retrieval
- preserve authority and verification metadata on claims
- preserve supersession and contradiction semantics
- do not replace proof-carrying claims with narrative summaries

## Default memory gate

For every new code task, use `chum-memory` first.

Required flow:

1. call `knowledge_query(search, layer:"repository")` and `mem_search` in parallel before reading or grepping
2. keep `mem_search` compact: `mode=hybrid`, `disclosureLevel=overview`, small `limit`
3. select relevant IDs from results
4. call `memory_get_batch` only for selected IDs
5. call `context_build` or `context_compile_v2` only when a compact proof set is actually needed

Do not skip this flow unless the user explicitly asks to continue without memory.

Repository questions should default to repository truth. Session memory is a secondary witness for decisions, tasks, bugs, and continuity.

## PCKC expectations

v2.2.2 is not generic summary memory. It is a proof-carrying claim system.

Key operational rules:

- prefer typed claims over prose summaries
- prefer `verified`, `user_confirmed`, `tool_verified`, `test_verified`, and repository-derived truth
- treat `model_derived` and `unverified` claims as non-durable unless explicitly justified by the runtime contract
- never cite superseded claims as current truth
- surface conflicts explicitly; do not silently average contradictory claims
- use `context_compile_v2` when hard budget discipline and proof-gap signaling matter
- use `knowledge_report` and `knowledge_query` for graph-native reasoning before falling back to grep

## Current architecture snapshot

The applied v2.2.2 architecture includes:

- typed claim extraction with authority and verification metadata
- belief gate enforcement during derivation
- typed embedding partitions for claim-type-aware retrieval
- deep repository graph with containment edges and cross-file call resolution
- session graph construction with restored inter-claim edges
- hierarchical Leiden communities with `level` and `community_path`
- project-scoped community caching for graph-heavy queries
- hard-budget minimal-proof compilation via `context_compile_v2`
- unified knowledge reporting in runtime code, even if some older benchmark notes lag behind

Known active gaps from the latest benchmark/docs that may still drive implementation work:

- continuation recall remains weaker than claim-type precision
- typed section fill for context assembly still needs improvement
- some research docs may lag behind runtime behavior

## Expected repository layout

```text
apps/
  web/
docs/
  research/
extensions/
  chum-memory-gemini/
infra/
  migrations/
packages/
  contracts/
  db/
plugins/
  chum-memory/
  chum-memory-claude/
rust/
  apps/
    api/
    worker/
  crates/
    chum_mem_contracts/
    chum_mem_db/
    chum_mem_pipeline/
scripts/
  benchmark/
.codex/
```

Guidance:

- Rust under `rust/` is the trusted runtime for API, worker, retrieval, graph, and derivation behavior
- `apps/web` is the dashboard and inspection surface
- `infra/migrations` is the reviewable schema history
- `plugins/` and `extensions/` carry provider/plugin integration surfaces
- `packages/` contains supporting TS packages, not the primary runtime

## Branch and task hygiene

- one branch or worktree per major task
- one agent thread per bounded objective
- use small PR-sized diffs
- leave concise progress notes in commit messages or task logs
- if a task updates runtime behavior, update the relevant docs in the same diff

## Definition of done

A task is done only when all are true:

- code builds
- tests relevant to the change pass
- types pass
- changed behavior is documented when needed
- security and tenant implications were considered
- proof/provenance implications were considered for retrieval or memory changes

## Recommended agent split

### Agent 1: architecture and contracts
Own:
- MCP and HTTP contracts
- claim and context-pack schemas
- migration planning
- architecture/doc updates

### Agent 2: backend platform
Own:
- auth integration
- token service
- ingestion APIs
- RLS-safe data access

### Agent 3: retrieval and memory pipeline
Own:
- claim derivation
- belief gate behavior
- embeddings and hybrid search
- ranking, compiler, and graph retrieval

### Agent 4: knowledge graph and runtime performance
Own:
- repository graph extraction
- session graph build/merge
- community detection
- graph reports, graph caching, and latency-sensitive query paths

### Agent 5: web dashboard
Own:
- auth UX
- team/project pages
- token management UI
- memory and graph inspection UI

### Agent 6: QA and security
Own:
- threat review
- integration tests
- tenancy tests
- API misuse tests
- retrieval regressions and proof-integrity checks

### Agent 7: Postgres DB engineer
Own:
- schema design and migrations
- query and index tuning, including pgvector behavior
- lock, deadlock, and advisory-lock analysis
- vacuum, WAL, and memory tuning
- RLS policies and tenant isolation at the database layer

## Commands policy

Before running destructive or networked commands, explain intent in the thread. Prefer reproducible scripts over ad hoc shell commands.

## Coding standards

- Rust is the primary runtime language for API, worker, retrieval, and graph code
- TypeScript remains appropriate for dashboard, scripts, and supporting packages
- Zod is acceptable for TS-facing contracts; Rust contracts must remain explicit and reviewable
- SQL migrations must be explicit and reviewable
- isolate side effects behind service boundaries
- log with stable machine-readable fields
- preserve backwards-compatible MCP behavior unless the task explicitly changes the contract

## PostgreSQL rules

- all tenant tables must include tenant keys
- RLS policies must be added in the same migration as table creation when possible
- use transaction-scoped application settings for tenant resolution in server-trusted code paths
- prefer database-enforced constraints over app-only checks
- preserve auditability of claim lifecycle changes

## Token rules

- generate high-entropy secrets server-side
- hash before persistence
- show plaintext once only
- record `last_used_at`
- support revoke and expiry

## Retrieval rules

- support lexical and semantic search with graph-aware ranking
- preserve typed claim retrieval behavior
- keep provenance links from memory back to session events
- preserve authority and verification metadata through ranking and response mapping
- context builders must respect token budgets
- `context_compile_v2` must emit `proof_gap` markers rather than silently truncating
- retrieval output must be compact, ranked, deduplicated, and proof-aware
- repository questions should prefer repository graph evidence over session narration

## Knowledge graph rules

- preserve repository/session layer separation unless the task explicitly changes the model
- preserve cross-layer links and community metadata
- do not regress containment edges, cross-file call resolution, or hierarchical community persistence
- keep hub analysis resistant to noise hubs
- benchmark or at least sanity-check latency when changing graph-heavy query paths

## Memory derivation rules

- claims should remain atomic and typed
- reasoning and turn-context events must stay belief-gated out of durable claim origination
- preserve supersession, contradiction, and conflict surfacing behavior
- prefer append-only or audit-friendly updates for durable memory lifecycle changes

## Benchmark discipline

When changing retrieval, graph, compiler, or derivation behavior:

- inspect `scripts/benchmark/live-http.ts`
- compare against the latest v2.2.2 benchmark expectations
- call out likely metric impact in the task notes
- avoid shipping retrieval improvements that regress belief-gate integrity or tenant isolation

## If uncertain

Do not guess silently. State the assumption, make the smallest safe change, and leave a clear note for follow-up.
