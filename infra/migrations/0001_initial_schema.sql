create extension if not exists pgcrypto;
create extension if not exists vector;

create schema if not exists app;

create type public.team_role as enum ('owner', 'admin', 'member');
create type public.membership_status as enum ('active', 'invited', 'suspended');
create type public.provider_kind as enum ('claude', 'codex', 'gemini');
create type public.session_status as enum ('active', 'completed', 'failed');
create type public.memory_type as enum (
  'fact',
  'decision',
  'task',
  'bug',
  'summary',
  'implementation_detail',
  'change_log',
  'risk'
);
create type public.memory_edge_type as enum (
  'duplicates',
  'supersedes',
  'caused_by',
  'depends_on',
  'related_to',
  'from_same_session'
);
create type public.actor_type as enum ('user', 'token', 'system');
create type public.audit_action as enum (
  'team.member_added',
  'team.member_updated',
  'project.created',
  'project.updated',
  'token.created',
  'token.revoked',
  'token.used',
  'session.started',
  'session.event_ingested',
  'session.ended',
  'memory.searched',
  'memory.read',
  'context.built'
);

create table if not exists public.app_users (
  id uuid primary key default gen_random_uuid(),
  email text unique,
  display_name text not null,
  status text not null default 'active',
  created_at timestamptz not null default now(),
  check (status in ('active', 'disabled'))
);

create table if not exists public.organizations (
  id uuid primary key default gen_random_uuid(),
  name text not null,
  slug text not null unique,
  created_at timestamptz not null default now()
);

create table if not exists public.teams (
  id uuid primary key default gen_random_uuid(),
  organization_id uuid not null references public.organizations(id) on delete cascade,
  name text not null,
  slug text not null,
  created_at timestamptz not null default now(),
  unique (organization_id, slug),
  unique (id, organization_id)
);

create table if not exists public.team_members (
  id uuid primary key default gen_random_uuid(),
  organization_id uuid not null references public.organizations(id) on delete cascade,
  team_id uuid not null,
  user_id uuid not null references public.app_users(id) on delete cascade,
  role public.team_role not null default 'member',
  status public.membership_status not null default 'active',
  created_at timestamptz not null default now(),
  foreign key (team_id, organization_id) references public.teams(id, organization_id) on delete cascade,
  unique (team_id, user_id)
);

create table if not exists public.projects (
  id uuid primary key default gen_random_uuid(),
  organization_id uuid not null references public.organizations(id) on delete cascade,
  team_id uuid not null,
  name text not null,
  slug text not null,
  repo_url text,
  default_branch text,
  created_at timestamptz not null default now(),
  foreign key (team_id, organization_id) references public.teams(id, organization_id) on delete cascade,
  unique (team_id, slug),
  unique (id, team_id, organization_id)
);

create table if not exists public.api_tokens (
  id uuid primary key default gen_random_uuid(),
  organization_id uuid not null references public.organizations(id) on delete cascade,
  team_id uuid not null,
  project_id uuid,
  user_id uuid not null references public.app_users(id) on delete cascade,
  name text not null,
  token_prefix text not null unique,
  token_hash text not null,
  scopes jsonb not null default '[]'::jsonb,
  last_used_at timestamptz,
  expires_at timestamptz,
  revoked_at timestamptz,
  created_at timestamptz not null default now(),
  foreign key (team_id, organization_id) references public.teams(id, organization_id) on delete cascade,
  foreign key (project_id, team_id, organization_id) references public.projects(id, team_id, organization_id) on delete cascade,
  check (jsonb_typeof(scopes) = 'array')
);

create table if not exists public.sessions (
  id uuid primary key default gen_random_uuid(),
  organization_id uuid not null references public.organizations(id) on delete cascade,
  team_id uuid not null,
  project_id uuid not null,
  user_id uuid references public.app_users(id) on delete set null,
  api_token_id uuid references public.api_tokens(id) on delete set null,
  provider public.provider_kind not null,
  external_session_id text not null,
  repo_url text,
  branch text,
  status public.session_status not null default 'active',
  started_at timestamptz not null default now(),
  ended_at timestamptz,
  metadata jsonb not null default '{}'::jsonb,
  foreign key (team_id, organization_id) references public.teams(id, organization_id) on delete cascade,
  foreign key (project_id, team_id, organization_id) references public.projects(id, team_id, organization_id) on delete cascade,
  unique (project_id, provider, external_session_id)
);

