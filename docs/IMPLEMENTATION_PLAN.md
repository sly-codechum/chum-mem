# chum-mem implementation plan

## 1. Architecture summary

`chum-mem` is a cloud-native, multi-tenant memory platform for coding agents. AI clients such as Claude, Codex, Gemini, and Cursor send provider-native activity through a normalized adapter layer into one trusted MCP server surface. The server resolves actor, organization, team, and optional project scope on the server, persists canonical session data plus raw provider payloads in PostgreSQL, and emits audit records for sensitive operations.

Raw session events are durable first-class records. Background workers should compact those events into session episodes, then extract structured memories, attach provenance back to source events, compute embeddings, and link related memories and sessions. Retrieval should combine PostgreSQL full-text search, pgvector similarity search, and bounded graph expansion, then apply session-aware reranking before building compact context packs for future sessions.

The security model has two layers:

- database-enforced tenant isolation via PostgreSQL RLS and application session settings
- server-side validation for machine API tokens, with team and optional project scope derived from the token record and never from caller input

## 2. Initial monorepo structure

```text
apps/
  api/                 MCP server, health endpoints, token/ingest/search/context tools
  web/                 future optional admin console
services/
  worker/              derivation, embedding, linking, replay workers
packages/
  auth/                token hashing, auth context, scope resolution
  contracts/           zod contracts and shared domain types
  db/                  SQL-facing schema metadata and repository boundaries
  provider-adapters/   canonical adapter interface and provider registry
  retrieval/           search ranking and context-pack assembly logic
infra/
  migrations/          explicit SQL migrations with RLS and indexes
  docker/              Docker packaging and local stack bootstrap
docs/
  IMPLEMENTATION_PLAN.md
  API_CONTRACTS.md
```

## 3. Initial database schema and migration plan

### Migration 0001 goals

- enable `pgcrypto` and `vector`
- create `app_users` for server-managed identities
- create core tenant tables: `organizations`, `teams`, `team_members`, `projects`
- create auth/token tables: `api_tokens`
- create ingestion tables: `sessions`, `session_events`
- create retrieval tables: `memories`, `memory_provenance`, `embeddings`, `memory_edges`, `context_requests`
- create observability tables: `audit_logs`
- add indexes for:
  - membership resolution
  - token lookup by prefix
  - session idempotency and provider session uniqueness
  - event idempotency
  - memory full-text search
  - vector retrieval
  - common tenant and project filters
- enable RLS on all tenant-owned tables
- define helper functions around `current_setting('app.*')`
- add policies for transaction-scoped tenant access

### Initial schema assumptions

- every tenant-owned row includes `organization_id` and `team_id`
- project-owned rows also include `project_id`
- `api_tokens` are hashed at rest using server-side crypto and only their `token_prefix` remains queryable
- memory provenance uses a join table because one memory can derive from multiple events
- initial embedding dimension is `1536`
  this assumes a single default embedding model in phase 1 and can be expanded later with model-specific tables or migration strategy if dimensions diverge
- the initial runtime is machine-first MCP, not browser-first SaaS

### Follow-on migrations

1. `0002_http_and_admin_auth.sql`
   add optional browser auth or OAuth administration later
2. `0003_episode_and_session_graph.sql`
   add `session_episodes`, `session_edges`, and retrieval feature storage for session-aware ranking
3. `0004_queue_and_replay.sql`
   add durable worker job state, poison queue support, and replay bookkeeping
4. `0005_billing_and_retention.sql`
   add quotas, retention windows, archival, and billing hooks

## 4. API contract plan

See [API_CONTRACTS.md](./docs/API_CONTRACTS.md) for payload shapes. The initial tool and transport surface is:

- MCP tools:
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
- HTTP transport endpoints:
  - `POST /mcp`
  - `GET /mcp`
  - `DELETE /mcp`
  - `GET /health`

Contract rules:

- use Zod at package boundaries
- keep canonical provider event types stable across adapters
- accept provider-specific payloads only in `raw_payload` and adapter metadata
- require event idempotency keys on session event writes
- require provenance handles in memory and context responses

## 5. Rollout order

1. workspace scaffold and shared configs
2. contracts and provider adapter interfaces
3. initial SQL migration with RLS
4. auth and token primitives
5. MCP server bootstrap and transport wiring
6. episode compaction and retrieval feature extraction
7. retrieval ranking and context-pack assembly with session-aware scoring
8. worker and replay scaffolds
9. Docker packaging and deployment config
10. tests for contracts, token hashing, scope resolution, ingestion idempotency, retrieval filtering, session-aware ranking, and context packing
