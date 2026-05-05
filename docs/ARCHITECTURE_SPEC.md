# chum-mem architecture spec

## 1. Objective

`chum-mem` is a multi-tenant persistent memory system for coding agents. It ingests normalized provider events from Claude, Codex, and Gemini clients, derives typed durable claims with proof, stores them in PostgreSQL-backed tenant-safe data structures, and serves compact evidence through MCP tools and API endpoints.

The current target architecture is v2.2.3 PCKC:

- claims are the unit of memory
- proof is the unit of trust
- compiled minimal proof sets are the unit of context
- repository and session graphs are first-class retrieval surfaces
- retrieval is hybrid across lexical, vector, and graph signals

The system must optimize for:

- tenant safety
- predictable retrieval quality
- low operational complexity
- benchmark-driven evolution
- Docker-native deployment
- parallel agent workflows
- resistance to context rot across long-running multi-session work

## 2. System context

### External actors

- human users
- team admins
- local provider clients or plugins
- background workers
- observability systems

### External systems

- PostgreSQL
- pgvector
- Chroma
- optional object storage for large artifacts
- optional Redis or equivalent queue/cache infrastructure

## 3. Current runtime implementation

The repo’s primary runtime is Rust:

- `rust/apps/api`: MCP and HTTP server
- `rust/apps/worker`: background jobs and graph persistence
- `rust/crates/chum_mem_pipeline`: derivation, retrieval, graph, and compilation logic
- `rust/crates/chum_mem_db`: SQL-backed data access and reconciliation
- `rust/crates/chum_mem_contracts`: shared contracts and enums

Supporting surfaces:

- `apps/web`: dashboard and graph inspection UI
- `plugins/` and `extensions/`: provider/plugin packaging and host integration
- `infra/migrations`: explicit database schema history

PostgreSQL remains the durable source of truth. Chroma is used for typed embedding collections in the semantic retrieval path. pgvector remains part of the hybrid retrieval model and durability/experimentation story.

## 4. High-level architecture

```text
Claude / Codex / Gemini clients
        |
        v
Provider adapters / plugin hooks
        |
        v
MCP + HTTP API server
        |
        +--> token auth / tenant scoping
        +--> ingestion service
        +--> hybrid retrieval service
        +--> context builder / proof compiler
        +--> knowledge graph query/report service
        |
        v
PostgreSQL + pgvector + Chroma
        |
        +--> team/project/auth tables
        +--> sessions / session_events / session_episodes
        +--> memories
        +--> claims / claim_proofs / claim_edges
        +--> embeddings and retrieval metadata
        +--> knowledge_snapshots / communities / artifacts
        |
        v
Worker runtime
        +--> derivation
        +--> claim extraction
        +--> proof attachment
        +--> contradiction / supersession updates
        +--> repository sync graph build
        +--> session graph build / merge
        +--> community detection
```

## 5. Core concepts

### 5.1 Memory units

The durable unit is a typed claim linked to a memory record.

Supported memory types:

- `fact`
- `decision`
- `task`
- `constraint`
- `bug`
- `fix`
- `open_question`
- `summary`
- `implementation_detail`
- `change_log`
- `risk`

### 5.2 Trust units

Each durable claim carries:

- `authority_class`
- `verification_status`
- `proof_handles`
- temporal validity
- supersession state

Canonical authority classes:

- `repository`
- `user_confirmed`
- `tool_verified`
- `test_verified`
- `session_derived`
- `model_derived`

Canonical verification statuses:

- `verified`
- `user_confirmed`
- `inferred`
- `contradicted`
- `unverified`

### 5.3 Context units

There are two context assembly paths:

- `context_build`: compact context packing
- `context_compile_v2`: proof-disciplined minimal cover with hard token ceilings and `proof_gap` signaling

### 5.4 Retrieval prelude

Client agents must build repository context before memory recall:

1. retrieve the repository `knowledge_report` in Markdown form
2. run a repository-layer `knowledge_query` for relevant components, architecture, and relationships
3. run compact `mem_search`
4. only then use targeted file reads or file-level search

This preserves the PCKC v2.2.3 model by anchoring work in repository graph truth before claim recall and proof expansion.

