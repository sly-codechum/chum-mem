#!/usr/bin/env bash
# session-sync.sh — Session layer ingestion for chum-memory.
# Reads a Claude Code or Codex hook payload from stdin, maps it to the
# chum-memory session event schema, and POSTs to the ingestion API.
#
# Called by hook-dispatch.sh for every relevant hook event. Keeps per-session
# state in .chum-cache/session-<provider>-<session-id>.json so events can
# reference the chum-mem session UUID for the whole lifetime of the shell.
#
# Provider is chosen by CHUM_PROVIDER (default "claude"). Valid values are
# "claude", "codex", "gemini" — matches the Provider enum in the Rust API.
#
# Errors are surfaced to stderr with exit 1 (non-blocking) when the API is
# unreachable — the user sees the error but their prompt still proceeds.

set -euo pipefail

API_URL="${CHUM_MEMORY_API_URL:-http://localhost:63001}"
PROJECT_ROOT="${CLAUDE_PROJECT_DIR:-${CODEX_PROJECT_DIR:-$PWD}}"
CACHE_DIR="${PROJECT_ROOT}/.chum-cache"
PROJECT_ID="${CHUM_MEM_PROJECT_ID:-00000000-0000-0000-0000-000000000003}"
PROVIDER="${CHUM_PROVIDER:-claude}"

mkdir -p "$CACHE_DIR"

# Read hook payload from stdin
HOOK_PAYLOAD=$(cat)

if [[ -z "$HOOK_PAYLOAD" ]]; then
  echo "session-sync: empty stdin payload, nothing to do" >&2
  exit 0
fi

HOOK_EVENT=$(echo "$HOOK_PAYLOAD" | jq -r '.hook_event_name // ""')
AGENT_SESSION_ID=$(echo "$HOOK_PAYLOAD" | jq -r '.session_id // ""')

if [[ -z "$AGENT_SESSION_ID" || "$AGENT_SESSION_ID" == "null" ]]; then
  echo "session-sync: ERROR missing session_id in hook payload" >&2
  exit 1
fi

SESSION_STATE_FILE="${CACHE_DIR}/session-${PROVIDER}-${AGENT_SESSION_ID}.json"

# ── Helper: POST session_start ─────────────────────────────────────────────