create table if not exists public.session_events (
  id uuid primary key default gen_random_uuid(),
  organization_id uuid not null references public.organizations(id) on delete cascade,
  team_id uuid not null,
  project_id uuid not null,
  session_id uuid not null references public.sessions(id) on delete cascade,
  provider public.provider_kind not null,
  event_type text not null,
  event_time timestamptz not null,
  event_id text not null,
  idempotency_key text not null,
  payload jsonb not null,
  raw_payload jsonb not null,
  created_at timestamptz not null default now(),
  foreign key (team_id, organization_id) references public.teams(id, organization_id) on delete cascade,
  foreign key (project_id, team_id, organization_id) references public.projects(id, team_id, organization_id) on delete cascade,
  unique (session_id, idempotency_key),
  unique (session_id, event_id)
);

create table if not exists public.memories (
  id uuid primary key default gen_random_uuid(),
  organization_id uuid not null references public.organizations(id) on delete cascade,
  team_id uuid not null,
  project_id uuid not null,
  session_id uuid references public.sessions(id) on delete set null,
  type public.memory_type not null,
  title text not null,
  content text not null,
  summary text not null,
  importance_score numeric(5,4) not null default 0.5,
  confidence_score numeric(5,4) not null default 0.5,
  metadata jsonb not null default '{}'::jsonb,
  created_by uuid references public.app_users(id) on delete set null,
  created_at timestamptz not null default now(),
  superseded_at timestamptz,
  search_vector tsvector generated always as (
    setweight(to_tsvector('english', coalesce(title, '')), 'A') ||
    setweight(to_tsvector('english', coalesce(summary, '')), 'B') ||
    setweight(to_tsvector('english', coalesce(content, '')), 'C')
  ) stored,
  foreign key (team_id, organization_id) references public.teams(id, organization_id) on delete cascade,
  foreign key (project_id, team_id, organization_id) references public.projects(id, team_id, organization_id) on delete cascade,
  unique (id, project_id, team_id, organization_id),
  check (importance_score >= 0 and importance_score <= 1),
  check (confidence_score >= 0 and confidence_score <= 1)
);

create table if not exists public.memory_provenance (
  id uuid primary key default gen_random_uuid(),
  organization_id uuid not null references public.organizations(id) on delete cascade,
  team_id uuid not null,
  project_id uuid not null,
  memory_id uuid not null,
  session_event_id uuid not null references public.session_events(id) on delete cascade,
  excerpt text,
  created_at timestamptz not null default now(),
  foreign key (team_id, organization_id) references public.teams(id, organization_id) on delete cascade,
  foreign key (project_id, team_id, organization_id) references public.projects(id, team_id, organization_id) on delete cascade,
  foreign key (memory_id, project_id, team_id, organization_id) references public.memories(id, project_id, team_id, organization_id) on delete cascade,
  unique (memory_id, session_event_id)
);

create table if not exists public.embeddings (
  id uuid primary key default gen_random_uuid(),
  organization_id uuid not null references public.organizations(id) on delete cascade,
  team_id uuid not null,
  project_id uuid not null,
  memory_id uuid not null,
  model text not null,
  embedding vector(1536) not null,
  created_at timestamptz not null default now(),
  foreign key (team_id, organization_id) references public.teams(id, organization_id) on delete cascade,
  foreign key (project_id, team_id, organization_id) references public.projects(id, team_id, organization_id) on delete cascade,
  foreign key (memory_id, project_id, team_id, organization_id) references public.memories(id, project_id, team_id, organization_id) on delete cascade,
  unique (memory_id, model)
);

create table if not exists public.memory_edges (
  id uuid primary key default gen_random_uuid(),
  organization_id uuid not null references public.organizations(id) on delete cascade,
  team_id uuid not null,
  project_id uuid not null,
  from_memory_id uuid not null,
  to_memory_id uuid not null,
  edge_type public.memory_edge_type not null,
  weight numeric(5,4) not null default 0.5,
  created_at timestamptz not null default now(),
  foreign key (team_id, organization_id) references public.teams(id, organization_id) on delete cascade,
  foreign key (project_id, team_id, organization_id) references public.projects(id, team_id, organization_id) on delete cascade,
  foreign key (from_memory_id, project_id, team_id, organization_id) references public.memories(id, project_id, team_id, organization_id) on delete cascade,
  foreign key (to_memory_id, project_id, team_id, organization_id) references public.memories(id, project_id, team_id, organization_id) on delete cascade,
  unique (from_memory_id, to_memory_id, edge_type),
  check (weight >= 0 and weight <= 1)
);

