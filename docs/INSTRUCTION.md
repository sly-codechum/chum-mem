# chum-mem build instruction

Build `chum-mem` as a cloud-native persistent memory platform for coding agents. The product should capture session activity, compress it into durable memory, make it searchable, and inject the most relevant context into future sessions.

## Scope

Support these clients as first-class providers:

- Claude
- Codex
- Gemini

Run `chum-mem` as a multi-tenant cloud service backed by Supabase PostgreSQL and pgvector. Support organizations, teams, team members, projects, and user-generated API tokens for machine-to-cloud authentication.
Run `chum-mem` as a self-hosted memory MCP server backed by PostgreSQL and pgvector. Support organizations, teams, team members, projects, and user-generated API tokens for machine-to-server authentication.

## Core loop

1. Capture session events, tool calls, prompts, outputs, files touched, git metadata, and optional end-of-session summaries.
2. Compress raw events into structured memory units such as facts, decisions, summaries, bugs, tasks, and code changes.
3. Store raw and derived records in PostgreSQL with vector embeddings and metadata indexes.
4. Retrieve memory using hybrid search with semantic, lexical, and metadata filters.
5. Inject compact context packs into future sessions through provider adapters and API endpoints.

## Identity and access

Use server-managed users and API tokens for now. Treat browser and SSO auth as a later phase.

Data model:

- user
- organization
- team
- team_member
- project
- api_token

Rules:

- a user can belong to multiple teams
- each team owns projects and memory
- each member can generate multiple API tokens
- tokens are scoped to team and optionally project
- only hashed token values are stored
- tokens are shown once at creation time

## Required APIs

Expose these capabilities through MCP tools and, if needed, thin HTTP wrappers:

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

## Non-functional requirements

- strict tenant isolation with PostgreSQL RLS and server-side scope resolution
- low-latency hybrid retrieval
- idempotent ingestion
- audit logs for auth, token use, and memory access
- observable services with structured logging and tracing
- provider adapter system that is easy to extend
- Docker-first local and production deployment
- MCP transport support for remote and local agent clients

## Delivery phases

### Phase 1
- users, teams, memberships
- token generation and revocation
- session ingestion
- memory persistence
- lexical search
- MCP server bootstrap
- Dockerized PostgreSQL and service runtime

### Phase 2
- embeddings via pgvector
- hybrid retrieval
- context pack builder
- Claude, Codex, Gemini adapters

### Phase 3
- dashboard
- timeline viewer
- relationship graph
- audit UX
- quotas and billing hooks
