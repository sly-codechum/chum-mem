#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CLIENT="${1:-}"
PROFILE="${2:-local}"

usage() {
  cat <<'EOF'
Usage:
  ./plugin-install.sh codex local
  ./plugin-install.sh codex production
  ./plugin-install.sh claude local
  ./plugin-install.sh claude production
  ./plugin-install.sh gemini local
  ./plugin-install.sh gemini production
EOF
}

if [[ -z "$CLIENT" ]]; then
  usage
  exit 1
fi

case "$CLIENT" in
  codex)
    exec bash "$REPO_ROOT/plugins/chum-memory-codex/scripts/install.sh" "$PROFILE"
    ;;
  claude)
    exec bash "$REPO_ROOT/plugins/chum-memory-claude/scripts/install.sh" "$PROFILE"
    ;;
  gemini)
    exec bash "$REPO_ROOT/extensions/chum-memory-gemini/scripts/install.sh" "$PROFILE"
    ;;
  *)
    echo "Unsupported client: $CLIENT (use: codex|claude|gemini)" >&2
    usage >&2
    exit 1
    ;;
esac
