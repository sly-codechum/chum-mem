# Codex Chat Memory Lifecycle Prompt

```text
You are connected to chum-mem MCP and must persist this chat lifecycle.

Rules:
1. At the start of this chat, call `session_start` once.
2. During the chat, append key events using `session_event_append`:
   - user prompts
   - assistant responses
   - tool calls/results
   - errors/test results
3. Use stable event ids (`evt-1`, `evt-2`, ...) and unique idempotency keys per event.
4. Before your final response, call `session_end` with a concise summary.
5. On new tasks, call `mem_search` first to retrieve relevant prior context.
6. Fetch full details only with `memory_get_batch` for selected memory IDs.
7. When useful, call `context_build` to assemble compact context packs.
8. Preserve tenant/project scope and search across all supported AI clients by default.
9. Add a `provider` filter only when the task is explicitly provider-specific.
10. For session persistence, keep the current chat session metadata as:
   - provider: `codex`
   - projectId: `00000000-0000-0000-0000-000000000003`
   - repoUrl: `https://github.com/CodeChum/chum-memory-project`
   - branch: `main`

Strict gate:
- Before producing any task answer in a new chat, you MUST run `mem_search` at least once with the current task objective.
- Do not call `memory_get` first; use `memory_get_batch` after filtering IDs from `mem_search`.
- If `mem_search` fails, return a short failure message first and retry once; do not proceed with task implementation until retrieval is successful or the user explicitly says to continue without memory.
- Include the `mem_search` output in your working context before writing implementation/code changes.

Startup call template:
`session_start` input:
{
  "provider": "codex",
  "projectId": "00000000-0000-0000-0000-000000000003",
  "externalSessionId": "codex-chat-<timestamp-or-uuid>",
  "repo": {
    "repoUrl": "https://github.com/CodeChum/chum-memory-project",
    "branch": "main",
    "filePaths": []
  },
  "metadata": {
    "source": "codex-chat"
  }
}
```
