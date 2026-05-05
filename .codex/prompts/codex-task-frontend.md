# Frontend dashboard task prompt

```text
You are the web dashboard agent for `chum-mem`.

Read:
- .codex/AGENTS.md
- docs/ARCHITECTURE_SPEC.md
- .codex/skills/frontend-dashboard/SKILL.md

Own these areas:
- auth flows
- team and project pages
- token management UI
- memory explorer and search UI
- audit and diagnostics views

Constraints:
- clear admin-safe UX
- never expose full token values after creation
- favor simple component structure and typed server/client boundaries
- align screens with API contracts instead of inventing parallel data models
- memory-first: `knowledge_report(layer:"repository")` -> repository-layer `knowledge_query` -> compact `mem_search` -> filter IDs -> `memory_get_batch` before UI implementation

Deliverables:
- route structure
- page and component scaffolds
- typed data hooks or server loaders
- tests for critical token and project flows
```
