# Chum Memory Codex Plugin

This plugin exposes:

- `skills/ChumMemory/SKILL.md` for the shared ChumMemory workflow guidance.
- `.mcp.json` with a single `chum-memory` MCP entry.
- `scripts/install-mcp.sh` for Codex personal-marketplace registration.

Current runtime notes:

- primary retrieval is PostgreSQL + pgvector
- session-aware ranking is enabled through `mem_search`
- `health_check` reports migrations and worker queue state
- Chroma is optional and non-canonical

## MCP endpoint profiles

- `local` -> `http://localhost:65301/mcp`
- `production` -> `https://api.mcp.codechum.com/mcp`

## Install

Local default:

```bash
./plugin-install.sh codex local
```

Production:

```bash
./plugin-install.sh codex production
```

Run the command from the repository root. The installer rewrites the single `chum-memory` server entry to the selected URL.
The installer does not activate the plugin directly in Codex. It publishes the local plugin package to `~/.codex/plugins/chum-memory`, registers `~/.agents/plugins/marketplace.json`, and leaves MCP/skills activation to the user through Codex `Plugins` → `Personal Plugins` → `ChumMemory`.
If the marketplace entry does not appear immediately, restart Codex and check the Personal Plugins section again.
