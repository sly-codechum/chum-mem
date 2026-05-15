use sha2::{Digest, Sha256};
use sqlx::Acquire;
use sqlx::Executor;
use sqlx::PgPool;
use thiserror::Error;

const MIGRATION_LOCK_KEY: i64 = 42_424_201;
pub const EXPECTED_MIGRATION_HEAD: &str = "0022_open_provider_identity.sql";

#[derive(Debug, Clone, Copy)]
pub struct MigrationFile {
    pub name: &'static str,
    pub contents: &'static str,
    pub sentinel: Option<&'static str>,
    pub transactional: bool,
}

pub const MIGRATION_FILES: &[MigrationFile] = &[
    MigrationFile {
        name: "0001_initial_schema.sql",
        contents: include_str!("../../../../infra/migrations/0001_initial_schema.sql"),
        sentinel: Some("public.memories"),
        transactional: true,
    },
    MigrationFile {
        name: "0002_episode_and_session_graph.sql",
        contents: include_str!("../../../../infra/migrations/0002_episode_and_session_graph.sql"),
        sentinel: Some("public.session_episodes"),
        transactional: true,
    },
    MigrationFile {
        name: "0003_session_edges.sql",
        contents: include_str!("../../../../infra/migrations/0003_session_edges.sql"),
        sentinel: Some("public.session_edges"),
        transactional: true,
    },
    MigrationFile {
        name: "0004_queue_and_replay.sql",
        contents: include_str!("../../../../infra/migrations/0004_queue_and_replay.sql"),
        sentinel: Some("public.worker_jobs"),
        transactional: true,
    },
    MigrationFile {
        name: "0005_knowledge_graph.sql",
        contents: include_str!("../../../../infra/migrations/0005_knowledge_graph.sql"),
        sentinel: Some("public.knowledge_communities"),
        transactional: true,
    },
    MigrationFile {
        name: "0006_fix_worker_job_type_constraint.sql",
        contents: include_str!(
            "../../../../infra/migrations/0006_fix_worker_job_type_constraint.sql"
        ),
        sentinel: None,
        transactional: true,
    },
    MigrationFile {
        name: "0007_performance_indexes.sql",
        contents: include_str!("../../../../infra/migrations/0007_performance_indexes.sql"),
        sentinel: None,
        transactional: true,
    },
    MigrationFile {
        name: "0008_latency_online_path.sql",
        contents: include_str!("../../../../infra/migrations/0008_latency_online_path.sql"),
        sentinel: Some("public.knowledge_snapshot_heads"),
        transactional: false,
    },
    MigrationFile {
        name: "0009_graph_layer_separation.sql",
        contents: include_str!("../../../../infra/migrations/0009_graph_layer_separation.sql"),
        sentinel: None,
        transactional: true,
    },
    MigrationFile {
        name: "0010_pckc_claims.sql",
        contents: include_str!("../../../../infra/migrations/0010_pckc_claims.sql"),
        sentinel: None,
        transactional: true,
    },
    MigrationFile {
        name: "0011_typed_claims.sql",
        contents: include_str!("../../../../infra/migrations/0011_typed_claims.sql"),
        sentinel: Some("public.claims"),
        transactional: true,
    },
    MigrationFile {
        name: "0012_derive_session_memories_job_type.sql",
        contents: include_str!(
            "../../../../infra/migrations/0012_derive_session_memories_job_type.sql"
        ),
        sentinel: None,
        transactional: true,
    },
    MigrationFile {
        name: "0013_reconcile_claim_state_job.sql",
        contents: include_str!("../../../../infra/migrations/0013_reconcile_claim_state_job.sql"),
        sentinel: None,
        transactional: true,
    },
    MigrationFile {
        name: "0014_drop_legacy_ivfflat_index.sql",
        contents: include_str!("../../../../infra/migrations/0014_drop_legacy_ivfflat_index.sql"),
        sentinel: None,
        // DROP INDEX CONCURRENTLY cannot run inside a transaction block.
        transactional: false,
    },
    MigrationFile {
        name: "0015_session_events_turn_id.sql",
        contents: include_str!("../../../../infra/migrations/0015_session_events_turn_id.sql"),
        sentinel: None,
        transactional: true,
    },
    MigrationFile {
        name: "0016_bulk_ingest_optimizations.sql",
        contents: include_str!("../../../../infra/migrations/0016_bulk_ingest_optimizations.sql"),
        sentinel: None,
        transactional: true,
    },
    MigrationFile {
        name: "0017_partition_session_events.sql",
        contents: include_str!("../../../../infra/migrations/0017_partition_session_events.sql"),
        sentinel: None,
        // DDL + large data copy cannot run in a single transaction.
        transactional: false,
    },
    MigrationFile {
        name: "0018_revert_deferrable_constraints.sql",
        contents: include_str!(
            "../../../../infra/migrations/0018_revert_deferrable_constraints.sql"
        ),
        sentinel: None,
        transactional: true,
    },
    MigrationFile {
        name: "0019_community_hierarchy.sql",
        contents: include_str!("../../../../infra/migrations/0019_community_hierarchy.sql"),
        sentinel: None,
        transactional: true,
    },
    MigrationFile {
        name: "0020_claim_governance.sql",
        contents: include_str!("../../../../infra/migrations/0020_claim_governance.sql"),
        sentinel: Some("public.claim_governance_history"),
        transactional: true,
    },
    MigrationFile {
        name: "0021_project_id_repository_identity.sql",
        contents: include_str!(
            "../../../../infra/migrations/0021_project_id_repository_identity.sql"
        ),
        sentinel: None,
        transactional: true,
    },
    MigrationFile {
        name: "0022_open_provider_identity.sql",
        contents: include_str!("../../../../infra/migrations/0022_open_provider_identity.sql"),
        sentinel: None,
        transactional: true,
    },
];

