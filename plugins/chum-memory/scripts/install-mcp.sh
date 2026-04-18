#!/usr/bin/env bash
# install-mcp.sh — install chum-memory for Codex globally.
#
# Writes five things:
#   1. ~/.codex/plugins/chum-memory/ — full plugin directory for marketplace
#   2. ~/.agents/plugins/marketplace.json — personal marketplace registration
#   3. ~/.codex/config.toml — MCP server entry so Codex can reach the API
#   4. ~/.codex/chum-memory-scripts/{sync.sh, session-sync.sh, hook-dispatch.sh}
#      copied from the repo so hooks.json can use stable absolute paths
#   5. ~/.codex/hooks.json — Codex hooks config with absolute paths baked in
#
# Running multiple times is safe: the MCP entry is replaced in place,
# scripts are overwritten, and hooks.json is merged with any existing
# non-chum-memory hooks via jq.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
source "$REPO_ROOT/scripts/plugin-installer-common.sh"

PROFILE="${1:-local}"
CODEX_HOME="${CODEX_HOME:-$HOME/.codex}"
AGENTS_HOME="${AGENTS_HOME:-$HOME/.agents}"
CONFIG_PATH="$CODEX_HOME/config.toml"
HOOKS_PATH="$CODEX_HOME/hooks.json"
SCRIPTS_DEST="$CODEX_HOME/chum-memory-scripts"
PLUGIN_DEST="$CODEX_HOME/plugins/chum-memory"
MARKETPLACE_PATH="$AGENTS_HOME/plugins/marketplace.json"
TEMPLATE_PATH="$REPO_ROOT/plugins/chum-memory/codex-hooks/hooks.template.json"
SOURCE_PLUGIN_DIR="$REPO_ROOT/plugins/chum-memory"
SOURCE_SCRIPTS_DIR="$SOURCE_PLUGIN_DIR/scripts"

MCP_URL="$(chum_memory_profile_url "$PROFILE")"

mkdir -p "$CODEX_HOME" "$CODEX_HOME/plugins" "$SCRIPTS_DEST" "$AGENTS_HOME/plugins"

# ── 1. Copy plugin directory for marketplace discovery ───────────────────

rm -rf "$PLUGIN_DEST"
mkdir -p "$PLUGIN_DEST"

cp -r "$SOURCE_PLUGIN_DIR/.codex-plugin" "$PLUGIN_DEST/"
cp -r "$SOURCE_PLUGIN_DIR/skills" "$PLUGIN_DEST/"
cp -r "$SOURCE_PLUGIN_DIR/scripts" "$PLUGIN_DEST/"
cp -r "$SOURCE_PLUGIN_DIR/hooks" "$PLUGIN_DEST/"
sed "s|http://localhost:63001/mcp|$MCP_URL|g" "$SOURCE_PLUGIN_DIR/.mcp.json" > "$PLUGIN_DEST/.mcp.json"
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
        "path": "'"$PLUGIN_DEST"'"
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
  BACKUP="$MARKETPLACE_PATH.bak.$(date +%s)"
  cp "$MARKETPLACE_PATH" "$BACKUP"

  jq --arg path "$PLUGIN_DEST" '
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

# ── 3. MCP server entry in config.toml ────────────────────────────────────

touch "$CONFIG_PATH"

awk '
  BEGIN { skip = 0 }
  /^\[mcp_servers\.chum_memory\]/ { skip = 1; next }
  /^\[mcp_servers\."chum-memory"\]/ { skip = 1; next }
  /^\[/ {
    if (skip == 1) {
      skip = 0
    }
    print
    next
  }
  {
    if (skip == 0) {
      print
    }
  }
' "$CONFIG_PATH" > "$CONFIG_PATH.tmp"
mv "$CONFIG_PATH.tmp" "$CONFIG_PATH"

{
  printf '\n[mcp_servers."chum-memory"]\n'
  printf 'enabled = true\n'
  printf 'url = "%s"\n' "$MCP_URL"
} >> "$CONFIG_PATH"

echo "✓ Configured MCP server: chum-memory -> $MCP_URL"
echo "  Config file: $CONFIG_PATH"

# ── 4. Copy hook scripts to a stable global location ─────────────────────

for script in sync.sh session-sync.sh hook-dispatch.sh; do
  src="$SOURCE_SCRIPTS_DIR/$script"
  if [[ ! -f "$src" ]]; then
    echo "✗ Missing source script: $src" >&2
    exit 1
  fi
  install -m 755 "$src" "$SCRIPTS_DEST/$script"
done

echo "✓ Installed hook scripts to $SCRIPTS_DEST"

# ── 5. Render and install hooks.json ─────────────────────────────────────

if [[ ! -f "$TEMPLATE_PATH" ]]; then
  echo "✗ Missing hooks template: $TEMPLATE_PATH" >&2
  exit 1
fi

RENDERED_HOOKS="$(sed "s|__SCRIPTS_DIR__|$SCRIPTS_DEST|g" "$TEMPLATE_PATH")"

mkdir -p "$PLUGIN_DEST/hooks"
printf '%s\n' "$RENDERED_HOOKS" > "$PLUGIN_DEST/hooks/hooks.json"

if [[ -f "$HOOKS_PATH" ]]; then
  if command -v jq >/dev/null 2>&1; then
    BACKUP="$HOOKS_PATH.bak.$(date +%s)"
    cp "$HOOKS_PATH" "$BACKUP"

    jq --argjson new "$RENDERED_HOOKS" '
      def strip_chum:
        map(select(
          (.hooks // []) | map(.command // "") | any(contains("chum-memory-scripts")) | not
        ));
      (.hooks // {}) as $existing
      | {
          hooks: (
            ($existing | to_entries | map(.value |= strip_chum) | from_entries) as $clean
            | reduce ($new.hooks | to_entries[]) as $entry ($clean;
                .[$entry.key] = (($clean[$entry.key] // []) + $entry.value)
              )
          )
        }
    ' "$HOOKS_PATH" > "$HOOKS_PATH.tmp"
    mv "$HOOKS_PATH.tmp" "$HOOKS_PATH"
    echo "✓ Merged hooks into existing $HOOKS_PATH (backup: $BACKUP)"
  else
    echo "⚠ jq not found — overwriting $HOOKS_PATH (existing hooks backed up to $HOOKS_PATH.bak)"
    cp "$HOOKS_PATH" "$HOOKS_PATH.bak"
    printf '%s\n' "$RENDERED_HOOKS" > "$HOOKS_PATH"
  fi
else
  printf '%s\n' "$RENDERED_HOOKS" > "$HOOKS_PATH"
  echo "✓ Installed hooks.json at $HOOKS_PATH"
fi

echo ""
echo "Codex plugin installation complete!"
echo ""
echo "Plugin is now available in the Codex marketplace."
echo "Open Codex and go to Plugins to install 'ChumMemory'."
echo ""
echo "Installed components:"
echo "  - Plugin:      $PLUGIN_DEST"
echo "  - Marketplace: $MARKETPLACE_PATH"
echo "  - MCP server:  chum-memory -> $MCP_URL"
echo "  - Hooks:       $HOOKS_PATH"
