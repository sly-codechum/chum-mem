#!/usr/bin/env bash
# hook-dispatch.sh — unified hook wrapper for chum-memory across Claude Code
# and Codex. Reads the hook payload once from stdin, runs session-sync.sh
# with it, then runs sync.sh (repository layer) for turn-boundary events
# only, and finally emits the provider-appropriate control JSON.
#
# Provider is selected by CHUM_PROVIDER (default "claude"). The same scripts
# work for both hosts because the Claude Code and Codex hook payloads share
# the same top-level shape (session_id, hook_event_name, cwd, prompt,
# tool_name/tool_input/tool_response).
#
# Scripts location is resolved from:
#   1. CHUM_SCRIPTS_DIR env var (set by the Codex installer)
#   2. CLAUDE_PLUGIN_ROOT/scripts (set by Claude Code)
#   3. dirname $0 (fallback — works when called with an absolute path)

set -uo pipefail

PROVIDER="${CHUM_PROVIDER:-claude}"

# Resolve scripts directory
if [[ -n "${CHUM_SCRIPTS_DIR:-}" ]]; then
  SCRIPTS_DIR="$CHUM_SCRIPTS_DIR"
elif [[ -n "${CLAUDE_PLUGIN_ROOT:-}" ]]; then
  SCRIPTS_DIR="${CLAUDE_PLUGIN_ROOT}/scripts"
else
  SCRIPTS_DIR="$(cd "$(dirname "$0")" && pwd)"
fi

# Read the full hook payload from stdin once
HOOK_PAYLOAD=$(cat)

# Extract hook event name and (Codex) cwd fallback
HOOK_EVENT=$(echo "$HOOK_PAYLOAD" | jq -r '.hook_event_name // ""' 2>/dev/null || echo "")
PAYLOAD_CWD=$(echo "$HOOK_PAYLOAD" | jq -r '.cwd // ""' 2>/dev/null || echo "")

# Resolve project dir: prefer Claude env var, else payload cwd, else $PWD
if [[ -n "${CLAUDE_PROJECT_DIR:-}" ]]; then
  PROJECT_DIR="$CLAUDE_PROJECT_DIR"
elif [[ -n "$PAYLOAD_CWD" && "$PAYLOAD_CWD" != "null" ]]; then
  PROJECT_DIR="$PAYLOAD_CWD"
else
  PROJECT_DIR="$PWD"
fi
export CLAUDE_PROJECT_DIR="$PROJECT_DIR"
export CHUM_PROVIDER="$PROVIDER"

# ── Fast health gate — bail immediately if API is unreachable ──
API_URL="${CHUM_MEMORY_API_URL:-http://localhost:63001}"
if ! curl -sf --max-time 2 "${API_URL}/health" >/dev/null 2>&1; then
  UNAVAIL_MSG="ChumMemory API unreachable at ${API_URL} — memory features unavailable this turn."
  case "$PROVIDER" in
    codex) printf '{"systemMessage":"%s"}\n' "$UNAVAIL_MSG" ;;
    *)     printf '{"hookSpecificOutput":{"hookEventName":"%s","additionalContext":"%s"}}\n' "$HOOK_EVENT" "$UNAVAIL_MSG" ;;
  esac
  exit 0
fi

# ── Session layer (always runs for every event) ──
SESSION_STDERR=""
if [[ -x "${SCRIPTS_DIR}/session-sync.sh" ]]; then
  SESSION_STDERR=$(printf '%s' "$HOOK_PAYLOAD" | bash "${SCRIPTS_DIR}/session-sync.sh" 2>&1 >/dev/null) || {
    echo "chum-memory session-sync error: ${SESSION_STDERR}" >&2
  }
fi

# ── Repository layer (only on turn-boundary events) ──
case "$HOOK_EVENT" in
  UserPromptSubmit|SessionStart)
    if [[ -x "${SCRIPTS_DIR}/sync.sh" ]]; then
      bash "${SCRIPTS_DIR}/sync.sh" "$PROJECT_DIR" >/dev/null 2>&1 || true
    fi
    ;;
