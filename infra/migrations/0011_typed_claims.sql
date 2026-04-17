-- 0011_typed_claims.sql
-- Add typed claim, proof, and claim-edge storage for PCKC v2.2.

create table if not exists public.claims (
  id uuid primary key default gen_random_uuid(),
  organization_id uuid not null references public.organizations(id) on delete cascade,
  team_id uuid not null,
  project_id uuid not null,
  memory_id uuid not null,
  session_id uuid references public.sessions(id) on delete set null,
  claim_key text not null,
  claim_type public.memory_type not null,
  subject text not null,
  predicate text not null,
  object text not null,
  claim_polarity text not null default 'positive',
  authority_class text not null,
  verification_status text not null,
  admitted boolean not null default false,
  valid_from timestamptz not null default now(),
  valid_to timestamptz,
  superseded_by uuid,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),
  foreign key (team_id, organization_id) references public.teams(id, organization_id) on delete cascade,
  foreign key (project_id, team_id, organization_id) references public.projects(id, team_id, organization_id) on delete cascade,
  foreign key (memory_id, project_id, team_id, organization_id) references public.memories(id, project_id, team_id, organization_id) on delete cascade,
  unique (memory_id),
  unique (id, project_id, team_id, organization_id),
  check (claim_polarity in ('positive', 'negative', 'neutral')),
  check (authority_class in ('repository', 'user_confirmed', 'tool_verified', 'test_verified', 'session_derived', 'model_derived')),
  check (verification_status in ('verified', 'user_confirmed', 'inferred', 'contradicted', 'unverified'))
);

alter table public.claims
  add constraint claims_superseded_by_fkey
  foreign key (superseded_by) references public.claims(id) on delete set null;

create index if not exists idx_claims_project_claim_key
  on public.claims (project_id, claim_key);

create index if not exists idx_claims_project_subject
  on public.claims (project_id, subject);

create index if not exists idx_claims_current_state
  on public.claims (project_id, admitted, verification_status, superseded_by, valid_to);

create table if not exists public.claim_proofs (
  id uuid primary key default gen_random_uuid(),
  organization_id uuid not null references public.organizations(id) on delete cascade,
  team_id uuid not null,
  project_id uuid not null,
  claim_id uuid not null,
  memory_id uuid not null,
  session_id uuid references public.sessions(id) on delete set null,
  session_event_id uuid references public.session_events(id) on delete cascade,
  proof_type text not null,
  source_ref text not null,
  excerpt text,
  authority_class text,
  verification_status text,
  proof_time timestamptz,
  created_at timestamptz not null default now(),
  foreign key (team_id, organization_id) references public.teams(id, organization_id) on delete cascade,
  foreign key (project_id, team_id, organization_id) references public.projects(id, team_id, organization_id) on delete cascade,
  foreign key (claim_id, project_id, team_id, organization_id) references public.claims(id, project_id, team_id, organization_id) on delete cascade,
  foreign key (memory_id, project_id, team_id, organization_id) references public.memories(id, project_id, team_id, organization_id) on delete cascade,
  unique (claim_id, source_ref),
  check (proof_type in ('repository', 'session_event', 'tool_result', 'test_result', 'user_confirmation', 'summary')),
  check (authority_class is null or authority_class in ('repository', 'user_confirmed', 'tool_verified', 'test_verified', 'session_derived', 'model_derived')),
  check (verification_status is null or verification_status in ('verified', 'user_confirmed', 'inferred', 'contradicted', 'unverified'))
);

create index if not exists idx_claim_proofs_claim
  on public.claim_proofs (claim_id);

create index if not exists idx_claim_proofs_memory
  on public.claim_proofs (memory_id);