## 6. Multi-tenant model

### 6.1 Entities

- `organizations`
- `teams`
- `team_members`
- `projects`
- `api_tokens`
- `sessions`
- `session_events`
- `session_episodes`
- `memories`
- `claims`
- `claim_proofs`
- `claim_edges`
- `knowledge_snapshots`
- `knowledge_communities`
- `knowledge_snapshot_heads`
- `knowledge_snapshot_artifacts`
- `audit_logs`

### 6.2 Isolation model

All tenant-owned rows must include:

- `organization_id`
- `team_id`

Project-owned rows must also include:

- `project_id`

Isolation rules:

- all tenant access resolves server-side from user or token identity
- clients do not control tenant scope directly
- project-scoped tokens cannot access out-of-scope projects
- durable memory and graph data inherit the tenant/project scope of their originating work

### 6.3 RLS strategy

Use PostgreSQL RLS on all tenant tables. Policies must check server-set transaction-scoped application settings or equivalent trusted helpers.

Requirements:

- reject revoked or expired tokens before trusted queries begin
- use database-enforced scope checks, not app-only filtering
- apply the same tenant discipline to claims, proofs, graph snapshots, and community rows

## 7. Provider adapter architecture

All providers must normalize into the same ingestion and retrieval contract.

Normalization goals:

- preserve the raw provider payload
- emit stable canonical event types
- preserve repo, branch, file, and session context
- avoid polluting the canonical schema with provider-only semantics

Canonical event types:

- `prompt`
- `response`
- `tool_call`
- `tool_result`
- `file_change`
- `command`
- `test_result`
- `summary`
- `error`
- `annotation`
- `reasoning`
- `turn_context`
- `agent_message`

## 8. Ingestion pipeline

### 8.1 Session start

Inputs:

- provider
- project identifier
- external session identifier
- repo metadata
- local/client metadata

Server actions:

- authenticate caller
- resolve tenant/project scope
- create or resume session
- emit audit information

### 8.2 Event append

Server actions:

- validate payload
- deduplicate on `idempotency_key`
- persist raw event
- extract cheap retrieval metadata immediately
- preserve turn boundaries and file-change information

### 8.3 Session end

Server actions:

- mark session complete
- derive episodes and typed memories
- attach proofs
- update contradiction / supersession state
- enqueue or run session graph build
- emit audit information

## 9. Memory derivation model

### 9.1 Event to episode

Segment sessions into coherent work episodes such as:

- debugging
- implementation
- refactor
- planning
- investigation

Each episode preserves:

- ordered event span
- `session_id`
- episode type
- timestamps
- compact summary metadata

### 9.2 Episode to memory and claims

Derivation produces reusable units:

- memory record
- claim record
- claim proofs
- claim edges where applicable

Each derived claim should preserve:

- `claim_key`
- `claim_type`
- subject / predicate / object
- polarity
- authority class
- verification status
- admission state
- validity window
- supersession link

### 9.3 Belief gate

The belief gate is mandatory.

Rules:

- reasoning traces and `turn_context` are stored as events but must not originate durable claims
- model-derived unverified prose is not current truth by default
- defense-in-depth filtering must also exist during compilation and ranking

### 9.4 Contradiction and supersession

The system must preserve:

- current-valid claims
- historical superseded claims
- explicit contradictory claims

Ranking and compilation must prefer:

- admitted, unsuperseded, current-valid claims
- higher-authority proof-backed corrections over older weak hypotheses

## 10. Knowledge graph architecture

### 10.1 Layer separation

There are two graph layers:

- `repository`
- `session`

These layers are stored separately through `snapshot_type` and queried independently unless a unified report is requested.

### 10.2 Repository layer

The repository layer is AST-derived and should include:

- files
- symbols
- imports
- containment edges
- cross-file calls
- documentation structure
- rationale/comment nodes where supported
- Graphify-style multimodal repository artifacts:
  - text/markup sections from `.md`, `.mdx`, `.html`, `.txt`, `.rst`, `.yaml`, and `.yml`
  - Office Open XML text units from `.docx`, `.xlsx`, and `.pptx`
  - best-effort PDF text units
  - image/media metadata nodes for `.png`, `.jpg`, `.webp`, `.gif`, `.mp4`, `.mov`, `.mp3`, and `.wav`
  - parse diagnostic nodes for corrupt, unsupported, or metadata-only files