esac

# ── Emit provider-appropriate control JSON ──
emit_claude() {
  local event="$1" message="$2"
  printf '{"hookSpecificOutput":{"hookEventName":"%s","additionalContext":"%s"}}\n' "$event" "$message"
}

emit_codex() {
  # Codex reads `systemMessage` from stdout JSON. Keep it short so it isn't
  # injected verbatim into every turn.
  local message="$1"
  printf '{"systemMessage":"%s"}\n' "$message"
}

USER_PROMPT_MSG="ChumMemory graph is fresh (PCKC v2.2.3). For any code-navigation or recall step, CALL knowledge_query(search, layer:repository) AND mem_search in parallel BEFORE any Read/Grep/Glob/Edit. Before editing a file, CALL knowledge_query(neighbors, nodeId:'file:<path>', layer:repository) first. Grep/Glob is fallback only. Three-way hybrid search: lexical + pgvector + Chroma ML. Reports are graphify-style markdown. Load the ChumMemory skill for the full cookbook if unsure."
SESSION_START_BASE="ChumMemory plugin active (PCKC v2.2.3, MCP server: chum-memory). The hook auto-runs repository_sync before every turn — do NOT call project_import or repository_sync manually. On every code-related prompt: knowledge_query(search, layer:repository) + mem_search in parallel first; Grep/Glob is the fallback only. Two layers: repository (code structure, AST) and session (interaction history). Always pass layer. Three-way hybrid search (lexical + pgvector + Chroma). Typed partitions for per-type precision. Hierarchical communities (level-0 + level-1). Governance: use claim_govern to pin/archive/reject claims. Load the ChumMemory skill for the full cookbook and decision tree."

# ── Fetch knowledge report on session start for codebase context ──
# Returns a JSON-safe string (newlines escaped) suitable for embedding in
# the additionalContext field. Empty string on failure.
fetch_knowledge_report_escaped() {
  local api_url="${CHUM_MEMORY_API_URL:-http://localhost:63001}"
  local report=""
  report=$(curl -sf --max-time 5 "${api_url}/api/knowledge/report?layer=repository" 2>/dev/null) || return 1
  echo "$report" | python3 -c "
import sys, json
try:
    data = json.load(sys.stdin)
    r = data.get('report', '')
    if isinstance(r, dict):
        r = r.get('markdown', '') or r.get('report', '')
    if len(r) > 2000:
        r = r[:2000] + '\n... (truncated, use knowledge_report for full)'
    # Escape for JSON string embedding: backslashes, quotes, newlines, tabs
    r = r.replace('\\\\', '\\\\\\\\').replace('\"', '\\\\\"').replace('\n', '\\\\n').replace('\t', '\\\\t')
    print(r, end='')
except:
    pass
" 2>/dev/null || echo ""
}

case "$HOOK_EVENT" in
  UserPromptSubmit)
    if [[ "$PROVIDER" == "codex" ]]; then
      emit_codex "$USER_PROMPT_MSG"
    else
      emit_claude "UserPromptSubmit" "$USER_PROMPT_MSG"
    fi
    ;;
  SessionStart)
    # Fetch repository knowledge report to prime the session
    KB_REPORT=$(fetch_knowledge_report_escaped 2>/dev/null || echo "")
    if [[ -n "$KB_REPORT" ]]; then
      SESSION_START_MSG="${SESSION_START_BASE}\\n\\n--- Repository Knowledge Report ---\\n${KB_REPORT}"
    else
      SESSION_START_MSG="$SESSION_START_BASE"
    fi
    if [[ "$PROVIDER" == "codex" ]]; then
      emit_codex "$SESSION_START_MSG"
    else
      emit_claude "SessionStart" "$SESSION_START_MSG"
    fi
    ;;
esac

exit 0
