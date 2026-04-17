-- 0013_reconcile_claim_state_job.sql
-- Introduce an async `reconcile-claim-state` worker job so session_end no longer
-- holds hundreds of advisory + relation locks inside the writer transaction.
--
-- Migration v2.2.1 (ingestion choke fix): the session_end path previously ran
-- reconcile_claim_memory_state() inline per admitted draft, acquiring one
-- advisory lock per (claim_key, claim_subject) pair and fanning out
-- supersedes/contradicts/confirms edges in the same transaction. For a single
-- import session with hundreds of drafts this blew the `max_locks_per_transaction`
-- budget and produced `deadlock detected` under concurrent session_end.
--
-- The fix moves reconciliation to a dedicated worker job type that processes
-- claims in bounded chunks under a per-project advisory lock.

ALTER TABLE public.worker_jobs DROP CONSTRAINT IF EXISTS worker_jobs_job_type_check;

ALTER TABLE public.worker_jobs ADD CONSTRAINT worker_jobs_job_type_check
  CHECK (job_type IN (
    'derive-session-memories',
    'reconcile-claim-state',
    'sync-chroma-index',
    'replay-failed-session',
    'build-knowledge-graph',
    'detect-communities',
    'generate-knowledge-report',
    'export-knowledge-snapshot'
  ));

-- Prior-candidate SELECT in reconcile_claim_memory_state filters
--   c.project_id = ? AND c.claim_key = ? AND c.admitted = true
-- We already have `idx_claims_project_claim_key` from 0011_typed_claims.sql and
-- `idx_claims_current_state` for admitted/verification/superseded filtering.
-- Add a partial index that makes the hot reconciliation SELECT an index-only
-- lookup over the currently-active admitted claims for a project.
CREATE INDEX IF NOT EXISTS idx_claims_active_admitted_lookup
  ON public.claims (project_id, claim_key, valid_from DESC)
  WHERE admitted = true AND superseded_by IS NULL;

CREATE INDEX IF NOT EXISTS idx_claims_active_admitted_subject
  ON public.claims (project_id, subject, valid_from DESC)
  WHERE admitted = true AND superseded_by IS NULL;
