//! PostgreSQL access, migration execution, and readiness checks for the Rust migration.

mod health;
mod migrate;
pub mod reconcile;
mod repos;
mod tenant;

use chum_mem_config::AppConfig;
use chum_mem_contracts::{ActorType, TeamRole};
pub use health::{DependencyReadiness, ReadinessReport, check_readiness};
pub use migrate::{
    EXPECTED_MIGRATION_HEAD, MIGRATION_FILES, MigrationFile, MigrationOutcome, MigrationStatus,
    get_migration_status, require_latest_migration_head, run_migrations,
};
pub use repos::{
    AppendSessionEventParams, AppendedSessionEvent, CandidateSessionRow, ClaimProofInsertParams,
    ClaimProofRow, ClaimRelationRow, ClaimRow, ClaimUpsertParams, DashboardGraphEdgeRow,
    DashboardGraphNodeRow, EpisodeBatchRow, EpisodeRow, MemoryDetailRow, MemoryInsertParams,
    MemoryProvenanceRow, MemorySearchRow, PckcMemoryEdgeRow, QueueSummary, SessionEndResult,
    SessionEventRow, SessionRow, StartedSession, WorkerJobRecord, append_memory_provenance,
    append_memory_provenance_batch, append_memory_provenance_preview,
    bulk_insert_session_events_copy, claim_next_worker_job, complete_worker_job,
    create_session_events_indexes, create_session_replay, drop_session_events_indexes,
    enqueue_worker_job, ensure_scope_entities, fail_worker_job, insert_audit_log, insert_memory,
    insert_session_event, insert_session_events_batch, load_candidate_completed_sessions,
    load_claim_proofs, load_claim_relations_for_memory_ids, load_dashboard_summary,
    load_memories_batch, load_memories_for_chroma, load_memories_for_chroma_scoped, load_memory,
    load_memory_edges_for_ids, load_memory_graph_edges, load_memory_graph_nodes,
    load_memory_provenance, load_memory_search_rows, load_pckc_memory_edges, load_queue_summary,
    load_session_events, load_session_events_limited, load_session_graph_weights,
    mark_claim_superseded, mark_memory_superseded, mark_session_completed,
    mark_session_replay_ready, replace_claim_proofs, resolve_session,
    resolve_session_events_for_candidate, update_claim_verification_status, upsert_claim,
    upsert_claim_edge, upsert_embedding, upsert_ingested_project, upsert_memory_edge,
    upsert_session, upsert_session_edge, upsert_session_episode, upsert_session_episodes_batch,
};
use sqlx::PgPool;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
pub use tenant::apply_repository_context;
use thiserror::Error;
use tracing::info;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct RepositoryContext {
    pub organization_id: Uuid,
    pub team_id: Uuid,
    pub project_id: Option<Uuid>,
    pub actor_id: Option<Uuid>,
    pub actor_type: ActorType,
    pub team_role: TeamRole,
}

impl RepositoryContext {
    pub fn from_config(config: &AppConfig) -> Self {
        Self {
            organization_id: config.organization_id,
            team_id: config.team_id,
            project_id: config.project_id,
            actor_id: config.user_id,
            actor_type: config.actor_type,
            team_role: config.team_role,
        }
    }
}

#[derive(Clone)]
pub struct Database {
    pool: PgPool,
}

impl Database {
    pub async fn connect(config: &AppConfig) -> Result<Self, DbError> {
        let options = config
            .database_url
            .parse::<PgConnectOptions>()
            .map_err(DbError::ConnectOptions)?;
        let pool_future = PgPoolOptions::new()
            .min_connections(config.db_min_connections)
            .max_connections(config.db_max_connections)
            .acquire_timeout(config.db_acquire_timeout())
            .idle_timeout(Some(std::time::Duration::from_secs(30)))
            .max_lifetime(Some(std::time::Duration::from_secs(60 * 30)))
            .connect_with(options);
        let pool: PgPool = tokio::time::timeout(config.db_connect_timeout(), pool_future)
            .await
            .map_err(|_| DbError::Sqlx(sqlx::Error::PoolTimedOut))?
            .map_err(DbError::Sqlx)?;

        Ok(Self { pool })
    }

    pub fn connect_lazy(config: &AppConfig) -> Result<Self, DbError> {
        let options = config
            .database_url
            .parse::<PgConnectOptions>()
            .map_err(DbError::ConnectOptions)?;
        let pool = PgPoolOptions::new()
            .min_connections(0)
            .max_connections(config.db_max_connections.max(1))
            .acquire_timeout(config.db_acquire_timeout())
            .connect_lazy_with(options);

        Ok(Self { pool })
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub async fn migrate_if_enabled(
        &self,
        config: &AppConfig,
    ) -> Result<Option<MigrationOutcome>, DbError> {
        if !config.run_db_migrations {
            return Ok(None);
        }

        let outcome = run_migrations(&self.pool).await?;
        if !outcome.applied.is_empty() {
            info!(applied = ?outcome.applied, "applied Rust-managed database migrations");
        }
        Ok(Some(outcome))
    }
}

#[derive(Debug, Error)]
pub enum DbError {
    #[error("invalid DATABASE_URL: {0}")]
    ConnectOptions(sqlx::Error),
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
    #[error(transparent)]
    Http(#[from] reqwest::Error),
    #[error(transparent)]
    Migration(#[from] migrate::MigrationError),
    #[error("database row not found: {0}")]
    NotFound(&'static str),
}
