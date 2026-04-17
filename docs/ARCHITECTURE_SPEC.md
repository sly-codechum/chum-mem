# chum-mem architecture spec

## 1. Objective

`chum-mem` is a multi-tenant memory platform for coding agents. It accepts session activity from Claude, Codex, and Gemini clients, derives reusable memory from that activity, stores memory in self-hosted PostgreSQL, and returns context packs for future work through MCP tools.

The system must optimize for:

- tenant safety
- predictable retrieval quality
- low operational complexity
- Docker-native deployment
- parallel agent workflows in Codex
- resistance to context rot across long-running and multi-session work

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
- object storage for large artifacts
- optional Redis for queues and caching
- embedding and summarization models

## 3. High-level architecture

```text
Claude/Codex/Gemini clients
        |
        v
Provider Adapter Layer
        |
        v
MCP Server / App Server
        |
        +--> Token AuthN/AuthZ
        +--> Ingestion Service
        +--> Query Understanding
        +--> Candidate Retrieval
        +--> Reranker
        +--> Context Builder
        +--> Token Service
        +--> Team/Project Service
        |
        v
PostgreSQL + pgvector
        |
        +--> raw session tables
        +--> episodic memory tables
        +--> semantic memory tables
        +--> normalized memory tables
        +--> retrieval feature tables
        +--> session and memory graph tables
        +--> team/project/user tables
        +--> audit tables
        |
        v
Background Workers
        +--> event normalization
        +--> episode compaction
        +--> memory extraction
        +--> embeddings
        +--> graph linking
        +--> supersession / invalidation
        +--> reindexing
```

## 4. Logical components

### 4.1 MCP server

Responsibilities:

- expose MCP tools over Streamable HTTP and stdio
- validate inputs with shared contracts
- authenticate API tokens
- resolve tenant and project scope on the server
- call ingestion, retrieval, and context services
- expose health and diagnostics endpoints for operations

Recommended stack:

- TypeScript
- MCP TypeScript SDK
- Express or Fastify
- OpenTelemetry for tracing

Compatibility note:

- Prefer `stdio` transport for maximum client compatibility across Codex, Claude, and Gemini.
- Keep Streamable HTTP enabled at `http://127.0.0.1:65301/mcp` for clients that support it.

### 4.2 Optional admin surfaces

Responsibilities:

- future browser admin and audit UI
- operator workflows for teams, users, projects, and tokens

Recommended stack:

- TypeScript
- Next.js or a thin admin console

### 4.3 Background workers

Responsibilities:

- normalize provider events into stable spans
- compact sessions into episode summaries
- extract structured memories with provenance
- create embeddings in PostgreSQL-backed indexes
- compute memory relationships and session relationships
- mark stale or superseded memories
- repair and replay failed jobs

Recommended execution model:

- queue-backed job runners
- idempotent jobs keyed by session event or memory ID
- retry with poison queue and manual replay tools
- no polling-based semantic sync loop on the primary retrieval path

### 4.4 Storage layer

Primary store:

- self-hosted PostgreSQL

Capabilities:

- row-level security
- JSONB metadata storage
- full-text search
- pgvector for semantic retrieval
- transactional integrity for ingestion

Optional secondary store:

- object storage for attachments, large transcripts, and generated artifacts

### 4.5 Context engineering model

`chum-mem` should behave like a memory operating system, not a raw transcript store.

The architecture must separate:

- hot context: the task-local context pack sent to the active model
- warm episodic memory: recent session- and branch-linked episodes
- cold semantic memory: durable facts, decisions, bugs, tasks, and implementation notes
- provenance graph: the event-level evidence that explains why any item was retrieved

This separation is the primary defense against context rot. Instead of stuffing more text into prompts, the system should continuously compress, rank, invalidate, and re-expand only the evidence needed for the current objective.

## 5. Multi-tenant model

### 5.1 Entities

- `app_user`
- `organization`
- `team`
- `team_member`
- `project`
- `api_token`
- `provider_connection`
- `session`
- `session_event`
- `memory`
- `embedding`
- `memory_edge`
- `context_request`
- `audit_log`

### 5.2 Isolation model

All tenant-owned records must include `organization_id` and `team_id`. Project-owned records must also include `project_id`.

Isolation rules:

- interactive users access data through authenticated membership
- machine tokens resolve team and user from the token record, never from caller-supplied identifiers
- team admins can manage tokens and projects for their team
- project-scoped tokens cannot access out-of-scope projects

### 5.3 RLS strategy

Use PostgreSQL RLS on all tenant tables. Policies should check application session settings set by the server on each transaction:

- current `organization_id` and `team_id` match the row
- optional project scoping is satisfied
- write access is limited to the resolved server actor and token scope
- revoked or expired tokens are rejected before the transaction begins