create table if not exists public.context_requests (
  id uuid primary key default gen_random_uuid(),
  organization_id uuid not null references public.organizations(id) on delete cascade,
  team_id uuid not null,
  project_id uuid,
  requester_user_id uuid references public.app_users(id) on delete set null,
  requester_token_id uuid references public.api_tokens(id) on delete set null,
  provider public.provider_kind not null,
  objective text not null,
  token_budget integer not null,
  response_summary text,
  created_at timestamptz not null default now(),
  foreign key (team_id, organization_id) references public.teams(id, organization_id) on delete cascade,
  foreign key (project_id, team_id, organization_id) references public.projects(id, team_id, organization_id) on delete cascade,
  check (token_budget > 0)
);

create table if not exists public.audit_logs (
  id uuid primary key default gen_random_uuid(),
  organization_id uuid references public.organizations(id) on delete cascade,
  team_id uuid,
  project_id uuid,
  actor_type public.actor_type not null,
  actor_id uuid,
  action public.audit_action not null,
  target_type text not null,
  target_id uuid,
  metadata jsonb not null default '{}'::jsonb,
  created_at timestamptz not null default now(),
  foreign key (team_id, organization_id) references public.teams(id, organization_id) on delete cascade,
  foreign key (project_id, team_id, organization_id) references public.projects(id, team_id, organization_id) on delete cascade
);

create index if not exists team_members_user_lookup_idx
  on public.team_members (user_id, status, team_id);

create index if not exists projects_team_lookup_idx
  on public.projects (team_id, created_at desc);

create index if not exists api_tokens_lookup_idx
  on public.api_tokens (token_prefix, team_id, project_id);

create index if not exists api_tokens_active_idx
  on public.api_tokens (team_id, revoked_at, expires_at);

create index if not exists sessions_team_project_idx
  on public.sessions (team_id, project_id, started_at desc);

create index if not exists session_events_session_time_idx
  on public.session_events (session_id, event_time asc);

create index if not exists memories_scope_created_idx
  on public.memories (team_id, project_id, created_at desc);

create index if not exists memories_search_idx
  on public.memories using gin (search_vector);

create index if not exists memory_provenance_memory_idx
  on public.memory_provenance (memory_id, session_event_id);

create index if not exists embeddings_memory_idx
  on public.embeddings (memory_id, model);

create index if not exists embeddings_vector_idx
  on public.embeddings using ivfflat (embedding vector_cosine_ops) with (lists = 100);

create index if not exists memory_edges_scope_idx
  on public.memory_edges (project_id, from_memory_id, edge_type);

create index if not exists context_requests_scope_idx
  on public.context_requests (team_id, project_id, created_at desc);

create index if not exists audit_logs_scope_idx
  on public.audit_logs (team_id, project_id, created_at desc);

create or replace function app.current_organization_id()
returns uuid
language sql
stable
as $$
  select nullif(current_setting('app.current_organization_id', true), '')::uuid
$$;

create or replace function app.current_team_id()
returns uuid
language sql
stable
as $$
  select nullif(current_setting('app.current_team_id', true), '')::uuid
$$;

create or replace function app.current_project_id()
returns uuid
language sql
stable
as $$
  select nullif(current_setting('app.current_project_id', true), '')::uuid
$$;

create or replace function app.current_user_id()
returns uuid
language sql
stable
as $$
  select nullif(current_setting('app.current_user_id', true), '')::uuid
$$;

create or replace function app.current_actor_type()
returns text
language sql
stable
as $$
  select nullif(current_setting('app.current_actor_type', true), '')
$$;

create or replace function app.current_team_role()
returns text
language sql
stable
as $$
  select nullif(current_setting('app.current_team_role', true), '')
$$;

