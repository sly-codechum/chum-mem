-- Migration 0005: Knowledge graph support
-- Adds evidence labeling, knowledge cache, communities, and graph snapshots

-- Evidence level enum
DO $$ BEGIN
  CREATE TYPE public.evidence_level AS ENUM ('extracted', 'inferred', 'ambiguous');
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

-- Extend memory_edges with evidence classification
ALTER TABLE public.memory_edges
  ADD COLUMN IF NOT EXISTS evidence public.evidence_level NOT NULL DEFAULT 'extracted',
  ADD COLUMN IF NOT EXISTS weight NUMERIC(3,2) NOT NULL DEFAULT 1.0,
  ADD COLUMN IF NOT EXISTS metadata JSONB NOT NULL DEFAULT '{}';

-- Knowledge cache: content-addressed extraction results
CREATE TABLE IF NOT EXISTS public.knowledge_cache (
  content_hash TEXT NOT NULL,
  project_id UUID NOT NULL REFERENCES public.projects(id) ON DELETE CASCADE,
  source_type TEXT NOT NULL,
  source_id UUID NOT NULL,
  extracted_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  result JSONB NOT NULL,
  expires_at TIMESTAMPTZ,
  PRIMARY KEY (content_hash, project_id)
);
CREATE INDEX IF NOT EXISTS idx_knowledge_cache_project
  ON public.knowledge_cache(project_id);
CREATE INDEX IF NOT EXISTS idx_knowledge_cache_expires
  ON public.knowledge_cache(expires_at) WHERE expires_at IS NOT NULL;

-- Community assignments
CREATE TABLE IF NOT EXISTS public.knowledge_communities (
  id SERIAL PRIMARY KEY,
  organization_id UUID NOT NULL,
  team_id UUID NOT NULL,
  project_id UUID NOT NULL REFERENCES public.projects(id) ON DELETE CASCADE,
  community_id INTEGER NOT NULL,
  label TEXT,
  cohesion_score NUMERIC(5,4) NOT NULL DEFAULT 0,
  node_count INTEGER NOT NULL DEFAULT 0,
  representative_nodes JSONB NOT NULL DEFAULT '[]',
  bridge_nodes JSONB NOT NULL DEFAULT '[]',
  computed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  UNIQUE(project_id, community_id)
);

-- Community membership on memories
ALTER TABLE public.memories
  ADD COLUMN IF NOT EXISTS community_id INTEGER;

-- Graph snapshots for persistent exports
CREATE TABLE IF NOT EXISTS public.knowledge_snapshots (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  organization_id UUID NOT NULL,
  team_id UUID NOT NULL,
  project_id UUID NOT NULL REFERENCES public.projects(id) ON DELETE CASCADE,
  snapshot JSONB NOT NULL,
  node_count INTEGER NOT NULL,
  edge_count INTEGER NOT NULL,
  community_count INTEGER NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_knowledge_snapshots_project
  ON public.knowledge_snapshots(project_id, created_at DESC);

-- Add new worker job types
-- Note: The worker_job_type enum needs to be extended
DO $$ BEGIN
  ALTER TYPE public.worker_job_type ADD VALUE IF NOT EXISTS 'build-knowledge-graph';
  ALTER TYPE public.worker_job_type ADD VALUE IF NOT EXISTS 'detect-communities';
  ALTER TYPE public.worker_job_type ADD VALUE IF NOT EXISTS 'generate-knowledge-report';
  ALTER TYPE public.worker_job_type ADD VALUE IF NOT EXISTS 'export-knowledge-snapshot';
EXCEPTION WHEN others THEN NULL;
END $$;

-- RLS policies for new tables
ALTER TABLE public.knowledge_communities ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.knowledge_snapshots ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS knowledge_communities_tenant_isolation ON public.knowledge_communities;
CREATE POLICY knowledge_communities_tenant_isolation ON public.knowledge_communities
  USING (
    organization_id = current_setting('app.current_organization_id', true)::uuid
    AND team_id = current_setting('app.current_team_id', true)::uuid
  );

DROP POLICY IF EXISTS knowledge_snapshots_tenant_isolation ON public.knowledge_snapshots;
CREATE POLICY knowledge_snapshots_tenant_isolation ON public.knowledge_snapshots
  USING (
    organization_id = current_setting('app.current_organization_id', true)::uuid
    AND team_id = current_setting('app.current_team_id', true)::uuid
  );
