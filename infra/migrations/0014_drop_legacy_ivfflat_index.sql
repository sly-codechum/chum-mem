-- 0014_drop_legacy_ivfflat_index.sql
-- Drop the legacy ivfflat embeddings index, which was declared fallback in
-- 0008_latency_online_path.sql (comment: "The existing ivfflat index
-- (embeddings_vector_idx) is kept during migration as fallback. HNSW provides
-- better recall at low latency without requiring periodic VACUUM/re-clustering.").
--
-- Keeping both indexes doubles write amplification on every embeddings insert
-- (session_end hot path) for no retrieval benefit: perform_search uses the
-- HNSW index (embeddings_hnsw_idx) added in 0008. EXPLAIN on the semantic
-- shortlist query confirms the planner picks the HNSW index once ivfflat is
-- dropped.
--
-- Note: DROP INDEX CONCURRENTLY cannot run inside a transaction block, so this
-- migration is registered as transactional=false in the Rust migration registry.

DROP INDEX CONCURRENTLY IF EXISTS public.embeddings_vector_idx;
