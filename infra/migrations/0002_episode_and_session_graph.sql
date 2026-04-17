create table if not exists public.session_episodes (
  id uuid primary key default gen_random_uuid(),
  organization_id uuid not null references public.organizations(id) on delete cascade,
  team_id uuid not null,
  project_id uuid not null,
  session_id uuid not null references public.sessions(id) on delete cascade,
  episode_ordinal integer not null,
  episode_type text not null,
  title text not null,
  summary text not null,
  started_at timestamptz not null,
  ended_at timestamptz not null,
  metadata jsonb not null default '{}'::jsonb,
  created_at timestamptz not null default now(),
  foreign key (team_id, organization_id) references public.teams(id, organization_id) on delete cascade,
  foreign key (project_id, team_id, organization_id) references public.projects(id, team_id, organization_id) on delete cascade,
  unique (session_id, episode_ordinal),
  unique (id, project_id, team_id, organization_id),
  check (episode_ordinal > 0),
  check (episode_type in ('conversation', 'implementation', 'debugging')),
  check (ended_at >= started_at)
);

alter table public.memories
  add column if not exists episode_id uuid;

do $$
begin
  if not exists (
    select 1
    from information_schema.table_constraints
    where constraint_schema = 'public'
      and table_name = 'memories'
      and constraint_name = 'memories_episode_id_fkey'
  ) then
    alter table public.memories
      add constraint memories_episode_id_fkey
      foreign key (episode_id, project_id, team_id, organization_id)
      references public.session_episodes(id, project_id, team_id, organization_id)
      on delete set null;
  end if;
end
$$;

create index if not exists session_episodes_session_ordinal_idx
  on public.session_episodes (session_id, episode_ordinal asc);

create index if not exists session_episodes_scope_started_idx
  on public.session_episodes (team_id, project_id, started_at desc);

alter table public.session_episodes enable row level security;

create policy "session episodes scoped"
  on public.session_episodes
  for all
  using (app.is_scoped_to_row(organization_id, team_id, project_id))
  with check (app.is_scoped_to_row(organization_id, team_id, project_id));
