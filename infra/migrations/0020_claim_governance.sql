-- 0020_claim_governance.sql
-- v2.2.3: Deterministic durable memory governance.
--
-- Adds governance_state to claims (active/pinned/archived/rejected) and
-- an append-only governance_history table for auditability.

ALTER TABLE public.claims
  ADD COLUMN IF NOT EXISTS governance_state text NOT NULL DEFAULT 'active'
  CHECK (governance_state IN ('active', 'pinned', 'archived', 'rejected'));

CREATE INDEX IF NOT EXISTS idx_claims_governance_state
  ON public.claims (project_id, governance_state);

CREATE TABLE IF NOT EXISTS public.claim_governance_history (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  organization_id uuid NOT NULL REFERENCES public.organizations(id) ON DELETE CASCADE,
  team_id uuid NOT NULL,
  project_id uuid NOT NULL,
  claim_id uuid NOT NULL,
  previous_state text NOT NULL,
  new_state text NOT NULL,
  reason text,
  actor_type text NOT NULL DEFAULT 'system',
  actor_id uuid,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (team_id, organization_id) REFERENCES public.teams(id, organization_id) ON DELETE CASCADE,
  FOREIGN KEY (project_id, team_id, organization_id) REFERENCES public.projects(id, team_id, organization_id) ON DELETE CASCADE,
  FOREIGN KEY (claim_id, project_id, team_id, organization_id) REFERENCES public.claims(id, project_id, team_id, organization_id) ON DELETE CASCADE,
  CHECK (previous_state IN ('active', 'pinned', 'archived', 'rejected')),
  CHECK (new_state IN ('active', 'pinned', 'archived', 'rejected')),
  CHECK (actor_type IN ('user', 'token', 'system'))
);

CREATE INDEX IF NOT EXISTS idx_claim_governance_history_claim
  ON public.claim_governance_history (claim_id, created_at DESC);

ALTER TABLE public.claim_governance_history ENABLE ROW LEVEL SECURITY;

CREATE POLICY "claim governance history scoped"
  ON public.claim_governance_history
  FOR ALL
  USING (app.is_scoped_to_row(organization_id, team_id, project_id))
  WITH CHECK (app.is_scoped_to_row(organization_id, team_id, project_id));
