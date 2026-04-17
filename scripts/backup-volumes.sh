#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

timestamp="$(date +"%Y%m%d-%H%M%S")"
backup_root="${BACKUP_ROOT:-$ROOT_DIR/backups}"
backup_dir="${BACKUP_DIR:-$backup_root/$timestamp}"
mkdir -p "$backup_dir"

project_name="${COMPOSE_PROJECT_NAME:-$(docker compose config | awk '/^name:/{print $2; exit}')}"
if [[ -z "${project_name:-}" ]]; then
  echo "Unable to determine compose project name."
  exit 1
fi

postgres_volume="${POSTGRES_VOLUME:-${project_name}_postgres_data}"
chroma_volume="${CHROMA_VOLUME:-${project_name}_chroma_data}"

backup_volume() {
  local volume_name="$1"
  local archive_name="$2"

  if ! docker volume inspect "$volume_name" >/dev/null 2>&1; then
    echo "Volume not found: $volume_name"
    echo "Start the stack at least once with: docker compose up -d"
    echo "Then retry backup."
    exit 1
  fi

  echo "Backing up $volume_name -> $archive_name"
  docker run --rm \
    -v "${volume_name}:/volume:ro" \
    -v "${backup_dir}:/backup" \
    alpine:3.20 \
    sh -c "cd /volume && tar czf /backup/${archive_name} ."
}

backup_volume "$postgres_volume" "postgres_data.tar.gz"
backup_volume "$chroma_volume" "chroma_data.tar.gz"

cat > "${backup_dir}/manifest.txt" <<EOF
created_at=$(date -u +"%Y-%m-%dT%H:%M:%SZ")
compose_project=${project_name}
postgres_volume=${postgres_volume}
chroma_volume=${chroma_volume}
EOF

echo "Backup complete: $backup_dir"
