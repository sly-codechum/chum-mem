-- v2.2.1: turn-graph column on session_events.
--
-- Adds a nullable `turn_id` column so events produced in one model step
-- (Codex `response_item` group, Claude prompt→response chain) can be
-- clustered. Historical rows stay NULL; new ingests populate from the
-- provider-native turn boundary. See docs/research/v2.2.1-pckc/DESIGN.md §3.
--
-- The composite index supports the "events in this turn, ordered"
-- lookup used by the minimal-proof compiler.

alter table public.session_events
  add column if not exists turn_id text;

create index if not exists session_events_session_turn_event_time_idx
  on public.session_events (session_id, turn_id, event_time)
  where turn_id is not null;
