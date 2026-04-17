create type public.worker_job_status as enum (
  'pending',
  'running',
  'completed',
  'failed',
  'poisoned',
  'cancelled'
);

create type public.session_replay_status as enum (
  'queued',
  'ready',
  'completed',
  'cancelled'
);

create table if not exists public.worker_jobs (
  id uuid primary key default gen_random_uuid(),
  organization_id uuid not null references public.organizations(id) on delete cascade,
  team_id uuid not null,
  project_id uuid not null,
  session_id uuid references public.sessions(id) on delete cascade,
  memory_id uuid references public.memories(id) on delete cascade,
  job_type text not null,
  dedupe_key text not null,
  status public.worker_job_status not null default 'pending',
  priority integer not null default 100,
  attempts integer not null default 0,
  max_attempts integer not null default 3,
  available_at timestamptz not null default now(),
  claimed_at timestamptz,
  completed_at timestamptz,
  worker_id text,
  payload jsonb not null default '{}'::jsonb,
  last_error text,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),
  foreign key (team_id, organization_id) references public.teams(id, organization_id) on delete cascade,
  foreign key (project_id, team_id, organization_id) references public.projects(id, team_id, organization_id) on delete cascade,
  check (job_type in ('sync-chroma-index', 'replay-failed-session')),
  check (priority >= 0),
  check (attempts >= 0),
  check (max_attempts > 0)
);

create unique index if not exists worker_jobs_active_dedupe_idx
  on public.worker_jobs (project_id, job_type, dedupe_key)
  where status in ('pending', 'running');

create index if not exists worker_jobs_claim_idx
  on public.worker_jobs (team_id, project_id, status, available_at asc, priority asc, created_at asc);

create index if not exists worker_jobs_session_idx
  on public.worker_jobs (session_id, created_at desc);

create table if not exists public.worker_job_attempts (
  id uuid primary key default gen_random_uuid(),
  organization_id uuid not null references public.organizations(id) on delete cascade,
  team_id uuid not null,
  project_id uuid not null,
  worker_job_id uuid not null references public.worker_jobs(id) on delete cascade,
  attempt_number integer not null,
  worker_id text,
  outcome text not null,
  error text,
  started_at timestamptz,
  finished_at timestamptz not null default now(),
  created_at timestamptz not null default now(),
  foreign key (team_id, organization_id) references public.teams(id, organization_id) on delete cascade,
  foreign key (project_id, team_id, organization_id) references public.projects(id, team_id, organization_id) on delete cascade,
  unique (worker_job_id, attempt_number),
  check (attempt_number > 0),
  check (outcome in ('completed', 'failed', 'poisoned', 'cancelled')),
  check (started_at is null or finished_at >= started_at)
);

create index if not exists worker_job_attempts_job_idx
  on public.worker_job_attempts (worker_job_id, attempt_number desc);

create table if not exists public.session_replays (
  id uuid primary key default gen_random_uuid(),
  organization_id uuid not null references public.organizations(id) on delete cascade,
  team_id uuid not null,
  project_id uuid not null,
  session_id uuid not null references public.sessions(id) on delete cascade,
  worker_job_id uuid references public.worker_jobs(id) on delete set null,
  status public.session_replay_status not null default 'queued',
  reason text not null,
  metadata jsonb not null default '{}'::jsonb,
  queued_at timestamptz not null default now(),
  prepared_at timestamptz,
  completed_at timestamptz,
  created_at timestamptz not null default now(),
  foreign key (team_id, organization_id) references public.teams(id, organization_id) on delete cascade,
  foreign key (project_id, team_id, organization_id) references public.projects(id, team_id, organization_id) on delete cascade,
  check (prepared_at is null or prepared_at >= queued_at),
  check (completed_at is null or completed_at >= queued_at)
);

create unique index if not exists session_replays_active_session_idx
  on public.session_replays (session_id)
  where status in ('queued', 'ready');

create index if not exists session_replays_scope_idx
  on public.session_replays (team_id, project_id, queued_at desc);

alter table public.worker_jobs enable row level security;
alter table public.worker_job_attempts enable row level security;
alter table public.session_replays enable row level security;

create policy "worker jobs scoped"
  on public.worker_jobs
  for all
  using (app.is_scoped_to_row(organization_id, team_id, project_id))
  with check (app.is_scoped_to_row(organization_id, team_id, project_id));

create policy "worker job attempts scoped"
  on public.worker_job_attempts
  for all
  using (app.is_scoped_to_row(organization_id, team_id, project_id))
  with check (app.is_scoped_to_row(organization_id, team_id, project_id));

create policy "session replays scoped"
  on public.session_replays
  for all
  using (app.is_scoped_to_row(organization_id, team_id, project_id))
  with check (app.is_scoped_to_row(organization_id, team_id, project_id));