create table if not exists public.claim_edges (
  id uuid primary key default gen_random_uuid(),
  organization_id uuid not null references public.organizations(id) on delete cascade,
  team_id uuid not null,
  project_id uuid not null,
  from_claim_id uuid not null,
  to_claim_id uuid not null,
  edge_type public.memory_edge_type not null,
  weight numeric(5,4) not null default 0.5,
  metadata jsonb not null default '{}'::jsonb,
  created_at timestamptz not null default now(),
  foreign key (team_id, organization_id) references public.teams(id, organization_id) on delete cascade,
  foreign key (project_id, team_id, organization_id) references public.projects(id, team_id, organization_id) on delete cascade,
  foreign key (from_claim_id, project_id, team_id, organization_id) references public.claims(id, project_id, team_id, organization_id) on delete cascade,
  foreign key (to_claim_id, project_id, team_id, organization_id) references public.claims(id, project_id, team_id, organization_id) on delete cascade,
  unique (from_claim_id, to_claim_id, edge_type),
  check (weight >= 0 and weight <= 1)
);

create index if not exists idx_claim_edges_from_claim
  on public.claim_edges (from_claim_id, edge_type);

create index if not exists idx_claim_edges_to_claim
  on public.claim_edges (to_claim_id, edge_type);

alter table public.claims enable row level security;
alter table public.claim_proofs enable row level security;
alter table public.claim_edges enable row level security;

create policy "claims scoped"
  on public.claims
  for all
  using (app.is_scoped_to_row(organization_id, team_id, project_id))
  with check (app.is_scoped_to_row(organization_id, team_id, project_id));

create policy "claim proofs scoped"
  on public.claim_proofs
  for all
  using (app.is_scoped_to_row(organization_id, team_id, project_id))
  with check (app.is_scoped_to_row(organization_id, team_id, project_id));

create policy "claim edges scoped"
  on public.claim_edges
  for all
  using (app.is_scoped_to_row(organization_id, team_id, project_id))
  with check (app.is_scoped_to_row(organization_id, team_id, project_id));

insert into public.claims (
  organization_id,
  team_id,
  project_id,
  memory_id,
  session_id,
  claim_key,
  claim_type,
  subject,
  predicate,
  object,
  claim_polarity,
  authority_class,
  verification_status,
  admitted,
  valid_from,
  valid_to
)
select
  m.organization_id,
  m.team_id,
  m.project_id,
  m.id,
  m.session_id,
  coalesce(nullif(m.metadata->>'claimKey', ''), m.type::text || ':' || m.id::text) as claim_key,
  m.type,
  coalesce(
    nullif(split_part(coalesce(nullif(m.metadata->>'claimKey', ''), ''), ':', 2), ''),
    nullif(m.metadata->>'sessionId', ''),
    'global'
  ) as subject,
  coalesce(nullif(m.metadata->>'rankingRole', ''), m.type::text) as predicate,
  coalesce(
    nullif(m.metadata->>'claimObject', ''),
    nullif(m.metadata->>'claimKey', ''),
    m.type::text || ':' || m.id::text
  ) as object,
  case
    when m.metadata->>'claimPolarity' in ('negative', 'neutral', 'positive') then m.metadata->>'claimPolarity'
    else 'positive'
  end as claim_polarity,
  case
    when m.metadata->>'authorityClass' in (
      'repository',
      'user_confirmed',
      'tool_verified',
      'test_verified',
      'session_derived',
      'model_derived'
    ) then m.metadata->>'authorityClass'
    else 'session_derived'
  end as authority_class,
  case
    when m.metadata->>'verificationStatus' in (
      'verified',
      'user_confirmed',
      'inferred',
      'contradicted',
      'unverified'
    ) then m.metadata->>'verificationStatus'
    else 'unverified'
  end as verification_status,
  coalesce((m.metadata->'belief'->>'admit')::boolean, false) as admitted,
  m.created_at as valid_from,
  m.superseded_at as valid_to
from public.memories m
left join public.claims c on c.memory_id = m.id
where c.id is null;

update public.claims claim
set
  superseded_by = superseding.id,
  valid_to = coalesce(claim.valid_to, edge.created_at),
  updated_at = now()
