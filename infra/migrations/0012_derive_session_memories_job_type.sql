-- Migration 0012: Add derive-session-memories to worker_jobs job_type constraint

ALTER TABLE public.worker_jobs DROP CONSTRAINT IF EXISTS worker_jobs_job_type_check;

ALTER TABLE public.worker_jobs ADD CONSTRAINT worker_jobs_job_type_check
  CHECK (job_type IN (
    'derive-session-memories',
    'sync-chroma-index',
    'replay-failed-session',
    'build-knowledge-graph',
    'detect-communities',
    'generate-knowledge-report',
    'export-knowledge-snapshot'
  ));
