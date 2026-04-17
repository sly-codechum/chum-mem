create table if not exists public.session_edges (
  id uuid primary key default gen_random_uuid(),
  organization_id uuid not null references public.organizations(id) on delete cascade,
  team_id uuid not null,
  project_id uuid not null,
  from_session_id uuid not null references public.sessions(id) on delete cascade,
  to_session_id uuid not null references public.sessions(id) on delete cascade,
  edge_type text not null,
  weight numeric(5,4) not null default 0.5,
  metadata jsonb not null default '{}'::jsonb,
  created_at timestamptz not null default now(),
  foreign key (team_id, organization_id) references public.teams(id, organization_id) on delete cascade,
  foreign key (project_id, team_id, organization_id) references public.projects(id, team_id, organization_id) on delete cascade,
  unique (from_session_id, to_session_id, edge_type),
  check (weight >= 0 and weight <= 1),
  check (from_session_id <> to_session_id),
  check (edge_type in ('related_to', 'same_branch'))
);

create index if not exists session_edges_scope_from_idx
  on public.session_edges (project_id, from_session_id, weight desc);

create index if not exists session_edges_scope_to_idx
  on public.session_edges (project_id, to_session_id, weight desc);

alter table public.session_edges enable row level security;

create policy "session edges scoped"
  on public.session_edges
  for all
  using (app.is_scoped_to_row(organization_id, team_id, project_id))
  with check (app.is_scoped_to_row(organization_id, team_id, project_id));
