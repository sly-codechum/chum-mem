-- Migration 0016: Bulk ingestion optimizations
--
-- 1. Make session_events unique constraints DEFERRABLE so bulk inserts can
--    defer constraint checking to commit time (avoids per-row index probes).
-- 2. Add helper functions to drop/recreate non-essential indexes on
--    session_events for bulk import windows.
--
-- These changes are backwards-compatible: constraints remain INITIALLY
-- IMMEDIATE so normal INSERT behaviour is unchanged. Only callers that
-- explicitly SET CONSTRAINTS ... DEFERRED see the difference.

-- ─── 1. Deferrable constraints ─────────────────────────────────────────
-- PostgreSQL cannot ALTER a constraint to add DEFERRABLE. We must drop and
-- recreate. The auto-generated names follow the pattern
-- <table>_<columns>_key.

-- (session_id, idempotency_key)
alter table public.session_events
  drop constraint if exists session_events_session_id_idempotency_key_key;
alter table public.session_events
  add constraint session_events_session_id_idempotency_key_key
  unique (session_id, idempotency_key)
  deferrable initially immediate;

-- (session_id, event_id)
alter table public.session_events
  drop constraint if exists session_events_session_id_event_id_key;
alter table public.session_events
  add constraint session_events_session_id_event_id_key
  unique (session_id, event_id)
  deferrable initially immediate;

-- ─── 2. Index management for bulk imports ──────────────────────────────
-- During a bulk import window the caller can drop non-unique indexes,
-- COPY data in, then recreate them. The unique constraint indexes above
-- are kept (needed for ON CONFLICT dedup after staging merge).

create or replace function public.drop_session_events_bulk_indexes()
returns void language plpgsql as $$
begin
  drop index if exists public.session_events_session_time_idx;
  drop index if exists public.idx_session_events_session_created;
  drop index if exists public.session_events_session_turn_event_time_idx;
end;
$$;

create or replace function public.create_session_events_bulk_indexes()
returns void language plpgsql as $$
begin
  create index if not exists session_events_session_time_idx
    on public.session_events (session_id, event_time asc);
  create index if not exists idx_session_events_session_created
    on public.session_events (session_id, created_at);
  create index if not exists session_events_session_turn_event_time_idx
    on public.session_events (session_id, turn_id, event_time)
    where turn_id is not null;
end;
$$;
