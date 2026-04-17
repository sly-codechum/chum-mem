-- v2.2.2: Add level and community_path for hierarchical Leiden communities
ALTER TABLE public.knowledge_communities
  ADD COLUMN IF NOT EXISTS level integer NOT NULL DEFAULT 0,
  ADD COLUMN IF NOT EXISTS community_path text;
