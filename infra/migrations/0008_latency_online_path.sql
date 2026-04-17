-- Migration 0008: Online path latency optimizations
-- Adds HNSW index for ANN shortlist, btree support indexes for filter pushdown,
-- knowledge snapshot heads, and knowledge snapshot artifacts.

-- Phase 2: HNSW index on embeddings for approximate nearest-neighbor shortlist
-- The existing ivfflat index (embeddings_vector_idx) is kept during migration as fallback.
-- HNSW provides better recall at low latency without requiring periodic VACUUM/re-clustering.
CREATE INDEX CONCURRENTLY IF NOT EXISTS embeddings_hnsw_idx
  ON public.embeddings USING hnsw (embedding vector_cosine_ops)
  WITH (m = 16, ef_construction = 64);

-- Phase 2: Btree support indexes for vector shortlist filter pushdown
CREATE INDEX CONCURRENTLY IF NOT EXISTS embeddings_project_model_idx
  ON public.embeddings (project_id, model, memory_id);

CREATE INDEX CONCURRENTLY IF NOT EXISTS memories_project_created_idx
  ON public.memories (project_id, created_at DESC);

-- Phase 3: Knowledge snapshot heads — pointer to the latest snapshot per project
CREATE TABLE IF NOT EXISTS public.knowledge_snapshot_heads (
  project_id UUID NOT NULL REFERENCES public.projects(id) ON DELETE CASCADE,
  organization_id UUID NOT NULL,
  team_id UUID NOT NULL,
  snapshot_id UUID NOT NULL REFERENCES public.knowledge_snapshots(id) ON DELETE CASCADE,
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  PRIMARY KEY (project_id, organization_id, team_id)
);

ALTER TABLE public.knowledge_snapshot_heads ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS knowledge_snapshot_heads_tenant_isolation ON public.knowledge_snapshot_heads;
CREATE POLICY knowledge_snapshot_heads_tenant_isolation ON public.knowledge_snapshot_heads
  USING (
    organization_id = current_setting('app.current_organization_id', true)::uuid
    AND team_id = current_setting('app.current_team_id', true)::uuid
  );

-- Phase 3: Knowledge snapshot artifacts — precomputed read models
CREATE TABLE IF NOT EXISTS public.knowledge_snapshot_artifacts (
  snapshot_id UUID NOT NULL REFERENCES public.knowledge_snapshots(id) ON DELETE CASCADE,
  organization_id UUID NOT NULL,
  team_id UUID NOT NULL,
  project_id UUID NOT NULL REFERENCES public.projects(id) ON DELETE CASCADE,
  report_markdown TEXT,
  node_link_json TEXT,
  node_count INTEGER NOT NULL DEFAULT 0,
  edge_count INTEGER NOT NULL DEFAULT 0,
  community_count INTEGER NOT NULL DEFAULT 0,
  computed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  PRIMARY KEY (snapshot_id)
);

CREATE INDEX IF NOT EXISTS idx_snapshot_artifacts_project
  ON public.knowledge_snapshot_artifacts (project_id, computed_at DESC);

ALTER TABLE public.knowledge_snapshot_artifacts ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS knowledge_snapshot_artifacts_tenant_isolation ON public.knowledge_snapshot_artifacts;
CREATE POLICY knowledge_snapshot_artifacts_tenant_isolation ON public.knowledge_snapshot_artifacts
  USING (
    organization_id = current_setting('app.current_organization_id', true)::uuid
    AND team_id = current_setting('app.current_team_id', true)::uuid
  );

-- Phase 3: Memory provenance preview — compact precomputed summary for search results
CREATE TABLE IF NOT EXISTS public.memory_provenance_preview (
  memory_id UUID NOT NULL,
  organization_id UUID NOT NULL,
  team_id UUID NOT NULL,
  project_id UUID NOT NULL,
  session_id UUID,
  session_event_id UUID,
  excerpt TEXT,
  computed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  PRIMARY KEY (memory_id),
  FOREIGN KEY (memory_id, project_id, team_id, organization_id)
    REFERENCES public.memories(id, project_id, team_id, organization_id) ON DELETE CASCADE
);

ALTER TABLE public.memory_provenance_preview ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS memory_provenance_preview_tenant_isolation ON public.memory_provenance_preview;
CREATE POLICY memory_provenance_preview_tenant_isolation ON public.memory_provenance_preview
  USING (
    organization_id = current_setting('app.current_organization_id', true)::uuid
    AND team_id = current_setting('app.current_team_id', true)::uuid
  );