## 6. Provider adapter architecture

Create one shared interface for all providers:

```ts
interface ProviderAdapter {
  provider: 'claude' | 'codex' | 'gemini';
  startSession(input: StartSessionInput): Promise<StartSessionResult>;
  appendEvent(input: SessionEventInput): Promise<void>;
  endSession(input: EndSessionInput): Promise<EndSessionResult>;
  searchMemory(input: SearchInput): Promise<SearchResult>;
  buildContextPack(input: ContextRequestInput): Promise<ContextPack>;
}
```

Normalization rules:

- convert provider-native events into a canonical schema
- preserve original payload in `raw_payload`
- attach provider-specific metadata without polluting the canonical contract

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

## 7. Ingestion pipeline

### 7.1 Start session

Client sends:

- provider
- project identifier
- external session identifier
- repo metadata
- branch metadata
- user agent and local metadata

Server actions:

- authenticate caller
- resolve team, project, and actor
- create or upsert session
- emit audit log

### 7.2 Append event

Client sends normalized or provider-native events.

Server actions:

- validate payload
- deduplicate on event idempotency key
- persist raw event
- extract stable retrieval metadata immediately when cheap:
  - file paths
  - symbols
  - command/tool names
  - branch / commit / repo context
  - error signatures
- enqueue derivation jobs only after event classification

### 7.3 End session

Server actions:

- mark session complete
- derive an episode summary and outcome summary
- trigger final compaction pipeline
- emit audit log

### 7.4 Two-stage derivation

The default derivation path should be:

1. event to episode
   - segment the session into coherent work episodes such as debugging, implementation, refactor, or investigation
   - preserve the ordered event span and the source `session_id`
2. episode to memory
   - extract atomic memories from each episode
   - assign a type, title, summary, confidence, importance, freshness window, and provenance handles
   - link memory to both `session_id` and `episode_id`

This avoids the current failure mode where a single end-of-session summary or failure blob becomes the dominant retrieval artifact.

## 8. Memory derivation model

### 8.1 Raw to memory transformation

Transform session events into reusable units:

- fact
- decision
- task
- bug
- summary
- implementation detail
- change log
- risk

Each memory record should include:

- concise title
- normalized content
- short summary
- provenance links to session events
- primary source `session_id`
- source `episode_id`
- importance score
- confidence score
- freshness state
- supersession state
- searchable metadata

### 8.2 Memory layers

Use three explicit layers:

- `episode_memory`
  - session-local findings, command traces, error clusters, local implementation steps
- `semantic_memory`
  - durable facts, decisions, tasks, bugs, risks, implementation details
- `reflection_memory`
  - higher-order summaries over multiple related sessions or episodes

The retrieval system should search all three layers but rank them differently depending on the query intent.

### 8.3 Linking

Use `memory_edges` to encode relations:

- duplicates
- supersedes
- caused_by
- depends_on
- related_to
- from_same_session
- from_same_episode
- contradicts
- confirms

### 8.4 Context-rot controls

To prevent stale or low-value memories from dominating retrieval, every memory must support:

- temporal decay that is type-aware
  - decisions decay slowly
  - active task state decays quickly
  - command transcripts decay fastest
- write-triggered invalidation
  - a new decision, file change, or successful fix can supersede older memories
- contradiction handling
  - contradictory memories stay queryable but cannot both receive top rank without explicit uncertainty markers
- summary refresh
  - long-running projects periodically regenerate reflection memories from current child memories
- provenance-first debugging
  - every ranked item must explain which session events justified the rank

## 9. Search and retrieval

### 9.1 Search modes

- lexical search via full-text indexes
- semantic search via pgvector
- graph and session-neighborhood retrieval
- hybrid reranked search
- metadata filtering

Primary recommendation:

- keep PostgreSQL plus pgvector as the source of truth for primary semantic retrieval
- do not depend on an eventually consistent Chroma mirror for core ranking
- if Chroma remains, treat it as an optional accelerator or experiment, not the canonical search path

### 9.2 Filters

- organization
- team
- project
- user
- provider
- repo
- branch
- session IDs
- memory type
- time range
- tags

### 9.3 Candidate generation

Generate candidates from four channels in parallel:

1. lexical
   - `tsvector` over title, summary, content, symbols, errors, and file paths
2. semantic
   - embedding search over summaries and normalized content in `public.embeddings`
3. session graph
   - exact `session_id` matches, adjacent sessions in the same branch, and sessions linked by shared files or repeated error signatures
4. memory graph
   - `memory_edges` expansion with small bounded hops

No single channel should dominate candidate generation. This is required to avoid the standard top-k chunk failure mode.

### 9.4 Ranking model

Ranking should be feature-based and session-aware. Each candidate should expose:

- lexical score
- semantic score
- session relevance score
- graph proximity score
- branch / repo / file overlap score
- recency score
- importance score
- confidence score
- freshness penalty
- superseded penalty

Recommended ranking formula for the first implementation:

```text
final_score =
  0.22 * lexical +
  0.22 * semantic +
  0.18 * session_relevance +
  0.12 * graph_proximity +
  0.08 * repo_branch_file_overlap +
  0.08 * recency +
  0.06 * importance +
  0.04 * confidence -
  0.10 * freshness_penalty -
  0.10 * superseded_penalty
```

This formula is intentionally session-aware. `session_relevance` is not optional metadata; it is a first-class retrieval signal.

### 9.5 Retrieval tiers

#### Tier 1: index results
Return compact ranked hits:

- id
- title
- type
- score
- matched `session_id`
- timestamp
- summary snippet

#### Tier 2: context neighborhood
Return nearby or related memory:

- same session
- same episode
- related edges

#### Tier 3: full details
Return full memory detail only for selected IDs:

- complete memory content
- provenance details
- related memory IDs

### 9.6 Mandatory retrieval workflow

To prevent context bloat and latency regressions, all clients should follow:

1. `mem_search` first, with compact output (`disclosureLevel=overview`, small `limit`).
2. `memory_get_batch` only for filtered IDs.
3. `context_build` only after filtering.

This is the default contract for Codex, Claude, and Gemini integrations.

### 9.7 Concise prompt contract

Recommended startup prompt for any client:

```text
Memory-first for this task.
Run mem_search (hybrid, overview, limit 5) for: "<task>".
Pick relevant IDs, run memory_get_batch only for those IDs, then solve "<task>".
If no relevant memory, say so briefly and continue.
```

## 10. Context pack builder

Purpose:

Build a token-efficient context payload for a future provider session or MCP-assisted task.

Inputs:

- team
- project
- provider
- objective or task description
- optional repo, branch, or file paths
- max token budget

Algorithm:

1. parse objective into retrieval intents
2. classify the query:
   - factual lookup
   - continuation of prior session
   - debugging
   - implementation
   - planning
3. run session-aware hybrid retrieval
4. deduplicate by provenance and supersession, not only by memory id
5. enforce coverage:
   - at least one high-confidence item
   - at least one recent item when applicable
   - at least one same-session or same-branch item when continuing prior work
6. compress selected evidence into compact sections:
   - project facts
   - recent decisions
   - active bugs/tasks
   - relevant implementation details
   - citations/provenance handles
7. include why-selected metadata for debugging and offline evaluation

Packing rules:

- never include raw transcript blobs when an episode or semantic memory exists
- prefer semantically distinct evidence over near-duplicate top-k hits
- prefer unsuperseded memories unless the query explicitly asks for history
- reserve part of the token budget for high-value provenance excerpts, not only summaries
- emit the top matched `session_id` values so clients can continue the right thread

Output contract:

```json
{
  "context_pack": {
    "project_facts": [],
    "recent_decisions": [],
    "active_tasks": [],
    "known_bugs": [],
    "implementation_notes": [],
    "sources": []
  }
}
```

## 11. Authentication and authorization

### 11.1 Initial auth model

Use:

- server-managed `app_users`
- API tokens for machine access
- optional bootstrap admin user created out of band

Future phases may add OAuth or an admin UI, but the initial deployment target is machine-to-server MCP access.

### 11.2 Machine auth

API token format:

- prefix: `cmem_live_`
- random secret generated server-side
- store only `token_hash`
- show plaintext once

Token attributes:

- team scope
- optional project scope
- scopes array
- creator user ID
- last used timestamp
- expiration
- revocation timestamp

### 11.3 Scope model

Example scopes:

- `ingest`
- `search`
- `context:read`
- `project:write`
- `team:admin`

## 12. Database schema outline

### `organizations`
- `id`
- `name`
- `slug`
- `created_at`

### `teams`
- `id`
- `organization_id`
- `name`
- `slug`
- `created_at`

### `team_members`
- `id`
- `organization_id`
- `team_id`
- `user_id`
- `role`
- `status`
- `created_at`

### `projects`
- `id`
- `organization_id`
- `team_id`
- `name`
- `slug`
- `repo_url`
- `default_branch`
- `created_at`

### `api_tokens`
- `id`
- `organization_id`
- `team_id`
- `project_id nullable`
- `user_id`
- `name`
- `token_prefix`
- `token_hash`
- `scopes jsonb`
- `last_used_at nullable`
- `expires_at nullable`
- `revoked_at nullable`
- `created_at`

### `sessions`
- `id`
- `organization_id`
- `team_id`
- `project_id`
- `user_id nullable`
- `provider`
- `external_session_id`
- `repo_url nullable`
- `branch nullable`
- `status`
- `started_at`
- `ended_at nullable`

