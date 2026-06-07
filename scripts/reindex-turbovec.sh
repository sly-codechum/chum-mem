#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

reset_index=false
start_services=true

usage() {
  cat <<'EOF'
Usage: scripts/reindex-turbovec.sh [--reset-index] [--no-start]

Rebuild TurboVec indexes from PostgreSQL source-of-truth by enqueueing one
full-project sync job per project. This does not read or mutate Chroma data.

Options:
  --reset-index  Remove existing files from the TurboVec volume before enqueueing jobs.
  --no-start     Do not start api/worker/postgres before enqueueing.
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --reset-index)
      reset_index=true
      shift
      ;;
    --no-start)
      start_services=false
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

export VECTOR_STORE_BACKEND="${VECTOR_STORE_BACKEND:-turbovec}"
export TURBOVEC_PATH="${TURBOVEC_PATH:-/data/turbovec}"
export TURBOVEC_BIT_WIDTH="${TURBOVEC_BIT_WIDTH:-4}"
export WORKER_CONCURRENCY="${WORKER_CONCURRENCY:-1}"

if [[ "$VECTOR_STORE_BACKEND" != "turbovec" ]]; then
  echo "VECTOR_STORE_BACKEND must be turbovec for this migration; got: $VECTOR_STORE_BACKEND" >&2
  exit 1
fi

if [[ "$reset_index" == true ]]; then
  docker compose stop worker api >/dev/null 2>&1 || true
fi

if [[ "$start_services" == true ]]; then
  docker compose up -d --build postgres
fi

if [[ "$reset_index" == true ]]; then
  docker compose build worker
  echo "Resetting TurboVec index files under ${TURBOVEC_PATH}"
  docker compose run --rm --no-deps --user root worker \
    sh -lc 'mkdir -p "${TURBOVEC_PATH:?}" && chown -R 10001:10001 "${TURBOVEC_PATH}" && rm -rf "${TURBOVEC_PATH:?}/"*'
fi

if [[ "$start_services" == true ]]; then
  docker compose up -d --build worker api
fi

echo "Enqueueing full-project TurboVec reindex jobs from PostgreSQL"
docker compose exec -T postgres psql \
  -U "${POSTGRES_USER:-chum_mem}" \
  -d "${POSTGRES_DB:-chum_mem}" \
  -v ON_ERROR_STOP=1 <<'SQL'
insert into public.worker_jobs (
  organization_id,
  team_id,
  project_id,
  session_id,
  memory_id,
  job_type,
  dedupe_key,
  priority,
  max_attempts,
  available_at,
  payload
)
select
  organization_id,
  team_id,
  id,
  null,
  null,
  'sync-chroma-index',
  'turbovec-full-reindex:' || id::text,
  10,
  3,
  now(),
  jsonb_build_object(
    'projectId', id::text,
    'backend', 'turbovec',
    'mode', 'full-reindex',
    'source', 'postgres'
  )
from public.projects
on conflict (project_id, job_type, dedupe_key) where status in ('pending', 'running')
do update set
  payload = excluded.payload,
  available_at = now(),
  priority = excluded.priority,
  updated_at = now();

select
  status,
  count(*) as job_count
from public.worker_jobs
where job_type = 'sync-chroma-index'
  and dedupe_key like 'turbovec-full-reindex:%'
group by status
order by status;
SQL

cat <<EOF

TurboVec reindex jobs are queued.

Monitor progress:
  docker compose logs -f worker

Check job status:
  docker compose exec postgres psql -U ${POSTGRES_USER:-chum_mem} -d ${POSTGRES_DB:-chum_mem} -c "select status, count(*) from public.worker_jobs where job_type = 'sync-chroma-index' and dedupe_key like 'turbovec-full-reindex:%' group by status order by status;"

Check TurboVec files:
  docker compose exec worker sh -lc 'find ${TURBOVEC_PATH} -maxdepth 1 -type f | sort | head -50'
EOF