from public.memory_edges edge
join public.claims superseding on superseding.memory_id = edge.from_memory_id
where edge.edge_type = 'supersedes'::public.memory_edge_type
  and claim.memory_id = edge.to_memory_id
  and claim.superseded_by is null;

insert into public.claim_proofs (
  organization_id,
  team_id,
  project_id,
  claim_id,
  memory_id,
  session_id,
  session_event_id,
  proof_type,
  source_ref,
  excerpt,
  authority_class,
  verification_status,
  proof_time
)
select
  c.organization_id,
  c.team_id,
  c.project_id,
  c.id,
  c.memory_id,
  c.session_id,
  mp.session_event_id,
  case
    when m.metadata->>'proofType' in (
      'repository',
      'session_event',
      'tool_result',
      'test_result',
      'user_confirmation',
      'summary'
    ) then m.metadata->>'proofType'
    else 'session_event'
  end as proof_type,
  'session_event:' || mp.session_event_id::text as source_ref,
  mp.excerpt,
  case
    when m.metadata->>'authorityClass' in (
      'repository',
      'user_confirmed',
      'tool_verified',
      'test_verified',
      'session_derived',
      'model_derived'
    ) then m.metadata->>'authorityClass'
    else null
  end as authority_class,
  case
    when m.metadata->>'verificationStatus' in (
      'verified',
      'user_confirmed',
      'inferred',
      'contradicted',
      'unverified'
    ) then m.metadata->>'verificationStatus'
    else null
  end as verification_status,
  se.event_time
from public.claims c
join public.memories m on m.id = c.memory_id
join public.memory_provenance mp on mp.memory_id = c.memory_id
join public.session_events se on se.id = mp.session_event_id
left join public.claim_proofs cp
  on cp.claim_id = c.id
 and cp.source_ref = 'session_event:' || mp.session_event_id::text
where cp.id is null;

insert into public.claim_proofs (
  organization_id,
  team_id,
  project_id,
  claim_id,
  memory_id,
  session_id,
  session_event_id,
  proof_type,
  source_ref,
  excerpt,
  authority_class,
  verification_status,
  proof_time
)
select
  c.organization_id,
  c.team_id,
  c.project_id,
  c.id,
  c.memory_id,
  c.session_id,
  null,
  case
    when m.metadata->>'proofType' in (
      'repository',
      'session_event',
      'tool_result',
      'test_result',
      'user_confirmation',
      'summary'
    ) then m.metadata->>'proofType'
    else 'summary'
  end as proof_type,
  coalesce(nullif(m.metadata->>'claimKey', ''), 'memory:' || m.id::text) as source_ref,
  null,
  case
    when m.metadata->>'authorityClass' in (
      'repository',
      'user_confirmed',
      'tool_verified',
      'test_verified',
      'session_derived',
      'model_derived'
    ) then m.metadata->>'authorityClass'
    else null
  end as authority_class,
  case
    when m.metadata->>'verificationStatus' in (
      'verified',
      'user_confirmed',
      'inferred',
      'contradicted',
      'unverified'
    ) then m.metadata->>'verificationStatus'
    else null
  end as verification_status,
  m.created_at
from public.claims c
join public.memories m on m.id = c.memory_id
left join public.claim_proofs cp on cp.claim_id = c.id
where cp.id is null;

insert into public.claim_edges (
  organization_id,
  team_id,
  project_id,
  from_claim_id,
  to_claim_id,
  edge_type,
  weight,
  metadata,
  created_at
)
select
  edge.organization_id,
  edge.team_id,
  edge.project_id,
  from_claim.id,
  to_claim.id,
  edge.edge_type,
  edge.weight,
  coalesce(edge.metadata, '{}'::jsonb),
  edge.created_at
from public.memory_edges edge
join public.claims from_claim on from_claim.memory_id = edge.from_memory_id
join public.claims to_claim on to_claim.memory_id = edge.to_memory_id
left join public.claim_edges existing
  on existing.from_claim_id = from_claim.id
 and existing.to_claim_id = to_claim.id
 and existing.edge_type = edge.edge_type
where existing.id is null;
