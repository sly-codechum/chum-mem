# `.codex` workspace

This directory is the canonical home for Codex-specific project guidance in this repository.

Use these files first:

1. `.codex/AGENTS.md`
2. `docs/INSTRUCTION.md`
3. `docs/ARCHITECTURE_SPEC.md`
4. relevant `.codex/skills/*/SKILL.md`
5. relevant `.codex/prompts/*.md`

## Layout

```text
.codex/
  AGENTS.md
  agents/
  prompts/
  skills/
```

## Agent split

- `agents/architect.toml`
- `agents/backend.toml`
- `agents/retrieval.toml`
- `agents/frontend.toml`
- `agents/security-qa.toml`

## Startup prompt

- `prompts/codex-chat-memory-lifecycle.md` for automatic MCP memory lifecycle on every new chat.
- task gate: always use `chum-memory` first (`mem_search` -> filter IDs -> `memory_get_batch` -> optional `context_build`).

## Skill split

- `skills/product-architect/`
- `skills/backend-platform/`
- `skills/retrieval-pipeline/`
- `skills/frontend-dashboard/`
- `skills/security-qa/`

All Codex-oriented updates should be made in `.codex/`.
