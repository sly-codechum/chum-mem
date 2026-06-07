pub mod ast_parser;
mod chroma;
mod compile;
mod context;
mod derivation;
mod jobs;
mod knowledge;
pub mod leiden;
mod ranking;
pub mod reconcile;
mod repository;
mod turbovec_store;
mod vector_store;

pub use chroma::{
    CHROMA_EMBEDDING_DIMENSIONS, ChromaQueryResult, UpsertMemory, effective_chroma_collection_name,
    query_chroma_memories, query_chroma_memories_typed, typed_collection_name,
    upsert_chroma_memories, upsert_chroma_memories_typed,
};
pub use compile::compile_minimal_proof_set;
pub use context::build_context_pack;
pub use derivation::{
    DerivedMemoryDraft, SessionEpisodeDraft, SessionEventRecord, SessionRelationshipScore,
    SessionSimilaritySignals, derive_memories_from_session, derive_session_episodes, embed_text,
    event_text, extract_session_signals, score_session_relationship,
};
pub use jobs::{SessionCompletionJobPlan, build_session_completion_job_plan};
pub use knowledge::{
    CommunityInfo, EvidenceDistribution, GraphProjection, GraphQueryResponse, GraphStatistics,
    KnowledgeEdge, KnowledgeGraph, KnowledgeNode, MemoryNodeInput, assign_communities_with_budget,
    build_knowledge_graph, community_relevance_from_query, generate_knowledge_report,
    memory_community_map, merge_graphs, project_graph_for_dashboard, run_knowledge_query,
    to_node_link_json, to_persistable_memory_edge,
};
pub use ranking::{
    MemorySearchEnvelope, ProgressiveDisclosureResult, RankedMemory, RankingContext, SearchMetrics,
    SemanticQueryResult, dedupe_hits_by_provenance, merge_hybrid_results, progressive_disclosure,
    rank_hybrid_results,
};
pub use repository::{
    RepositoryBuildArtifacts, RepositoryBuildOptions, RepositoryBuildResult, RepositoryFilePayload,
    SyncRules, build_repository_knowledge, parse_file_batch, parse_file_payload_batch, sync_rules,
};
pub use turbovec_store::{TurboVecScope, TurboVecStore};
pub use vector_store::{
    DeleteOptions, ScopeOptions, SearchOptions, VECTOR_EMBEDDING_DIMENSIONS, VectorSearchResult,
    VectorStore, VectorStoreError, VectorStoreFuture, VectorStoreItem, vector_from_f64,
};
