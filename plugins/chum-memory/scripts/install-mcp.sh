#!/usr/bin/env bash
# install-mcp.sh — register the chum-memory Codex plugin in the personal
# marketplace without pre-installing its MCP or skills into Codex.
#
# Writes two things:
#   1. ~/.codex/plugins/chum-memory/ — local plugin package for marketplace use
#   2. ~/.agents/plugins/marketplace.json — personal marketplace registration
#
# Running multiple times is safe: the plugin package is replaced in place and
# the marketplace entry is updated in place when jq is available.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
source "$REPO_ROOT/scripts/plugin-installer-common.sh"

PROFILE="${1:-local}"
CODEX_HOME="${CODEX_HOME:-$HOME/.codex}"
AGENTS_HOME="${AGENTS_HOME:-$HOME/.agents}"
PLUGIN_DEST="$CODEX_HOME/plugins/chum-memory"
MARKETPLACE_PATH="$AGENTS_HOME/plugins/marketplace.json"
MARKETPLACE_PLUGIN_PATH="./.codex/plugins/chum-memory"
SOURCE_PLUGIN_DIR="$REPO_ROOT/plugins/chum-memory"

MCP_URL="$(chum_memory_profile_url "$PROFILE")"

mkdir -p "$CODEX_HOME/plugins" "$AGENTS_HOME/plugins"

# ── 1. Copy plugin directory for marketplace discovery ───────────────────

rm -rf "$PLUGIN_DEST"
mkdir -p "$PLUGIN_DEST"

# Copy .codex-plugin manifest
cp -r "$SOURCE_PLUGIN_DIR/.codex-plugin" "$PLUGIN_DEST/"

# Copy skills directory
cp -r "$SOURCE_PLUGIN_DIR/skills" "$PLUGIN_DEST/"

# Copy scripts directory for plugin-managed hooks/workflows
cp -r "$SOURCE_PLUGIN_DIR/scripts" "$PLUGIN_DEST/"

# Copy hooks directory
cp -r "$SOURCE_PLUGIN_DIR/hooks" "$PLUGIN_DEST/"

# Copy .mcp.json and update URL based on profile
sed "s|http://localhost:63001/mcp|$MCP_URL|g" "$SOURCE_PLUGIN_DIR/.mcp.json" > "$PLUGIN_DEST/.mcp.json"

# Copy README
cp "$SOURCE_PLUGIN_DIR/README.md" "$PLUGIN_DEST/"

echo "✓ Installed plugin to $PLUGIN_DEST"

# ── 2. Register in personal marketplace ──────────────────────────────────

MARKETPLACE_ENTRY='{
  "name": "personal",
  "interface": {
    "displayName": "Personal Plugins"
  },
  "plugins": [
        {
      "name": "chum-memory",
      "source": {
        "source": "local",
        "path": "'"$MARKETPLACE_PLUGIN_PATH"'"
      },
      "policy": {
        "installation": "AVAILABLE",
        "authentication": "ON_INSTALL"
      },
      "category": "Coding"
    }
  ]
}'

if [[ -f "$MARKETPLACE_PATH" ]] && command -v jq >/dev/null 2>&1; then
  # Merge: add/update chum-memory plugin in existing marketplace
  BACKUP="$MARKETPLACE_PATH.bak.$(date +%s)"
  cp "$MARKETPLACE_PATH" "$BACKUP"

  jq --arg path "$MARKETPLACE_PLUGIN_PATH" '
    .plugins = ((.plugins // []) | map(select(.name != "chum-memory"))) + [{
      "name": "chum-memory",
      "source": {
        "source": "local",
        "path": $path
      },
      "policy": {
        "installation": "AVAILABLE",
        "authentication": "ON_INSTALL"
      },
      "category": "Coding"
    }]
  ' "$MARKETPLACE_PATH" > "$MARKETPLACE_PATH.tmp"
  mv "$MARKETPLACE_PATH.tmp" "$MARKETPLACE_PATH"
  echo "✓ Updated personal marketplace at $MARKETPLACE_PATH (backup: $BACKUP)"
elif [[ -f "$MARKETPLACE_PATH" ]]; then
  echo "✗ Cannot update $MARKETPLACE_PATH without jq." >&2
  echo "  Install jq or remove the existing marketplace file and rerun." >&2
  exit 1
else
  printf '%s\n' "$MARKETPLACE_ENTRY" > "$MARKETPLACE_PATH"
  echo "✓ Created personal marketplace at $MARKETPLACE_PATH"
fi

echo ""
echo "Codex marketplace registration complete!"
echo ""
echo "Plugin is now available in Codex under Personal Plugins."
echo "Open Codex, go to Plugins, and install 'ChumMemory' there."
echo ""
echo "Registered components:"
echo "  - Plugin package: $PLUGIN_DEST"
echo "  - Marketplace:    $MARKETPLACE_PATH"
echo "  - Plugin profile: $PROFILE ($MCP_URL)"
