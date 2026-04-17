-- 0009_graph_layer_separation.sql
-- Separate repository graphs from session graphs.
-- Repository graphs contain code structure (AST symbols, imports, calls).
-- Session graphs contain interaction history (events, episodes, memories).

-- Add snapshot_type to knowledge_snapshots
ALTER TABLE public.knowledge_snapshots
  ADD COLUMN IF NOT EXISTS snapshot_type TEXT NOT NULL DEFAULT 'session';

-- Add snapshot_type to knowledge_snapshot_heads (new composite key)
ALTER TABLE public.knowledge_snapshot_heads
  DROP CONSTRAINT IF EXISTS knowledge_snapshot_heads_pkey;

ALTER TABLE public.knowledge_snapshot_heads
  ADD COLUMN IF NOT EXISTS snapshot_type TEXT NOT NULL DEFAULT 'session';

ALTER TABLE public.knowledge_snapshot_heads
  ADD PRIMARY KEY (project_id, organization_id, team_id, snapshot_type);

-- Add snapshot_type to knowledge_snapshot_artifacts
ALTER TABLE public.knowledge_snapshot_artifacts
  ADD COLUMN IF NOT EXISTS snapshot_type TEXT NOT NULL DEFAULT 'session';

-- Index for fast layer-scoped queries
CREATE INDEX IF NOT EXISTS idx_knowledge_snapshots_type
  ON public.knowledge_snapshots (project_id, snapshot_type, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_knowledge_snapshot_artifacts_type
  ON public.knowledge_snapshot_artifacts (project_id, snapshot_type, computed_at DESC);

-- Mark all existing snapshots as 'session' (they were session-merged before)
-- No data migration needed since DEFAULT handles it.

COMMENT ON COLUMN public.knowledge_snapshots.snapshot_type IS
  'Graph layer: "repository" for code structure, "session" for interaction history';
