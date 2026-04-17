-- Migration 0007: Performance indexes for knowledge graph operations
-- Adds missing indexes on frequently joined/filtered columns

-- memory_edges: lookups by from/to memory IDs (used in edge persistence and graph queries)
CREATE INDEX IF NOT EXISTS idx_memory_edges_from_memory
  ON public.memory_edges(from_memory_id);
CREATE INDEX IF NOT EXISTS idx_memory_edges_to_memory
  ON public.memory_edges(to_memory_id);
CREATE INDEX IF NOT EXISTS idx_memory_edges_project
  ON public.memory_edges(project_id);

-- memories: filtered by project + type in worker and API queries
CREATE INDEX IF NOT EXISTS idx_memories_project_type
  ON public.memories(project_id, type);

-- session_events: ordered scans by session
CREATE INDEX IF NOT EXISTS idx_session_events_session_created
  ON public.session_events(session_id, created_at);

-- memory_provenance: subquery lookups by session event and memory
CREATE INDEX IF NOT EXISTS idx_memory_provenance_session_event
  ON public.memory_provenance(session_event_id);
CREATE INDEX IF NOT EXISTS idx_memory_provenance_memory
  ON public.memory_provenance(memory_id);

-- knowledge_snapshots: ensure fast latest-snapshot lookup (composite already exists, add org+team)
CREATE INDEX IF NOT EXISTS idx_knowledge_snapshots_org_team_project
  ON public.knowledge_snapshots(organization_id, team_id, project_id, created_at DESC);
