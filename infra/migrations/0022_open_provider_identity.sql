-- Session provider identity is an open AI client identifier.
--
-- Provider remains useful metadata and an optional retrieval filter, but it
-- must not be a closed database enum that rejects new clients.

alter table if exists public.sessions
  alter column provider type text using provider::text;

alter table if exists public.session_events
  alter column provider type text using provider::text;

alter table if exists public.context_requests
  alter column provider type text using provider::text;

do $$
begin
  if not exists (
    select 1
    from pg_constraint
    where conrelid = 'public.sessions'::regclass
      and conname = 'sessions_provider_identifier_check'
  ) then
    alter table public.sessions
      add constraint sessions_provider_identifier_check
      check (provider ~ '^[a-z0-9][a-z0-9._-]{0,63}$');
  end if;
end $$;

do $$
begin
  if to_regclass('public.context_requests') is not null and not exists (
    select 1
    from pg_constraint
    where conrelid = 'public.context_requests'::regclass
      and conname = 'context_requests_provider_identifier_check'
  ) then
    alter table public.context_requests
      add constraint context_requests_provider_identifier_check
      check (provider ~ '^[a-z0-9][a-z0-9._-]{0,63}$');
  end if;
end $$;

do $$
begin
  if not exists (
    select 1
    from pg_constraint
    where conrelid = 'public.session_events'::regclass
      and conname = 'session_events_provider_identifier_check'
  ) then
    alter table public.session_events
      add constraint session_events_provider_identifier_check
      check (provider ~ '^[a-z0-9][a-z0-9._-]{0,63}$');
  end if;
end $$;

do $$
begin
  drop type if exists public.provider_kind;
exception
  when dependent_objects_still_exist then
    raise notice 'provider_kind still has dependencies; leaving enum type in place';
end $$;