v2.2.2 repository expectations:

- containment edges are first-class
- cross-file call resolution is applied in the sync path
- repository reasoning should reduce fallback to grep

### 10.3 Session layer

The session layer is event- and claim-derived and should include:

- sessions
- episodes
- claims/memories
- file changes
- commands
- tools
- tests
- errors

The session layer should preserve inter-claim connectivity rather than leaving claims as isolated leaves.

### 10.4 Communities

Community detection uses hierarchical Leiden clustering.

Persist:

- `community_id`
- `level`
- `community_path`
- cohesion score
- representative nodes
- bridge nodes

The system must support:

- level-0 communities
- level-1 sub-communities
- project-scoped community caching

### 10.5 Reports and queries

The graph service must support:

- search
- hub nodes
- neighbors
- shortest path
- communities
- goal-directed graph retrieval
- human-readable reports

The runtime supports repository, session, and unified reporting. Public tool schemas should remain aligned with that behavior.

## 11. Search and retrieval

### 11.1 Search modes

- lexical
- semantic
- hybrid

Retrieval intents:

- `none`
- `memory_only`
- `repository_only`
- `session_graph_only`
- `hybrid`

### 11.2 Candidate generation

Generate candidates from multiple channels:

1. PostgreSQL lexical search
2. vector/embedding search
3. Chroma typed collections
4. session graph proximity
5. repository graph proximity

No single channel should dominate by default.

### 11.3 Typed retrieval

Typed retrieval is a v2.2.2 requirement.

Requirements:

- route `types`-constrained searches to per-type semantic partitions
- preserve soft type filtering when exact matches are sparse
- prevent continuation and bug/fix queries from being overwhelmed by unrelated claim types

### 11.4 Ranking model

Ranking must be feature-based and proof-aware.

Important inputs include:

- lexical score
- semantic score
- session relevance
- graph proximity
- community relevance
- recency
- importance
- confidence
- freshness penalty
- superseded penalty
- claim-type fit

Operational guidance:

- content match should dominate
- stale graph centrality should not dominate ranking
- contradictions must be surfaced, not averaged away

### 11.5 Retrieval workflow

Default retrieval workflow:

1. `knowledge_report(layer:"repository")` first, in Markdown form
2. repository-layer `knowledge_query` for relevant components, architecture, and relationships
3. compact `mem_search` after graph context
4. `memory_get_batch` only for selected IDs
5. `context_build` or `context_compile_v2` only when necessary

## 12. Context assembly

### 12.1 `context_build`

Purpose:

- assemble compact context packs for active model work

Typical sections:

- current truth
- project facts
- recent decisions
- active tasks
- constraints
- known bugs
- verified fixes
- open questions
- implementation notes
- repository knowledge
- session continuity
- conflicts
- proof handles

### 12.2 `context_compile_v2`

Purpose:

- compile the smallest admissible proof set that covers the objective’s sub-goals under a hard token budget

Requirements:

- filter out inadmissible claims
- prefer current-valid claims
- refuse silent truncation
- emit `proof_gap` markers when coverage exceeds budget

### 12.3 Context-rot controls

The system should fight context rot by design:

- prefer atomic truth over long narration
- preserve contradiction markers
- decay or de-prioritize low-value transcript-like material
- favor repository evidence for repository questions
- favor current-valid decisions/tasks for continuation questions

## 13. Authentication and authorization

### 13.1 Machine auth

API tokens must:

- be generated server-side
- be high entropy
- be hashed before persistence
- be shown once only
- record `last_used_at`
- support expiration and revocation

### 13.2 Scope model

Example scopes:

- `ingest`
- `search`
- `context:read`
- `project:write`
- `team:admin`

### 13.3 Trusted code paths

All trusted code paths must:

- resolve scope server-side
- apply transaction-scoped tenant settings
- write auditable records for sensitive operations

## 14. Database schema outline

### 14.1 Identity and tenancy tables

- `organizations`
- `teams`
- `team_members`
- `projects`
- `api_tokens`