#[derive(Debug, Clone, Default)]
pub struct MigrationOutcome {
    pub applied: Vec<&'static str>,
    pub skipped: Vec<&'static str>,
}

#[derive(Debug, Clone)]
pub struct MigrationStatus {
    pub applied: Vec<String>,
    pub pending: Vec<&'static str>,
}

#[derive(Debug, Error)]
pub enum MigrationError {
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
    #[error("migration checksum mismatch for {name}")]
    ChecksumMismatch { name: &'static str },
    #[error("database is missing migration {name}")]
    MissingMigrationHead { name: &'static str },
}

pub async fn run_migrations(pool: &PgPool) -> Result<MigrationOutcome, MigrationError> {
    let mut conn = pool.acquire().await?;
    sqlx::query("select pg_advisory_lock($1)")
        .bind(MIGRATION_LOCK_KEY)
        .execute(&mut *conn)
        .await?;

    let result = async {
        ensure_schema_migrations_table(&mut *conn).await?;

        let mut outcome = MigrationOutcome::default();
        for file in MIGRATION_FILES {
            let checksum = checksum(file.contents);
            let existing = sqlx::query_scalar::<_, String>(
                "select checksum from public.schema_migrations where name = $1 limit 1",
            )
            .bind(file.name)
            .fetch_optional(&mut *conn)
            .await?;

            if let Some(existing_checksum) = existing {
                if existing_checksum != checksum {
                    return Err(MigrationError::ChecksumMismatch { name: file.name });
                }
                outcome.skipped.push(file.name);
                continue;
            }

            if migration_already_materialized(&mut *conn, file.sentinel).await? {
                record_applied_migration(&mut *conn, file.name, &checksum).await?;
                outcome.skipped.push(file.name);
                continue;
            }

            if file.transactional {
                let mut tx = conn.begin().await?;
                sqlx::raw_sql(file.contents).execute(&mut *tx).await?;
                record_applied_migration(&mut *tx, file.name, &checksum).await?;
                tx.commit().await?;
            } else {
                sqlx::raw_sql(file.contents).execute(&mut *conn).await?;
                record_applied_migration(&mut *conn, file.name, &checksum).await?;
            }

            outcome.applied.push(file.name);
        }

        Ok::<MigrationOutcome, MigrationError>(outcome)
    }
    .await;

    let unlock = sqlx::query("select pg_advisory_unlock($1)")
        .bind(MIGRATION_LOCK_KEY)
        .execute(&mut *conn)
        .await;

    let outcome = result?;
    unlock?;

    Ok(outcome)
}

pub async fn get_migration_status(pool: &PgPool) -> Result<MigrationStatus, MigrationError> {
    let mut conn = pool.acquire().await?;
    ensure_schema_migrations_table(&mut *conn).await?;

    let applied = sqlx::query_scalar::<_, String>(
        "select name from public.schema_migrations order by name asc",
    )
    .fetch_all(&mut *conn)
    .await?;

    let pending = MIGRATION_FILES
        .iter()
        .filter(|file| !applied.iter().any(|name| name == file.name))
        .map(|file| file.name)
        .collect::<Vec<_>>();

    Ok(MigrationStatus { applied, pending })
}

pub async fn require_latest_migration_head(pool: &PgPool) -> Result<(), MigrationError> {
    let status = get_migration_status(pool).await?;
    let has_expected_head = status
        .applied
        .iter()
        .any(|name| name == EXPECTED_MIGRATION_HEAD);
    if has_expected_head {
        return Ok(());
    }

    Err(MigrationError::MissingMigrationHead {
        name: EXPECTED_MIGRATION_HEAD,
    })
}

async fn ensure_schema_migrations_table<'e, E>(executor: E) -> Result<(), sqlx::Error>
where
    E: Executor<'e, Database = sqlx::Postgres>,
{
    sqlx::query(
        r#"
        create table if not exists public.schema_migrations (
          name text primary key,
          checksum text not null,
          applied_at timestamptz not null default now()
        )
        "#,
    )
    .execute(executor)
    .await?;
    Ok(())
}

async fn record_applied_migration<'e, E>(
    executor: E,
    name: &'static str,
    checksum: &str,
) -> Result<(), sqlx::Error>
where
    E: Executor<'e, Database = sqlx::Postgres>,
{
    sqlx::query("insert into public.schema_migrations (name, checksum) values ($1, $2)")
        .bind(name)
        .bind(checksum)
        .execute(executor)
        .await?;
    Ok(())
}

async fn migration_already_materialized<'e, E>(
    executor: E,
    sentinel: Option<&'static str>,
) -> Result<bool, sqlx::Error>
where
    E: Executor<'e, Database = sqlx::Postgres>,
{
    let Some(sentinel) = sentinel else {
        return Ok(false);
    };

    let Some((schema, table)) = sentinel.split_once('.') else {
        return Ok(false);
    };

    let exists = sqlx::query_scalar::<_, bool>(
        r#"
        select exists (
          select 1
          from information_schema.tables
          where table_schema = $1
            and table_name = $2
        )
        "#,
    )
    .bind(schema)
    .bind(table)
    .fetch_one(executor)
    .await?;

    Ok(exists)
}

fn checksum(contents: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(contents.as_bytes());
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migration_registry_includes_latency_head() {
        assert_eq!(
            MIGRATION_FILES.last().map(|file| file.name),
            Some(EXPECTED_MIGRATION_HEAD)
        );
    }

    #[test]
    fn migration_registry_matches_infra_migrations() {
        let migrations_dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../infra/migrations");
        let mut disk_migrations = std::fs::read_dir(migrations_dir)
            .expect("read migrations directory")
            .map(|entry| {
                entry
                    .expect("read migration entry")
                    .file_name()
                    .into_string()
                    .expect("migration file name is utf-8")
            })
            .filter(|name| name.ends_with(".sql"))
            .collect::<Vec<_>>();
        disk_migrations.sort();

        let registry_migrations = MIGRATION_FILES
            .iter()
            .map(|file| file.name.to_string())
            .collect::<Vec<_>>();

        assert_eq!(registry_migrations, disk_migrations);
    }
}
