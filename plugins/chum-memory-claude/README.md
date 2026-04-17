# Chum Memory Claude Plugin

This plugin provides Claude Code integration for `chum-memory-project`.

Current runtime notes:

- retrieval is PostgreSQL-first and session-aware
- `health_check` exposes migrations plus queue state
- `session_end` triggers episode-based derivation and queue-backed follow-up work
- Chroma is optional only

## Includes

- `.claude-plugin/plugin.json`
- `.mcp.json` with a single `chum-memory` MCP entry
- `skills/ChumMemory/SKILL.md` with project-specific memory workflow guidance

## Install (marketplace)

From repo root:

```bash
chmod +x ./plugin-install.sh
./plugin-install.sh claude local
```

Production:

```bash
./plugin-install.sh claude production
```

This script adds the Personal Plugins marketplace, installs:

`chum-memory@personal`

and rewrites the single `chum-memory` MCP server URL in Claude config.

## Slash commands

- `/chum-memory` -> primary ChumMemory skill
