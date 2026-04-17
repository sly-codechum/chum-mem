-- Migration 0017: Hash-partition session_events by session_id (8 partitions)
--
-- Why: session_events is the hottest ingestion table. Hash partitioning by
-- session_id distributes writes across partition files, reduces index bloat
-- per partition, and enables partition-pruned queries (all session-scoped
-- queries already filter on session_id).
--
-- Constraints:
--   PostgreSQL requires the partition key to be part of every unique
--   constraint and the primary key. Both existing uniques already contain
--   session_id, so we only need to change the PK from (id) to (id, session_id).
--
--   FKs pointing TO session_events(id) from memory_provenance and claim_proofs
--   cannot target a partitioned table unless the FK includes the partition key.
--   Rather than propagating session_id into those tables, we drop the FKs.
--   Referential integrity is maintained by:
--     - application-level writes (always tied to a valid session_event)
--     - CASCADE from sessions → session_events (deleting a session cascades)
--     - ON DELETE CASCADE from organizations / teams / projects
--
-- Strategy: rename old → create partitioned → migrate data → drop old.
-- This is NOT transactional (DDL + large data copy), marked transactional=false
-- in the Rust migration runner.
--
-- Idempotency: if session_events is already partitioned (e.g. docker-entrypoint
-- already ran this file), the entire migration is skipped.

-- ─── Step 1: Drop FK constraints referencing session_events(id) ───────

-- memory_provenance.session_event_id FK
DO $$
BEGIN
  ALTER TABLE public.memory_provenance
    DROP CONSTRAINT IF EXISTS memory_provenance_session_event_id_fkey;
EXCEPTION WHEN undefined_object THEN
  NULL;
END $$;

-- claim_proofs.session_event_id FK
DO $$
BEGIN
  ALTER TABLE public.claim_proofs
    DROP CONSTRAINT IF EXISTS claim_proofs_session_event_id_fkey;
EXCEPTION WHEN undefined_object THEN
  NULL;
END $$;

-- ─── Steps 2–9: wrapped in a DO block that skips if already partitioned ──

