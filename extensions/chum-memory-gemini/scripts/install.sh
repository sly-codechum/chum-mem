#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
source "$REPO_ROOT/scripts/plugin-installer-common.sh"

EXT_PATH="$REPO_ROOT/extensions/chum-memory-gemini"
PROFILE="${1:-local}"
MCP_URL="$(chum_memory_profile_url "$PROFILE")"

chum_memory_require_command "gemini" "Install Gemini CLI first, then rerun this script."

echo "Installing Gemini extension from: $EXT_PATH"
gemini extensions install "$EXT_PATH"

echo "Updating Gemini MCP server: chum-memory -> $MCP_URL"
gemini mcp remove chum-memory >/dev/null 2>&1 || true
gemini mcp add chum-memory "$MCP_URL" >/dev/null

# Remove stale MCP server overrides from settings.json that conflict with
# the single configured endpoint.
SETTINGS_FILE="${GEMINI_HOME:-$HOME/.gemini}/settings.json"
if [[ -f "$SETTINGS_FILE" ]] && command -v python3 >/dev/null 2>&1; then
  python3 -c "
import json, sys
p = sys.argv[1]
with open(p) as f:
    data = json.load(f)
servers = data.get('mcpServers', {})
removed = []
for name in ['chum-memory-local']:
    if name in servers:
        removed.append(name)
        del servers[name]
if removed:
    if not servers:
        data.pop('mcpServers', None)
    with open(p, 'w') as f:
        json.dump(data, f, indent=2)
        f.write('\n')
    print('Cleaned stale MCP overrides from settings.json: ' + ', '.join(removed))
" "$SETTINGS_FILE"
fi

cat <<EOF
Installed Gemini extension: chum-memory

Configured MCP server:
- chum-memory ($MCP_URL)
EOF
