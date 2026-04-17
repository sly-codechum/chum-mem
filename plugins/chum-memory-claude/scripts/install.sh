#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
source "$REPO_ROOT/scripts/plugin-installer-common.sh"

PROFILE="${1:-local}"
MARKETPLACE_ROOT="$REPO_ROOT"
MARKETPLACE_NAME="personal"
PLUGIN_NAME="chum-memory"
PLUGIN_DIR="$REPO_ROOT/plugins/chum-memory-claude"
MCP_URL="$(chum_memory_profile_url "$PROFILE")"

chum_memory_require_command "claude" "Install Claude Code first, then rerun this script."

if [[ ! -f "$PLUGIN_DIR/.claude-plugin/plugin.json" ]]; then
  echo "Missing plugin manifest at $PLUGIN_DIR/.claude-plugin/plugin.json" >&2
  exit 1
fi

# Remove any standalone MCP server to avoid duplicates.
# The MCP is bundled inside the plugin via .mcp.json.
echo "Cleaning up standalone MCP server (if any)..."
claude mcp remove chum-memory >/dev/null 2>&1 || true
claude mcp remove chum-memory-local >/dev/null 2>&1 || true

# Update the bundled .mcp.json with the correct URL for this profile
echo "Configuring bundled MCP endpoint: $MCP_URL"
cat > "$PLUGIN_DIR/.mcp.json" <<MCPEOF
{
  "mcpServers": {
    "chum-memory": {
      "type": "http",
      "url": "$MCP_URL"
    }
  }
}
MCPEOF

echo "Adding Personal Plugins marketplace from: $MARKETPLACE_ROOT"
claude plugins marketplace add "$MARKETPLACE_ROOT" 2>/dev/null || true

echo "Installing plugin: ${PLUGIN_NAME}@${MARKETPLACE_NAME}"
if ! claude plugins install "${PLUGIN_NAME}@${MARKETPLACE_NAME}"; then
  echo "Primary marketplace install failed, retrying legacy aliases." >&2
  claude plugins install "${PLUGIN_NAME}@codechum-chum-memory" \
    || claude plugins install "${PLUGIN_NAME}@codechum"
fi

cat <<EOF

Installed Claude Code plugin: chum-memory

MCP server bundled in plugin:
  - chum-memory ($MCP_URL)

No standalone MCP server registered — the plugin manages its own MCP connection.
If Claude Code is already running, reload plugins with: /reload-plugins
EOF