DO $$
BEGIN
  -- Guard: skip if session_events is already a partitioned table
  IF EXISTS (
    SELECT 1 FROM pg_partitioned_table
    WHERE partrelid = 'public.session_events'::regclass
  ) THEN
    RAISE NOTICE 'session_events is already partitioned — skipping 0017 body';
    RETURN;
  END IF;

  -- Step 2: Rename existing table
  ALTER TABLE public.session_events RENAME TO session_events_old;

  EXECUTE 'ALTER INDEX IF EXISTS session_events_pkey RENAME TO session_events_old_pkey';
  EXECUTE 'ALTER INDEX IF EXISTS session_events_session_id_idempotency_key_key RENAME TO session_events_old_idem_key';
  EXECUTE 'ALTER INDEX IF EXISTS session_events_session_id_event_id_key RENAME TO session_events_old_eid_key';
  EXECUTE 'ALTER INDEX IF EXISTS session_events_session_time_idx RENAME TO session_events_old_time_idx';
  EXECUTE 'ALTER INDEX IF EXISTS idx_session_events_session_created RENAME TO session_events_old_created_idx';
  EXECUTE 'ALTER INDEX IF EXISTS session_events_session_turn_event_time_idx RENAME TO session_events_old_turn_idx';

  -- Rename RLS policy
  ALTER POLICY "events scoped" ON public.session_events_old RENAME TO "events scoped old";

  -- Step 3: Create partitioned table
  CREATE TABLE public.session_events (
    id uuid NOT NULL DEFAULT gen_random_uuid(),
    organization_id uuid NOT NULL,
    team_id uuid NOT NULL,
    project_id uuid NOT NULL,
    session_id uuid NOT NULL,
    provider public.provider_kind NOT NULL,
    event_type text NOT NULL,
    event_time timestamptz NOT NULL,
    event_id text NOT NULL,
    idempotency_key text NOT NULL,
    payload jsonb NOT NULL,
    raw_payload jsonb NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    turn_id text,
    PRIMARY KEY (id, session_id),
    FOREIGN KEY (organization_id) REFERENCES public.organizations(id) ON DELETE CASCADE,
    FOREIGN KEY (session_id) REFERENCES public.sessions(id) ON DELETE CASCADE,
    FOREIGN KEY (team_id, organization_id) REFERENCES public.teams(id, organization_id) ON DELETE CASCADE,
    FOREIGN KEY (project_id, team_id, organization_id) REFERENCES public.projects(id, team_id, organization_id) ON DELETE CASCADE,
    CONSTRAINT session_events_session_id_idempotency_key_key
      UNIQUE (session_id, idempotency_key) DEFERRABLE INITIALLY IMMEDIATE,
    CONSTRAINT session_events_session_id_event_id_key
      UNIQUE (session_id, event_id) DEFERRABLE INITIALLY IMMEDIATE
  ) PARTITION BY HASH (session_id);

  -- Step 4: Create 8 hash partitions
  CREATE TABLE public.session_events_p0 PARTITION OF public.session_events
    FOR VALUES WITH (MODULUS 8, REMAINDER 0);
  CREATE TABLE public.session_events_p1 PARTITION OF public.session_events
    FOR VALUES WITH (MODULUS 8, REMAINDER 1);
  CREATE TABLE public.session_events_p2 PARTITION OF public.session_events
    FOR VALUES WITH (MODULUS 8, REMAINDER 2);
  CREATE TABLE public.session_events_p3 PARTITION OF public.session_events
    FOR VALUES WITH (MODULUS 8, REMAINDER 3);
  CREATE TABLE public.session_events_p4 PARTITION OF public.session_events
    FOR VALUES WITH (MODULUS 8, REMAINDER 4);
  CREATE TABLE public.session_events_p5 PARTITION OF public.session_events
    FOR VALUES WITH (MODULUS 8, REMAINDER 5);
  CREATE TABLE public.session_events_p6 PARTITION OF public.session_events
    FOR VALUES WITH (MODULUS 8, REMAINDER 6);
  CREATE TABLE public.session_events_p7 PARTITION OF public.session_events
    FOR VALUES WITH (MODULUS 8, REMAINDER 7);

  -- Step 5: Recreate non-unique indexes
  CREATE INDEX session_events_session_time_idx
    ON public.session_events (session_id, event_time ASC);
  CREATE INDEX idx_session_events_session_created
    ON public.session_events (session_id, created_at);
  CREATE INDEX session_events_session_turn_event_time_idx
    ON public.session_events (session_id, turn_id, event_time)
    WHERE turn_id IS NOT NULL;

  -- Step 6: RLS
  ALTER TABLE public.session_events ENABLE ROW LEVEL SECURITY;
  CREATE POLICY "events scoped"
    ON public.session_events
    FOR ALL
    USING (app.is_scoped_to_row(organization_id, team_id, project_id))
    WITH CHECK (app.is_scoped_to_row(organization_id, team_id, project_id));

  -- Step 7: Migrate data
  INSERT INTO public.session_events (
    id, organization_id, team_id, project_id, session_id, provider,
    event_type, event_time, event_id, idempotency_key, payload,
    raw_payload, created_at, turn_id
  )
  SELECT
    id, organization_id, team_id, project_id, session_id, provider,
    event_type, event_time, event_id, idempotency_key, payload,
    raw_payload, created_at, turn_id
  FROM public.session_events_old;

  -- Step 8: Drop old table
  DROP TABLE public.session_events_old;
END $$;

-- ─── Step 9: Update bulk index management functions ──────────────────
-- (from migration 0016, same definitions — just ensures they target the
-- new partitioned table which they already do by name)

CREATE OR REPLACE FUNCTION public.drop_session_events_bulk_indexes()
RETURNS void LANGUAGE plpgsql AS $$
BEGIN
  DROP INDEX IF EXISTS public.session_events_session_time_idx;
  DROP INDEX IF EXISTS public.idx_session_events_session_created;
  DROP INDEX IF EXISTS public.session_events_session_turn_event_time_idx;
END;
$$;

CREATE OR REPLACE FUNCTION public.create_session_events_bulk_indexes()
RETURNS void LANGUAGE plpgsql AS $$
BEGIN
  CREATE INDEX IF NOT EXISTS session_events_session_time_idx
    ON public.session_events (session_id, event_time ASC);
  CREATE INDEX IF NOT EXISTS idx_session_events_session_created
    ON public.session_events (session_id, created_at);
  CREATE INDEX IF NOT EXISTS session_events_session_turn_event_time_idx
    ON public.session_events (session_id, turn_id, event_time)
    WHERE turn_id IS NOT NULL;
END;
$$;
