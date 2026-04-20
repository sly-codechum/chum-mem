# Chum Memory Codex Plugin

This plugin exposes:

- `skills/chum-memory/SKILL.md` for the ChumMemory workflow guidance (PCKC v2.2.3).
- `.mcp.json` with a single `chum-memory` MCP entry.
- `scripts/install-mcp.sh` for Codex personal-marketplace registration.

Current runtime notes:

- Three-way hybrid search: PostgreSQL FTS + pgvector ANN + Chroma ML
- Session-aware ranking via `mem_search` with typed partitions
- Deterministic governance: pin/archive/reject claims via `claim_govern`
- Session-start knowledge report injected on `SessionStart` hook
- `health_check` reports migrations and worker queue state

## MCP endpoint profiles

- `local` -> `http://localhost:63001/mcp`
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

Run the command from the repository root. The installer copies the plugin to `~/.codex/plugins/chum-memory-codex` and registers it in `~/.agents/plugins/marketplace.json`.

The installer does **not** activate the plugin directly in Codex. After running the script:

1. Open Codex
2. Go to **Plugins → Personal Plugins**
3. Install **ChumMemory**

Codex handles MCP activation, hooks, and script paths from the plugin manifest at install time.