create or replace function app.is_scoped_to_row(row_organization_id uuid, row_team_id uuid, row_project_id uuid default null)
returns boolean
language sql
stable
as $$
  select
    app.current_organization_id() = row_organization_id
    and app.current_team_id() = row_team_id
    and (
      app.current_project_id() is null
      or row_project_id is null
      or app.current_project_id() = row_project_id
    )
$$;

create or replace function app.is_team_admin()
returns boolean
language sql
stable
as $$
  select app.current_team_role() in ('owner', 'admin')
$$;

alter table public.app_users enable row level security;
alter table public.organizations enable row level security;
alter table public.teams enable row level security;
alter table public.team_members enable row level security;
alter table public.projects enable row level security;
alter table public.api_tokens enable row level security;
alter table public.sessions enable row level security;
alter table public.session_events enable row level security;
alter table public.memories enable row level security;
alter table public.memory_provenance enable row level security;
alter table public.embeddings enable row level security;
alter table public.memory_edges enable row level security;
alter table public.context_requests enable row level security;
alter table public.audit_logs enable row level security;

create policy "users scoped by current user"
  on public.app_users
  for select
  using (
    id = app.current_user_id()
    or app.is_team_admin()
  );

create policy "organizations scoped"
  on public.organizations
  for select
  using (id = app.current_organization_id());

create policy "teams scoped"
  on public.teams
  for select
  using (app.is_scoped_to_row(organization_id, id));

create policy "team members scoped read"
  on public.team_members
  for select
  using (app.is_scoped_to_row(organization_id, team_id));

create policy "team members admin mutate"
  on public.team_members
  for all
  using (app.is_scoped_to_row(organization_id, team_id) and app.is_team_admin())
  with check (app.is_scoped_to_row(organization_id, team_id) and app.is_team_admin());

create policy "projects scoped read"
  on public.projects
  for select
  using (app.is_scoped_to_row(organization_id, team_id, id));

create policy "projects admin mutate"
  on public.projects
  for all
  using (app.is_scoped_to_row(organization_id, team_id, id) and app.is_team_admin())
  with check (app.is_scoped_to_row(organization_id, team_id, id) and app.is_team_admin());

create policy "tokens scoped read"
  on public.api_tokens
  for select
  using (app.is_scoped_to_row(organization_id, team_id, project_id));

create policy "tokens admin mutate"
  on public.api_tokens
  for all
  using (app.is_scoped_to_row(organization_id, team_id, project_id) and app.is_team_admin())
  with check (app.is_scoped_to_row(organization_id, team_id, project_id) and app.is_team_admin());

create policy "sessions scoped"
  on public.sessions
  for all
  using (app.is_scoped_to_row(organization_id, team_id, project_id))
  with check (app.is_scoped_to_row(organization_id, team_id, project_id));

create policy "events scoped"
  on public.session_events
  for all
  using (app.is_scoped_to_row(organization_id, team_id, project_id))
  with check (app.is_scoped_to_row(organization_id, team_id, project_id));

create policy "memories scoped"
  on public.memories
  for all
  using (app.is_scoped_to_row(organization_id, team_id, project_id))
  with check (app.is_scoped_to_row(organization_id, team_id, project_id));

create policy "memory provenance scoped"
  on public.memory_provenance
  for all
  using (app.is_scoped_to_row(organization_id, team_id, project_id))
  with check (app.is_scoped_to_row(organization_id, team_id, project_id));

create policy "embeddings scoped"
  on public.embeddings
  for all
  using (app.is_scoped_to_row(organization_id, team_id, project_id))
  with check (app.is_scoped_to_row(organization_id, team_id, project_id));

create policy "memory edges scoped"
  on public.memory_edges
  for all
  using (app.is_scoped_to_row(organization_id, team_id, project_id))
  with check (app.is_scoped_to_row(organization_id, team_id, project_id));

create policy "context requests scoped"
  on public.context_requests
  for all
  using (app.is_scoped_to_row(organization_id, team_id, project_id))
  with check (app.is_scoped_to_row(organization_id, team_id, project_id));

create policy "audit logs scoped"
  on public.audit_logs
  for select
  using (
    organization_id = app.current_organization_id()
    and (
      team_id is null
      or app.is_scoped_to_row(organization_id, team_id, project_id)
    )
  );
