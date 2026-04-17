# Codex initiation prompt

```text
Build `chum-mem` as a cloud-native persistent memory platform for coding agents.

Memory-first rule for this workspace:
- run `mem_search` first for the current objective
- fetch full detail only via `memory_get_batch` for selected IDs
- use `context_build` only when needed
- continue without memory only if user explicitly requests it

Read these files first and treat them as the source of truth:
- .codex/AGENTS.md
- docs/INSTRUCTION.md
- docs/ARCHITECTURE_SPEC.md
- all relevant skills in .codex/skills

Core product constraints:
- support Claude, Codex, and Gemini through a normalized provider adapter layer
- use self-hosted PostgreSQL as the primary backend
- use pgvector for semantic retrieval and Postgres full-text search for lexical retrieval
- support organizations, teams, team members, projects, and per-user API tokens
- tokens must be hashed at rest and shown only once when created
- enforce strict multi-tenant isolation with RLS and server-side auth checks

Desired repo shape:
- apps/web
- apps/api
- services/worker
- packages/contracts
- packages/provider-adapters
- packages/db
- packages/retrieval
- packages/auth
- infra/migrations
- docs

Execution rules:
- produce an implementation plan before coding
- work in small, reviewable steps
- define contracts and schema before feature code
- add tests for all critical auth, token, ingestion, and retrieval logic
- keep provider-specific code isolated behind adapters
- preserve provenance from memory back to raw session events

Start by:
1. summarizing the architecture in your own words
2. proposing the monorepo structure
3. defining the initial database schema and migration plan
4. defining the API contracts for auth, tokens, ingestion, search, and context building
5. then scaffold the repository
```