### `session_events`
- `id`
- `organization_id`
- `team_id`
- `project_id`
- `session_id`
- `provider`
- `event_type`
- `event_time`
- `idempotency_key`
- `payload jsonb`
- `raw_payload jsonb`
- `created_at`

### `session_episodes`
- `id`
- `organization_id`
- `team_id`
- `project_id`
- `session_id`
- `episode_type`
- `title`
- `summary`
- `started_at`
- `ended_at`
- `metadata jsonb`
- `created_at`

### `memories`
- `id`
- `organization_id`
- `team_id`
- `project_id`
- `session_id nullable`
- `episode_id nullable`
- `type`
- `title`
- `content`
- `summary`
- `importance_score`
- `confidence_score`
- `freshness_score`
- `metadata jsonb`
- `created_by`
- `created_at`
- `superseded_at nullable`

### `embeddings`
- `id`
- `organization_id`
- `team_id`
- `project_id`
- `memory_id`
- `model`
- `embedding vector(...)`
- `created_at`

### `memory_edges`
- `id`
- `organization_id`
- `team_id`
- `project_id`
- `from_memory_id`
- `to_memory_id`
- `edge_type`
- `weight`
- `created_at`

### `session_edges`
- `id`
- `organization_id`
- `team_id`
- `project_id`
- `from_session_id`
- `to_session_id`
- `edge_type`
- `weight`
- `created_at`

### `retrieval_features`
- `id`
- `organization_id`
- `team_id`
- `project_id`
- `memory_id`
- `feature_name`
- `feature_value`
- `feature_group`
- `updated_at`

### `context_requests`
- `id`
- `organization_id`
- `team_id`
- `project_id`
- `requester_user_id nullable`
- `requester_token_id nullable`
- `provider`
- `objective`
- `token_budget`
- `response_summary`
- `created_at`

### `audit_logs`
- `id`
- `organization_id nullable`
- `team_id nullable`
- `project_id nullable`
- `actor_type`
- `actor_id`
- `action`
- `target_type`
- `target_id`
- `metadata jsonb`
- `created_at`

## 13. API design

### Ingestion

- `POST /v1/ingest/session/start`
- `POST /v1/ingest/session/event`
- `POST /v1/ingest/session/end`

### Retrieval

- `POST /v1/memory/search`
- `GET /v1/memory/:id`
- `POST /v1/context/build`

### MCP tool surface

- `session_start`
- `session_event_append`
- `session_end`
- `memory_search`
- `memory_get`
- `context_build`
- `projects_list`
- `token_create`
- `token_revoke`
- `teams_me`
- `audit_list`

### Operational HTTP endpoints

- `POST /mcp`
- `GET /mcp`
- `DELETE /mcp`
- `GET /health`

## 14. Research basis

The retrieval and context-engineering changes in this spec are supported by the paper notes in [CONTEXT_ENGINEERING_RESEARCH.md](./docs/CONTEXT_ENGINEERING_RESEARCH.md).

## 15. Reliability and idempotency

- require idempotency key on session event ingestion
- use exactly-once semantics where feasible, otherwise at-least-once with deduplication
- wrap session close and summary creation with transactional guards
- worker jobs must be restart-safe

## 16. Observability

Implement:

- structured application logs
- request IDs and correlation IDs
- distributed tracing
- metrics for ingestion volume, queue depth, retrieval latency, context pack build time, token validation failures
- audit records for sensitive operations

## 17. Security requirements

- hash API tokens with a slow password hash or keyed hash strategy
- rotate signing and secret material through managed secrets
- validate webhook/provider payloads where applicable
- encrypt sensitive configuration at rest
- minimize raw sensitive prompt retention where possible
- define retention policy per team or plan

## 18. Deployment model

### Recommended environments

- local development
- preview/staging
- production

### Deployment units

- web app
- MCP server
- worker service
- Redis optional
- PostgreSQL

### Cloud-first guidance

Keep compute stateless. Persist durable state only in managed stores. All background work should be resumable in a new runtime.

## 19. Build order

### Track A: platform
- project bootstrap
- server-managed users
- teams and projects
- RLS and migration pipeline
- token service
- Docker packaging

### Track B: ingestion
- session contracts
- ingestion endpoints
- raw persistence
- derivation jobs

### Track C: retrieval
- full-text indexes
- embeddings pipeline
- hybrid search
- context builder

### Track D: product
- MCP tool ergonomics
- Docker operations
- optional dashboard
- audit UX

## 20. Acceptance criteria

The architecture is acceptable when:

- all reads and writes are tenant-isolated
- token-scoped clients can ingest and retrieve only allowed project/team data
- memory search returns useful results from lexical and semantic signals
- context packs are compact and provenance-aware
- Claude, Codex, and Gemini can use the same normalized APIs
