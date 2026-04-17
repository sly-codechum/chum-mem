-- Migration 0006: Fix worker_jobs job_type CHECK constraint
-- 0004 defined job_type as TEXT with a CHECK constraint, but 0005 tried to
-- ALTER TYPE on a non-existent enum. This migration drops the old CHECK and
-- replaces it with one that includes all knowledge pipeline job types.

ALTER TABLE public.worker_jobs DROP CONSTRAINT IF EXISTS worker_jobs_job_type_check;

ALTER TABLE public.worker_jobs ADD CONSTRAINT worker_jobs_job_type_check
  CHECK (job_type IN (
    'sync-chroma-index',
    'replay-failed-session',
    'build-knowledge-graph',
    'detect-communities',
    'generate-knowledge-report',
    'export-knowledge-snapshot'
  ));
