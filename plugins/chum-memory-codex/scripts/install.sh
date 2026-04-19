#!/usr/bin/env bash
# install.sh — register chum-memory in the Codex marketplace.
#
# This script does four things:
#   1. Copies the plugin directory to the marketplace
#   2. Installs hooks to ~/.codex/hooks.json (Codex discovery location)
#   3. Creates a marketplace directory with marketplace.json
#   4. Registers the marketplace in ~/.codex/config.toml
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
source "$REPO_ROOT/scripts/plugin-installer-common.sh"

PROFILE="${1:-local}"
CODEX_HOME="${CODEX_HOME:-$HOME/.codex}"
CONFIG_PATH="$CODEX_HOME/config.toml"
MARKETPLACE_DIR="$CODEX_HOME/marketplaces/codechum"
PLUGIN_DEST="$MARKETPLACE_DIR/plugins/chum-mem"
SOURCE_PLUGIN_DIR="$REPO_ROOT/plugins/chum-memory-codex"
SHARED_SCRIPTS_DIR="$CODEX_HOME/chum-memory-scripts"
HOOKS_TEMPLATE_PATH="$SOURCE_PLUGIN_DIR/codex-hooks/hooks.template.json"

chum_memory_require_command "jq" "Install jq: brew install jq (macOS) or apt-get install jq (Linux)"

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
sed "s|http://localhost:63001/mcp|$MCP_URL|g" "$SOURCE_PLUGIN_DIR/.mcp.json" > "$PLUGIN_DEST/.mcp.json"
cp "$SOURCE_PLUGIN_DIR/README.md" "$PLUGIN_DEST/"

# Materialize the shared scripts directory used by Codex hooks.
rm -rf "$SHARED_SCRIPTS_DIR"
mkdir -p "$SHARED_SCRIPTS_DIR"
cp -r "$SOURCE_PLUGIN_DIR/scripts/." "$SHARED_SCRIPTS_DIR/"

# Generate hooks.json with a concrete scripts path so SessionStart and
# subsequent hook events keep working after plugin installation.
mkdir -p "$PLUGIN_DEST/hooks"
sed "s|__SCRIPTS_DIR__|$SHARED_SCRIPTS_DIR|g" "$HOOKS_TEMPLATE_PATH" > "$PLUGIN_DEST/hooks/hooks.json"

echo "✓ Installed plugin to $PLUGIN_DEST"
echo "✓ Installed shared hook scripts to $SHARED_SCRIPTS_DIR"

# ── 1b. Install hooks to ~/.codex/hooks.json (Codex discovery location) ─
# Codex only reads hooks from ~/.codex/hooks.json and <repo>/.codex/hooks.json,
# NOT from plugin directories. Merge our hooks into the user-level file.

CODEX_HOOKS_PATH="$CODEX_HOME/hooks.json"
GENERATED_HOOKS="$PLUGIN_DEST/hooks/hooks.json"

if [[ -f "$CODEX_HOOKS_PATH" ]]; then
  # Remove any existing chum-memory hooks (idempotent reinstall), then
  # concatenate our hook entries alongside any other plugins' hooks.
  jq --arg sd "$SHARED_SCRIPTS_DIR" '
    .hooks |= with_entries(
      .value |= map(select(.hooks | all(.command | contains($sd) | not)))
    )
  ' "$CODEX_HOOKS_PATH" > "$CODEX_HOOKS_PATH.tmp"

  jq -s '
    [.[].hooks | to_entries[]] | group_by(.key) |
    map({key: .[0].key, value: [.[].value[]]}) |
    from_entries | {hooks: .}
  ' "$CODEX_HOOKS_PATH.tmp" "$GENERATED_HOOKS" > "$CODEX_HOOKS_PATH"
  rm -f "$CODEX_HOOKS_PATH.tmp"
  echo "✓ Merged hooks into $CODEX_HOOKS_PATH"
else
  cp "$GENERATED_HOOKS" "$CODEX_HOOKS_PATH"
  echo "✓ Installed hooks to $CODEX_HOOKS_PATH"
fi

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
