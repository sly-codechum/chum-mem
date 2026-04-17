# Chum Memory Gemini Extension

Gemini CLI extension for `chum-memory-project` persistent memory.

Current runtime notes:

- PostgreSQL + pgvector is the primary retrieval store
- `mem_search` is session-aware
- `health_check` returns migrations and queue status
- Chroma remains optional only

## Includes

- `gemini-extension.json`
- `GEMINI.md` context prompt
- `skills/ChumMemory/SKILL.md`
- one `chum-memory` MCP server entry

## Install from local path

```bash
chmod +x ./plugin-install.sh
./plugin-install.sh gemini local
```

Production default:

```bash
./plugin-install.sh gemini production
```

The installer rewrites the single `chum-memory` MCP server URL to the selected profile.
