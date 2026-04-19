#!/usr/bin/env bash
# install.sh — register chum-memory in the Codex marketplace.
#
# This script does three things:
#   1. Copies the plugin directory to ~/.codex/plugins/chum-mem/
#   2. Creates a marketplace directory with marketplace.json
#   3. Registers the marketplace in ~/.codex/config.toml
#
# The user then manually installs the plugin from Codex → Plugins →
# ChumMem. Codex handles MCP server activation, hooks, and script
# paths from the plugin manifest at install time.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
source "$REPO_ROOT/scripts/plugin-installer-common.sh"

PROFILE="${1:-local}"
CODEX_HOME="${CODEX_HOME:-$HOME/.codex}"
CONFIG_PATH="$CODEX_HOME/config.toml"
MARKETPLACE_DIR="$CODEX_HOME/marketplaces/codechum"
PLUGIN_DEST="$MARKETPLACE_DIR/plugins/chum-mem"
SOURCE_PLUGIN_DIR="$REPO_ROOT/plugins/chum-memory-codex"

MCP_URL="$(chum_memory_profile_url "$PROFILE")"

mkdir -p "$MARKETPLACE_DIR/.agents/plugins" "$MARKETPLACE_DIR/plugins"

# ── 0. Clean up stale registrations from older installers ────────────────

rm -f "$HOME/.agents/plugins/marketplace.json" 2>/dev/null || true
rm -rf "$CODEX_HOME/plugins/chum-memory-codex" 2>/dev/null || true

# ── 1. Copy plugin directory ─────────────────────────────────────────────

rm -rf "$PLUGIN_DEST"
mkdir -p "$PLUGIN_DEST"

cp -r "$SOURCE_PLUGIN_DIR/.codex-plugin" "$PLUGIN_DEST/"
cp -r "$SOURCE_PLUGIN_DIR/skills" "$PLUGIN_DEST/"
cp -r "$SOURCE_PLUGIN_DIR/scripts" "$PLUGIN_DEST/"
cp -r "$SOURCE_PLUGIN_DIR/hooks" "$PLUGIN_DEST/"
sed "s|http://localhost:63001/mcp|$MCP_URL|g" "$SOURCE_PLUGIN_DIR/.mcp.json" > "$PLUGIN_DEST/.mcp.json"
cp "$SOURCE_PLUGIN_DIR/README.md" "$PLUGIN_DEST/"

echo "✓ Installed plugin to $PLUGIN_DEST"

# ── 2. Write marketplace.json ────────────────────────────────────────────

cat > "$MARKETPLACE_DIR/.agents/plugins/marketplace.json" <<'MARKETPLACE'
{
  "name": "chum-mem",
  "interface": {
    "displayName": "CodeChum Plugins"
  },
  "plugins": [
    {
      "name": "chum-mem",
      "source": {
        "source": "local",
        "path": "./plugins/chum-mem"
      },
      "policy": {
        "installation": "AVAILABLE",
        "authentication": "ON_INSTALL"
      },
      "category": "Coding"
    }
  ]
}
MARKETPLACE

echo "✓ Created marketplace at $MARKETPLACE_DIR/.agents/plugins/marketplace.json"

# ── 3. Register marketplace in config.toml ───────────────────────────────

touch "$CONFIG_PATH"

# Remove any existing codechum marketplace entry
awk '
  BEGIN { skip = 0 }
  /^\[marketplaces\.chum-mem\]/ { skip = 1; next }
  /^\[marketplaces\."chum-mem"\]/ { skip = 1; next }
  /^\[/ {
    if (skip == 1) { skip = 0 }
    print; next
  }
  { if (skip == 0) print }
' "$CONFIG_PATH" > "$CONFIG_PATH.tmp"
mv "$CONFIG_PATH.tmp" "$CONFIG_PATH"

{
  printf '\n[marketplaces.chum-mem]\n'
  printf 'source_type = "local"\n'
  printf 'source = "%s"\n' "$MARKETPLACE_DIR"
} >> "$CONFIG_PATH"

echo "✓ Registered marketplace in $CONFIG_PATH"

echo ""
echo "Codex marketplace registration complete!"
echo ""
echo "Next steps:"
echo "  1. Restart Codex"
echo "  2. Go to Plugins → CodeChum Plugins"
echo "  3. Install 'ChumMem'"
echo "  4. Codex will activate the MCP server ($MCP_URL) automatically"
echo ""
echo "Registered components:"
echo "  - Plugin:      $PLUGIN_DEST"
echo "  - Marketplace: $MARKETPLACE_DIR/.agents/plugins/marketplace.json"
echo "  - Config:      $CONFIG_PATH [marketplaces.codechum]"
