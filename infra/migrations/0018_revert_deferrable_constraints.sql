-- Migration 0018: Revert DEFERRABLE constraints on session_events
--
-- PostgreSQL's ON CONFLICT DO NOTHING cannot use DEFERRABLE unique constraints
-- as arbiters — it errors with "ON CONFLICT does not support deferrable unique
-- constraints/exclusion constraints as arbiters". All session_events insert
-- paths (single, batch, and bulk COPY) use ON CONFLICT DO NOTHING for
-- idempotent upserts, so we need non-deferrable constraints.
--
-- The constraint deferral optimization (0016) is dropped. The remaining bulk
-- optimizations (COPY protocol, UNLOGGED staging, partitioning, index
-- management) provide the real throughput gains.

-- (session_id, idempotency_key) — revert to NOT DEFERRABLE
ALTER TABLE public.session_events
  DROP CONSTRAINT IF EXISTS session_events_session_id_idempotency_key_key;
ALTER TABLE public.session_events
  ADD CONSTRAINT session_events_session_id_idempotency_key_key
  UNIQUE (session_id, idempotency_key);

-- (session_id, event_id) — revert to NOT DEFERRABLE
ALTER TABLE public.session_events
  DROP CONSTRAINT IF EXISTS session_events_session_id_event_id_key;
ALTER TABLE public.session_events
  ADD CONSTRAINT session_events_session_id_event_id_key
  UNIQUE (session_id, event_id);