### 14.2 Session and memory tables

- `sessions`
- `session_events`
- `session_episodes`
- `memories`

### 14.3 Claim tables

- `claims`
- `claim_proofs`
- `claim_edges`

Claim storage requirements:

- one claim row per memory/claim unit
- authority and verification metadata on the claim
- proof rows with source references and excerpts
- claim edges for supersedes / contradicts / confirms / depends_on / derived_from style relations

### 14.4 Graph tables

- `knowledge_snapshots`
- `knowledge_snapshot_heads`
- `knowledge_snapshot_artifacts`
- `knowledge_communities`

### 14.5 Supporting tables

- embeddings and retrieval metadata tables
- audit log tables

## 15. MCP and HTTP surface

### 15.1 MCP tools

Required MCP tools:

- `session_start`
- `session_event_append`
- `session_end`
- `repository_sync`
- `health_check`
- `mem_search`
- `memory_get`
- `memory_get_batch`
- `context_build`
- `context_compile_v2`
- `graph_snapshot`
- `knowledge_graph_export`
- `knowledge_report`
- `knowledge_query`
- `knowledge_communities`

### 15.2 HTTP endpoints

Required server surfaces include:

- MCP endpoint(s)
- health/readiness
- graph/report inspection where appropriate

The exact transport can evolve, but the normalized capability surface must remain stable for provider clients.

## 16. Reliability and performance

### 16.1 Reliability

- require idempotency keys on append paths
- keep worker jobs restart-safe
- preserve exactly-once semantics where feasible and deduplicate otherwise
- avoid mixing stale artifact reports with current graph output

### 16.2 Performance

Graph-heavy queries must avoid repeatedly loading large snapshots on hot paths.

Expected measures:

- project-scoped graph/community caching
- bounded candidate generation
- typed semantic partitions
- batch upserts and bounded sync jobs

### 16.3 Benchmark discipline

Changes to retrieval, derivation, compilation, or graph logic should be validated against the benchmark harness in `scripts/benchmark/live-http.ts`.

The architecture should preserve or improve:

- retrieval noise suppression
- claim-type fit
- continuation quality
- context fill quality
- proof-gap correctness
- graph latency

## 17. Observability

Implement:

- structured logs
- request and correlation IDs
- tracing across ingestion, retrieval, and worker flows
- metrics for ingestion, retrieval latency, graph latency, compilation latency, token failures, and queue depth
- audit records for token use, memory access, and sensitive governance operations

## 18. Security requirements

- hash API tokens with a strong keyed or slow-hash strategy
- protect secrets via managed secret storage
- minimize sensitive prompt retention where possible
- encrypt sensitive configuration at rest
- preserve tenant isolation on all graph and claim tables
- treat proof and provenance as potentially sensitive tenant data

## 19. Deployment model

Recommended environments:

- local development
- preview/staging
- production

Deployment units:

- API server
- worker service
- PostgreSQL
- Chroma
- optional queue/cache service
- optional web dashboard

Compute should remain stateless. Durable state must live in managed stores.

## 20. Build order

### Track A: platform

- tenancy and auth
- token service
- migration and RLS foundation
- audit plumbing

### Track B: ingestion

- session contracts
- event ingestion
- deduplication
- episode segmentation

### Track C: PCKC memory

- typed claim derivation
- belief gate
- proof attachment
- contradiction and supersession handling

### Track D: retrieval and graph

- lexical + semantic hybrid retrieval
- typed partitions
- repository and session graph build
- hierarchical communities
- context builder and compiler

### Track E: product surfaces

- MCP ergonomics
- dashboard/report surfaces
- operational tooling

## 21. Acceptance criteria

The architecture is acceptable when:

- all reads and writes are tenant-isolated
- token-scoped clients can ingest and retrieve only allowed project/team data
- durable memory is claim- and proof-centric rather than summary-centric
- belief-gated truth filtering works
- retrieval combines lexical, semantic, and graph signals
- repository questions can be answered from repository graph evidence
- context packs are compact, provenance-aware, and budget-disciplined
- contradictions and supersession are surfaced correctly
- the same normalized APIs serve Claude, Codex, and Gemini clients
