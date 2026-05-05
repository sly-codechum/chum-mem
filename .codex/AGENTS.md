# AGENTS.md

This repository is built with Codex using parallel agents. Treat this file as the authoritative operating guide for any agent working in the repo.

## Product summary

`chum-mem` is a cloud-native persistent memory system for coding agents. It supports Claude, Codex, and Gemini clients and uses self-hosted PostgreSQL plus pgvector for storage and retrieval. The product supports organizations, teams, members, projects, and personal API tokens used by local clients to connect to the memory MCP server.

## Primary goals

- implement a secure multi-tenant backend
- support provider-normalized ingestion and retrieval
- provide an MCP-first memory service with optional future admin surfaces
- optimize for reproducible agent work and low-friction handoff

## Source of truth

Use these files first:

1. `docs/INSTRUCTION.md`
2. `docs/ARCHITECTURE_SPEC.md`
3. `.codex/AGENTS.md`
4. relevant skill in `.codex/skills/`
5. relevant prompt in `.codex/prompts/`

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

## Default chat startup instruction

For every new Codex chat in this workspace, load and apply:

- `.codex/prompts/codex-chat-memory-lifecycle.md`

This startup prompt defines the required MCP lifecycle:

- `session_start` at chat begin
- `session_event_append` for key events
- `session_end` before final answer
- `knowledge_report(layer:"repository")` before any graph query or memory search
- repository-layer `knowledge_query` before memory search
- `mem_search` only after report + repository query when prior memory may help

Do not skip these unless the user explicitly disables memory ingestion for the chat.

## Task memory gate

For every implementation task, always run this retrieval sequence before coding:

1. `knowledge_report(layer:"repository")` in Markdown form; treat it as primary high-level repository context
2. repository-layer `knowledge_query` for relevant components, architecture, and relationships
3. `mem_search` (`mode=hybrid`, `disclosureLevel=overview`, small `limit`)
4. filter relevant memory IDs
5. `memory_get_batch` for filtered IDs only
6. optional `context_build` if extra compact context is needed

Do not jump straight to full memory dumps.
Do not use Grep/Glob or file-level search before steps 1-3.

## Expected repository layout

```text
apps/
  web/
  api/
services/
  worker/
packages/
  contracts/
  provider-adapters/
  db/
  retrieval/
  auth/
infra/
  migrations/
  seeds/
docs/
.codex/
```

## Branch and task hygiene

- one branch or worktree per major task
- one agent thread per bounded objective
- use small PR-sized diffs
- leave concise progress notes in commit messages or task logs

## Definition of done

A task is done only when all are true:

- code builds
- tests relevant to the change pass
- types pass
- changed behavior is documented when needed
- security and tenant implications were considered

## Recommended agent split

### Agent 1: architecture and contracts
Own:
- API contracts
- database schema
- type systems
- migration planning

### Agent 2: backend platform
Own:
- auth integration
- token service
- ingestion APIs
- RLS-safe data access

### Agent 3: retrieval and memory pipeline
Own:
- summarization jobs
- embeddings
- hybrid search
- context pack builder

### Agent 4: web dashboard
Own:
- auth UX
- team/project pages
- token management UI
- memory explorer

### Agent 5: QA and security
Own:
- threat review
- integration tests
- tenancy tests
- API misuse tests

### Agent 6: Postgres DB engineer
Own:
- schema design and migrations
- query and index tuning (including pgvector HNSW/IVFFlat)
- lock, deadlock, and advisory-lock analysis
- vacuum, WAL, and memory tuning
- RLS policies and tenant isolation at the database layer

## Commands policy

Before running destructive or networked commands, explain intent in the thread. Prefer reproducible scripts over ad hoc shell commands.

## Coding standards

- TypeScript everywhere unless there is a strong reason otherwise
- Zod for external contracts
- SQL migrations must be explicit and reviewable
- isolate side effects behind service boundaries
- log with stable machine-readable fields

## PostgreSQL rules

- all tenant tables must include tenant keys
- RLS policies must be added in the same migration as table creation when possible
- use transaction-scoped application settings for tenant resolution in server-trusted code paths
- prefer database-enforced constraints over app-only checks

## Token rules

- generate high-entropy secrets server-side
- hash before persistence
- show plaintext once only
- record `last_used_at`
- support revoke and expiry

## Retrieval rules

- support lexical and semantic search
- keep provenance links from memory back to session events
- context builder must respect token budgets
- retrieval output must be compact, ranked, and deduplicated

## If uncertain

Do not guess silently. State the assumption, make the smallest safe change, and leave a clear note for follow-up.
