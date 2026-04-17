# MCP Compatibility (Codex, Claude, Gemini)

`chum-mem` now supports two MCP access patterns:

1. `stdio` bridge (recommended for maximum cross-client compatibility)
2. Streamable HTTP (`http://127.0.0.1:65301/mcp`)

## Why `stdio` first

Different MCP clients implement Streamable HTTP details slightly differently (SSE framing, response decoding, accept headers). `stdio` avoids those transport differences and is the most reliable path across Codex, Claude, and Gemini clients.

## Start services

```bash
docker compose up -d --build
```

Ensure API is reachable:

```bash
curl -s http://127.0.0.1:65301/health
```

## Run stdio MCP server

```bash
pnpm -C apps/api mcp:stdio
```

This process proxies MCP tool calls to the local API (`CHUM_MEM_API_BASE_URL`, default `http://127.0.0.1:65301`).

## Codex config (`~/.codex/config.toml`)

```toml
[mcp_servers.chum-memory]
enabled = true
command = "pnpm"
args = ["-C", "/ABS/PATH/chum-memory-project/apps/api", "mcp:stdio"]
```

## Claude Code config

Example:

```bash
claude mcp add chum-memory -- pnpm -C /ABS/PATH/chum-memory-project/apps/api mcp:stdio
```

## Gemini settings (`~/.gemini/settings.json`)

Example:

```json
{
  "mcpServers": {
    "chum-memory": {
      "command": "pnpm",
      "args": [
        "-C",
        "/ABS/PATH/chum-memory-project/apps/api",
        "mcp:stdio"
      ]
    }
  }
}
```

## Optional Streamable HTTP mode

If your client supports Streamable HTTP cleanly, use:

```text
http://127.0.0.1:65301/mcp
```

## Tools exposed (same in stdio + HTTP)

- `health_check`
- `session_start`
- `session_event_append`
- `session_end`
- `mem_search`
- `context_build`
- `graph_snapshot`
- `memory_get`
- `memory_get_batch`

## References

- Anthropic Claude Code MCP docs: https://docs.anthropic.com/en/docs/claude-code/mcp
- Gemini Code Assist agent mode (MCP servers in Gemini settings JSON): https://developers.google.com/gemini-code-assist/docs/use-agentic-chat-pair-programmer
- Gemini Code Assist release notes (MCP in agent mode): https://cloud.google.com/gemini/docs/codeassist/release-notes
