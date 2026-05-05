# chum-mem build instruction

Build `chum-mem` as a cloud-native persistent memory platform for coding agents.

The current target is the v2.2.3 PCKC runtime:

- claims are the unit of memory
- proof is the unit of trust
- compiled minimal proof sets are the unit of context
- repository and session graphs are first-class retrieval surfaces
- retrieval is hybrid: PostgreSQL lexical + pgvector + Chroma typed partitions

The product should capture session activity, derive durable typed memory with provenance, make it searchable, and inject compact evidence into future sessions through MCP tools and API endpoints.

## Scope

Support these clients as first-class providers:

- Claude
- Codex
- Gemini

Run `chum-mem` as:

- a self-hosted MCP server backed by PostgreSQL, pgvector, and Chroma-assisted retrieval
- a multi-tenant service with organizations, teams, members, projects, and user-generated API tokens
- a graph-backed repository and session memory system, not only a transcript store

## Core loop

1. Capture normalized session events, tool calls, prompts, outputs, files touched, git metadata, and optional summaries.
2. Derive typed claims and supporting memories from those events.
3. Attach proof, authority, verification status, and temporal validity.
4. Build repository and session knowledge graphs.
5. Retrieve evidence through hybrid lexical, semantic, and graph-aware ranking.
6. Compile compact context packs with hard token budgets and proof-gap signaling.

## Operating principles

- default to verified atomic truth, not broad session narration
- preserve provenance for every durable claim
- preserve supersession and contradiction semantics
- keep provider-specific logic behind adapters
- keep tenant resolution server-side
- never store plaintext API tokens
- do not trust caller-supplied tenant identifiers
- prefer benchmark-driven retrieval changes over intuition-only tuning

## Memory model

Durable memory is claim-centric.

Primary memory types:

- `fact`
- `decision`
- `task`
- `constraint`
- `bug`
- `fix`
- `open_question`
- `implementation_detail`

Supporting memory types:

- `summary`
- `change_log`
- `risk`

Each durable claim or memory should preserve:

- title
- normalized content
- short summary
- `claim_type`
- authority class
- verification status
- proof handles
- provenance links to session events
- temporal validity
- supersession state
- searchable metadata

## Knowledge model

The runtime uses two graph layers:

- repository layer: code structure derived from repository sync and AST parsing
- session layer: interaction history derived from session events, episodes, claims, tools, tests, and file changes

v2.2.3 capabilities that must be preserved:

- containment edges
- cross-file call resolution
- typed embedding partitions
- hierarchical Leiden communities with `level` and `community_path`
- graph-aware ranking
- project-scoped graph/community caching
- continuation-aware ranking (is_continuation flag, actionable-claim boosting)
- section-aware context assembly (baseline queries for all 6 core section types)
- deterministic memory governance (active/pinned/archived/rejected with audit history)
- governance-aware scoring and SQL filtering

## Identity and access

Use server-managed users and API tokens for machine access.

Data model:

- user
- organization
- team
- team_member
- project
- api_token
- session
- session_event
- memory
- claim
- claim_proof
- claim_edge
- claim_governance_history
- knowledge_snapshot

Rules:

- a user can belong to multiple teams
- each team owns projects and memory
- each member can generate multiple API tokens
- tokens are scoped to team and optionally project
- only hashed token values are stored
- tokens are shown once at creation time
- all tenant-owned rows must carry tenant keys

## Required MCP tools

Expose these capabilities through MCP tools and thin HTTP wrappers where needed:

- `session_start`
- `session_event_append`
- `session_end`
- `repository_sync`
- `mem_search`
- `memory_get`
- `memory_get_batch`
- `context_build`
- `context_compile_v2`
- `knowledge_query`
- `knowledge_report`
- `knowledge_communities`
- `knowledge_graph_export`
- `graph_snapshot`
- `health_check`

## Retrieval contract

All clients should follow the graph-first compact retrieval workflow:

1. `knowledge_report(layer:"repository")` first, in Markdown form, as the primary high-level repository context
2. repository-layer `knowledge_query` next for components, architecture, and relationships
3. `mem_search` after graph context, compact output, small `limit`
4. `memory_get_batch` only for selected IDs
5. `context_build` or `context_compile_v2` only after filtering

Repository questions should default to `knowledge_query(search, layer:"repository")` plus targeted file reads.
File-level search tools are fallback only after the report, repository query, and compact memory search.

`context_compile_v2` is the proof-disciplined path:

- hard token ceiling
- drops inadmissible claims
- prefers current-valid claims
- emits `proof_gap` markers instead of silently truncating

## Non-functional requirements

- strict tenant isolation with PostgreSQL RLS and server-side scope resolution
- low-latency hybrid retrieval
- idempotent ingestion
- observable services with structured logging and tracing
- benchmarkable ranking behavior
- conflict-aware and proof-aware retrieval
- reproducible local and production deployment
- MCP transport support for local and remote agent clients

## Delivery priorities

### Priority 1

- secure ingestion and token auth
- durable claim/proof storage
- tenant-safe hybrid retrieval
- repository and session graph persistence
- MCP server bootstrap

### Priority 2

- typed retrieval and graph-aware ranking
- hard-budget proof compilation
- knowledge report and graph inspection surfaces
- provider adapters for Claude, Codex, and Gemini

### Priority 3

- richer admin/dashboard surfaces
- stronger continuation retrieval
- better typed section fill in context assembly
- deeper cross-layer reporting and governance workflows

## Definition of success

`chum-mem` is successful when:

- memory quality improves as knowledge grows
- irrelevant evidence is suppressed
- verified corrections outrank stale hypotheses
- context packs remain compact and provenance-aware
- repository understanding reduces fallback to grep
- tenant isolation and auditability hold under all trusted code paths
