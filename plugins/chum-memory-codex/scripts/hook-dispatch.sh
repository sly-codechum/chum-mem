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

# ── Resolve project identity (.chum-mem) ──
CHUM_MEM_FILE="${PROJECT_DIR}/.chum-mem"
if [[ -f "$CHUM_MEM_FILE" ]]; then
  RESOLVED_PROJECT_ID=$(jq -r '.projectId // ""' "$CHUM_MEM_FILE" 2>/dev/null || echo "")
fi
if [[ -z "${RESOLVED_PROJECT_ID:-}" || "$RESOLVED_PROJECT_ID" == "null" ]]; then
  REPO_URL=$(git -C "$PROJECT_DIR" config --get remote.origin.url 2>/dev/null || echo "")
  if [[ "$REPO_URL" =~ ^git@([^:]+):(.+)$ ]]; then
    REPO_URL="https://${BASH_REMATCH[1]}/${BASH_REMATCH[2]}"
  fi
  [[ ! "$REPO_URL" =~ ^https?:// ]] && REPO_URL=""
  PROJECT_NAME=$(basename "$PROJECT_DIR")
  RESOLVE_PAYLOAD=$(jq -n --arg name "$PROJECT_NAME" --arg repoUrl "$REPO_URL" \
    '{name: $name} + (if $repoUrl != "" then {repoUrl: $repoUrl} else {} end)')
  RESOLVE_RESP=$(curl -sf --max-time 5 -X POST -H "Content-Type: application/json" \
    -d "$RESOLVE_PAYLOAD" "${API_URL}/v1/projects/resolve" 2>/dev/null) || RESOLVE_RESP=""
  if [[ -n "$RESOLVE_RESP" ]]; then
    RESOLVED_PROJECT_ID=$(echo "$RESOLVE_RESP" | jq -r '.projectId // ""' 2>/dev/null || echo "")
    if [[ -n "$RESOLVED_PROJECT_ID" && "$RESOLVED_PROJECT_ID" != "null" ]]; then
      echo "$RESOLVE_RESP" | jq '{projectId: .projectId, name: .name}' > "$CHUM_MEM_FILE" 2>/dev/null || true
    fi
  fi
fi
export CHUM_MEM_PROJECT_ID="${RESOLVED_PROJECT_ID:-${CHUM_MEM_PROJECT_ID:-}}"

# ── Ensure .mcp.json carries the project ID in the URL ──
if [[ -n "${CHUM_MEM_PROJECT_ID:-}" ]]; then
  MCP_JSON_PATH="${SCRIPTS_DIR}/../.mcp.json"
  MCP_URL_WITH_PROJECT="${API_URL}/mcp?projectId=${CHUM_MEM_PROJECT_ID}"
  if [[ -f "$MCP_JSON_PATH" ]]; then
    CURRENT_URL=$(jq -r '.mcpServers["chum-memory"].url // ""' "$MCP_JSON_PATH" 2>/dev/null || echo "")
    if [[ "$CURRENT_URL" != "$MCP_URL_WITH_PROJECT" ]]; then
      jq --arg url "$MCP_URL_WITH_PROJECT" \
        '.mcpServers["chum-memory"].url = $url' "$MCP_JSON_PATH" > "${MCP_JSON_PATH}.tmp" \
        && mv "${MCP_JSON_PATH}.tmp" "$MCP_JSON_PATH"
    fi
  fi
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
  local message="$1"
  printf '{"systemMessage":"%s"}\n' "$message"
}

USER_PROMPT_MSG="ChumMemory graph is fresh (PCKC v2.2.3). For any code-navigation or recall step, CALL knowledge_query(search, layer:repository) AND mem_search in parallel BEFORE any Read/Grep/Glob/Edit. Before editing a file, CALL knowledge_query(neighbors, nodeId:'file:<path>', layer:repository) first. Grep/Glob is fallback only. Three-way hybrid search: lexical + pgvector + Chroma ML. Reports are graphify-style markdown. Load the ChumMemory skill for the full cookbook if unsure."
SESSION_START_BASE="ChumMemory plugin active (PCKC v2.2.3, MCP server: chum-memory). Multi-project mode: each project folder has its own project ID (auto-resolved via .chum-mem). Repository layer (knowledge_query, knowledge_report, knowledge_communities) is STRICTLY per-project — projectId is required, no global fallback. Session layer knowledge queries fall back to global project if no project-specific snapshot exists. mem_search falls back to global project for historical memories. The hook auto-runs repository_sync before every turn — do NOT call project_import or repository_sync manually. On every code-related prompt: knowledge_query(search, layer:repository) + mem_search in parallel first; Grep/Glob is the fallback only. Two layers: repository (code structure, AST) and session (interaction history). Always pass layer. Three-way hybrid search (lexical + pgvector + Chroma). Typed partitions for per-type precision. Hierarchical communities (level-0 + level-1). Governance: use claim_govern to pin/archive/reject claims. Load the ChumMemory skill for the full cookbook and decision tree."

# ── Fetch knowledge report on session start for codebase context ──
fetch_knowledge_report_escaped() {
  local api_url="${CHUM_MEMORY_API_URL:-http://localhost:63001}"
  local qs="layer=repository"
  [[ -n "${CHUM_MEM_PROJECT_ID:-}" ]] && qs="${qs}&projectId=${CHUM_MEM_PROJECT_ID}"
  local report=""
  report=$(curl -sf --max-time 5 "${api_url}/api/knowledge/report?${qs}" 2>/dev/null) || return 1
  [[ -z "$report" ]] && return 1
  printf '%s' "${report:0:2000}" | jq -Rs '.' 2>/dev/null | sed 's/^"//;s/"$//' || echo ""
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
