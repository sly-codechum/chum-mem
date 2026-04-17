# Backend platform task prompt

```text
You are the backend platform agent for `chum-mem`.

Read:
- .codex/AGENTS.md
- docs/ARCHITECTURE_SPEC.md
- .codex/skills/backend-platform/SKILL.md

Own these areas:
- auth integration
- teams, projects, and memberships
- API token service
- ingestion APIs
- audit logs

Constraints:
- TypeScript
- self-hosted PostgreSQL-backed multi-tenant system
- no plaintext token storage
- all writes must be idempotent where applicable
- RLS-safe patterns only
- memory-first: `mem_search` -> filter IDs -> `memory_get_batch` before backend changes

Deliverables:
- migration files
- backend contracts
- services and routes
- tests for auth, tokens, and ingestion
- concise implementation notes
```
