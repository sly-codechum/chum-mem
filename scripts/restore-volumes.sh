#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

backup_dir="${1:-}"
if [[ -z "$backup_dir" ]]; then
  echo "Usage: ./scripts/restore-volumes.sh <backup-directory>"
  exit 1
fi

if [[ ! -d "$backup_dir" ]]; then
  echo "Backup directory does not exist: $backup_dir"
  exit 1
fi

postgres_archive="${backup_dir}/postgres_data.tar.gz"
chroma_archive="${backup_dir}/chroma_data.tar.gz"
if [[ ! -f "$postgres_archive" || ! -f "$chroma_archive" ]]; then
  echo "Backup directory must contain postgres_data.tar.gz and chroma_data.tar.gz"
  exit 1
fi

project_name="${COMPOSE_PROJECT_NAME:-$(docker compose config | awk '/^name:/{print $2; exit}')}"
if [[ -z "${project_name:-}" ]]; then
  echo "Unable to determine compose project name."
  exit 1
fi

postgres_volume="${POSTGRES_VOLUME:-${project_name}_postgres_data}"
chroma_volume="${CHROMA_VOLUME:-${project_name}_chroma_data}"

restore_volume() {
  local volume_name="$1"
  local archive_path="$2"

  docker volume create "$volume_name" >/dev/null

  echo "Restoring $volume_name from $(basename "$archive_path")"
  docker run --rm \
    -v "${volume_name}:/volume" \
    -v "${backup_dir}:/backup:ro" \
    alpine:3.20 \
    sh -c "rm -rf /volume/* /volume/.[!.]* /volume/..?* 2>/dev/null || true; tar xzf /backup/$(basename "$archive_path") -C /volume"
}

restore_volume "$postgres_volume" "$postgres_archive"
restore_volume "$chroma_volume" "$chroma_archive"

echo "Restore complete."