ensure_session_started() {
  if [[ -f "$SESSION_STATE_FILE" ]]; then
    return 0
  fi

  local repo_url branch commit_sha hostname_val os_val
  repo_url=$(git -C "$PROJECT_ROOT" config --get remote.origin.url 2>/dev/null || echo "")
  branch=$(git -C "$PROJECT_ROOT" branch --show-current 2>/dev/null || echo "")
  commit_sha=$(git -C "$PROJECT_ROOT" rev-parse HEAD 2>/dev/null || echo "")
  hostname_val=$(hostname 2>/dev/null || echo "")
  os_val=$(uname -s 2>/dev/null || echo "")

  # Convert SSH-style git URL to https:// so the API's URL validator accepts it.
  # git@github.com:user/repo.git → https://github.com/user/repo.git
  # git@host-alias:user/repo.git → https://host-alias/user/repo.git
  if [[ "$repo_url" =~ ^git@([^:]+):(.+)$ ]]; then
    repo_url="https://${BASH_REMATCH[1]}/${BASH_REMATCH[2]}"
  fi
  # Drop anything that still doesn't look like an http(s):// URL — the API
  # rejects non-URL values via schema validation.
  if [[ ! "$repo_url" =~ ^https?:// ]]; then
    repo_url=""
  fi

  local payload
  payload=$(jq -n \
    --arg projectId "$PROJECT_ID" \
    --arg externalSessionId "$AGENT_SESSION_ID" \
    --arg repoUrl "$repo_url" \
    --arg branch "$branch" \
    --arg commitSha "$commit_sha" \
    --arg hostname "$hostname_val" \
    --arg os "$os_val" \
    --arg provider "$PROVIDER" \
    '{
      provider: $provider,
      projectId: $projectId,
      externalSessionId: $externalSessionId
    }
    + (
      ({repoUrl: $repoUrl, branch: $branch, commitSha: $commitSha}
        | with_entries(select(.value != "" and .value != null))) as $r
      | if ($r | length) > 0 then {repo: $r} else {} end
    )
    + (
      ({hostname: $hostname, os: $os}
        | with_entries(select(.value != "" and .value != null))) as $l
      | if ($l | length) > 0 then {local: $l} else {} end
    )')

  local response http_code
  response=$(curl -sS --max-time 10 \
    -o /tmp/chum-session-start-resp.$$.json \
    -w "%{http_code}" \
    -X POST \
    -H "Content-Type: application/json" \
    -d "$payload" \
    "${API_URL}/v1/ingest/session/start" 2>&1) || {
    echo "session-sync: ERROR session_start curl failed — API unreachable at ${API_URL}: $response" >&2
    rm -f /tmp/chum-session-start-resp.$$.json
    exit 1
  }

  http_code="$response"
  if [[ "$http_code" != "200" && "$http_code" != "201" ]]; then
    local body
    body=$(cat /tmp/chum-session-start-resp.$$.json 2>/dev/null || echo "")
    echo "session-sync: ERROR session_start returned HTTP ${http_code}: ${body}" >&2
    rm -f /tmp/chum-session-start-resp.$$.json
    exit 1
  fi

  mv /tmp/chum-session-start-resp.$$.json "$SESSION_STATE_FILE"
}

# ── Helper: POST session_event_append ──────────────────────────────────────

post_event() {
  local event_type="$1"
  local payload_json="$2"

  ensure_session_started

  local chum_session_id
  chum_session_id=$(jq -r '.sessionId // ""' "$SESSION_STATE_FILE")

  if [[ -z "$chum_session_id" || "$chum_session_id" == "null" ]]; then
    echo "session-sync: ERROR no sessionId in ${SESSION_STATE_FILE}" >&2
    exit 1
  fi

  local event_id event_time idempotency_key
  event_id=$(python3 -c 'import uuid; print(uuid.uuid4())')
  event_time=$(python3 -c 'import datetime; print(datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%S.%f")[:-3]+"Z")')
  idempotency_key=$(printf '%s|%s|%s' "$chum_session_id" "$event_type" "$event_id" | shasum -a 256 | cut -d' ' -f1)

  local full_payload
  full_payload=$(jq -n \
    --arg sessionId "$chum_session_id" \
    --arg eventId "$event_id" \
    --arg idempotencyKey "$idempotency_key" \
    --arg eventType "$event_type" \
    --arg eventTime "$event_time" \
    --arg provider "$PROVIDER" \
    --argjson payload "$payload_json" \
    --argjson rawPayload "$HOOK_PAYLOAD" \
    '{
      sessionId: $sessionId,
      eventId: $eventId,
      idempotencyKey: $idempotencyKey,
      provider: $provider,
      eventType: $eventType,
      eventTime: $eventTime,
      payload: $payload,
      rawPayload: $rawPayload
    }')

  local http_code
  http_code=$(curl -sS --max-time 10 \
    -o /tmp/chum-session-event-resp.$$.json \
    -w "%{http_code}" \
    -X POST \
    -H "Content-Type: application/json" \
    -d "$full_payload" \
    "${API_URL}/v1/ingest/session/event" 2>&1) || {
    echo "session-sync: ERROR session_event_append curl failed — API unreachable at ${API_URL}" >&2
    rm -f /tmp/chum-session-event-resp.$$.json
    exit 1
  }

  if [[ "$http_code" != "200" && "$http_code" != "201" && "$http_code" != "202" ]]; then
    local body
    body=$(cat /tmp/chum-session-event-resp.$$.json 2>/dev/null || echo "")
    echo "session-sync: ERROR session_event_append returned HTTP ${http_code}: ${body}" >&2
    rm -f /tmp/chum-session-event-resp.$$.json
    exit 1
  fi
  rm -f /tmp/chum-session-event-resp.$$.json
}

# ── Helper: POST session_end ───────────────────────────────────────────────

end_session() {
  if [[ ! -f "$SESSION_STATE_FILE" ]]; then
    return 0
  fi

  local chum_session_id
  chum_session_id=$(jq -r '.sessionId // ""' "$SESSION_STATE_FILE")

  if [[ -z "$chum_session_id" || "$chum_session_id" == "null" ]]; then
    rm -f "$SESSION_STATE_FILE"
    return 0
  fi

  local summary_text
  summary_text=$(echo "$HOOK_PAYLOAD" | jq -r '.last_assistant_message // ""')

  local payload
  payload=$(jq -n \
    --arg sessionId "$chum_session_id" \
    --arg summary "$summary_text" \
    '{sessionId: $sessionId, summary: $summary}')

  local http_code
  http_code=$(curl -sS --max-time 30 \
    -o /tmp/chum-session-end-resp.$$.json \
    -w "%{http_code}" \
    -X POST \
    -H "Content-Type: application/json" \
    -d "$payload" \
    "${API_URL}/v1/ingest/session/end" 2>&1) || {
    echo "session-sync: ERROR session_end curl failed — API unreachable at ${API_URL}" >&2
    rm -f /tmp/chum-session-end-resp.$$.json
    exit 1
  }

  if [[ "$http_code" != "200" && "$http_code" != "201" && "$http_code" != "202" ]]; then
    local body
    body=$(cat /tmp/chum-session-end-resp.$$.json 2>/dev/null || echo "")
    echo "session-sync: ERROR session_end returned HTTP ${http_code}: ${body}" >&2
    rm -f /tmp/chum-session-end-resp.$$.json
    exit 1
  fi
  rm -f /tmp/chum-session-end-resp.$$.json
  rm -f "$SESSION_STATE_FILE"
}

# ── Dispatch by hook event ─────────────────────────────────────────────────

# Payload schema required by the API (see SessionEventPayload in
# rust/crates/chum_mem_contracts/src/lib.rs):
#   { message, toolName, command, exitCode, filePath, diffStat, metadata }
# Unknown fields are silently dropped by the server's JSON deserializer —
# anything extra must go inside `metadata` to survive round-trip.

case "$HOOK_EVENT" in
  SessionStart)
    ensure_session_started
    ;;
  UserPromptSubmit)
    prompt_payload=$(echo "$HOOK_PAYLOAD" | jq -c '{
      message: (.prompt // ""),
      metadata: {source: "UserPromptSubmit"}
    }')
    post_event "prompt" "$prompt_payload"
    ;;
  PreToolUse)
    tool_payload=$(echo "$HOOK_PAYLOAD" | jq -c '{
      toolName: (.tool_name // ""),
      message: (.tool_name // ""),
      metadata: {
        toolUseId: (.tool_use_id // ""),
        input: (.tool_input // {})
      }
    }')
    post_event "tool_call" "$tool_payload"
    ;;
  PostToolUse)
    tool_payload=$(echo "$HOOK_PAYLOAD" | jq -c '{
      toolName: (.tool_name // ""),
      message: (.tool_name // ""),
      filePath: (.tool_input.file_path // .tool_input.path // null),
      command: (.tool_input.command // null),
      metadata: {
        toolUseId: (.tool_use_id // ""),
        input: (.tool_input // {}),
        output: (.tool_response // null)
      }
    }')
    post_event "tool_result" "$tool_payload"
    ;;
  Notification)
    notif_payload=$(echo "$HOOK_PAYLOAD" | jq -c '{
      message: (.message // ""),
      metadata: {
        title: (.title // ""),
        notificationType: (.notification_type // "")
      }
    }')
    post_event "annotation" "$notif_payload"
    ;;
  PreCompact)
    compact_payload=$(echo "$HOOK_PAYLOAD" | jq -c '{
      message: (.trigger // "precompact"),
      metadata: {
        trigger: (.trigger // ""),
        customInstructions: (.custom_instructions // "")
      }
    }')
    post_event "summary" "$compact_payload"
    ;;
  SubagentStop)
    subagent_payload=$(echo "$HOOK_PAYLOAD" | jq -c '{
      message: (.last_assistant_message // "subagent stopped"),
      metadata: {
        agentId: (.agent_id // ""),
        agentType: (.agent_type // ""),
        lastAssistantMessage: (.last_assistant_message // "")
      }
    }')
    post_event "summary" "$subagent_payload"
    ;;
  Stop)
    stop_payload=$(echo "$HOOK_PAYLOAD" | jq -c '{
      message: (.last_assistant_message // ""),
      metadata: {source: "Stop"}
    }')
    post_event "response" "$stop_payload"
    end_session
    ;;
  SessionEnd)
    end_session
    ;;
  *)
    echo "session-sync: unknown hook event '${HOOK_EVENT}', skipping" >&2
    ;;
esac
