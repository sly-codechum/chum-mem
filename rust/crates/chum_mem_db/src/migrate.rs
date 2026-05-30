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
                execute_non_transactional_migration(&mut conn, file.contents).await?;
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

async fn execute_non_transactional_migration(
    conn: &mut sqlx::PgConnection,
    contents: &str,
) -> Result<(), sqlx::Error> {
    for statement in split_sql_statements(contents) {
        sqlx::raw_sql(statement).execute(&mut *conn).await?;
    }
    Ok(())
}

fn split_sql_statements(contents: &str) -> Vec<&str> {
    let mut statements = Vec::new();
    let mut state = SqlScanState::Normal;
    let mut start = 0;
    let mut index = 0;

    while index < contents.len() {
        match state {
            SqlScanState::Normal => {
                if contents[index..].starts_with("--") {
                    state = SqlScanState::LineComment;
                    index += 2;
                    continue;
                }
                if contents[index..].starts_with("/*") {
                    state = SqlScanState::BlockComment(1);
                    index += 2;
                    continue;
                }
                if contents[index..].starts_with('\'') {
                    state = SqlScanState::SingleQuoted;
                    index += 1;
                    continue;
                }
                if contents[index..].starts_with('"') {
                    state = SqlScanState::DoubleQuoted;
                    index += 1;
                    continue;
                }
                if let Some(delimiter) = dollar_quote_delimiter_at(contents, index) {
                    state = SqlScanState::DollarQuoted(delimiter);
                    index += delimiter.len();
                    continue;
                }
                if contents[index..].starts_with(';') {
                    let end = index + 1;
                    let statement = contents[start..end].trim();
                    if !statement.is_empty() {
                        statements.push(statement);
                    }
                    start = end;
                    index = end;
                    continue;
                }
            }
            SqlScanState::SingleQuoted => {
                if contents[index..].starts_with("''") {
                    index += 2;
                    continue;
                }
                if contents[index..].starts_with('\'') {
                    state = SqlScanState::Normal;
                    index += 1;
                    continue;
                }
            }
            SqlScanState::DoubleQuoted => {
                if contents[index..].starts_with("\"\"") {
                    index += 2;
                    continue;
                }
                if contents[index..].starts_with('"') {
                    state = SqlScanState::Normal;
                    index += 1;
                    continue;
                }
            }
            SqlScanState::LineComment => {
                if contents[index..].starts_with('\n') {
                    state = SqlScanState::Normal;
                }
            }
            SqlScanState::BlockComment(depth) => {
                if contents[index..].starts_with("/*") {
                    state = SqlScanState::BlockComment(depth + 1);
                    index += 2;
                    continue;
                }
                if contents[index..].starts_with("*/") {
                    state = if depth == 1 {
                        SqlScanState::Normal
                    } else {
                        SqlScanState::BlockComment(depth - 1)
                    };
                    index += 2;
                    continue;
                }
            }
            SqlScanState::DollarQuoted(delimiter) => {
                if contents[index..].starts_with(delimiter) {
                    state = SqlScanState::Normal;
                    index += delimiter.len();
                    continue;
                }
            }
        }

        let Some(ch) = contents[index..].chars().next() else {
            break;
        };
        index += ch.len_utf8();
    }

    let statement = contents[start..].trim();
    if !statement.is_empty() {
        statements.push(statement);
    }

    statements
}

#[derive(Debug, Clone, Copy)]
enum SqlScanState<'a> {
    Normal,
    SingleQuoted,
    DoubleQuoted,
    LineComment,
    BlockComment(usize),
    DollarQuoted(&'a str),
}

fn dollar_quote_delimiter_at(contents: &str, index: usize) -> Option<&str> {
    if !contents[index..].starts_with('$') {
        return None;
    }

    let rest = &contents[index + 1..];
    let tag_end = rest.find('$')?;
    let tag = &rest[..tag_end];
    if !tag
        .chars()
        .all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
    {
        return None;
    }

    Some(&contents[index..index + tag_end + 2])
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

    #[test]
    fn migrations_with_concurrent_index_operations_are_non_transactional() {
        for file in MIGRATION_FILES {
            if file.contents.to_uppercase().contains(" CONCURRENTLY") {
                assert!(
                    !file.transactional,
                    "{} contains CONCURRENTLY and must run outside transactions",
                    file.name
                );
            }
        }
    }

    #[test]
    fn non_transactional_split_keeps_dollar_quoted_blocks_together() {
        let sql = r#"
        CREATE INDEX CONCURRENTLY IF NOT EXISTS example_idx ON public.examples (id);
        DO $$
        BEGIN
          RAISE NOTICE 'semicolon; inside dollar quote';
        END $$;
        CREATE OR REPLACE FUNCTION public.example()
        RETURNS void LANGUAGE plpgsql AS $fn$
        BEGIN
          RAISE NOTICE 'another; semicolon';
        END;
        $fn$;
        "#;

        let statements = split_sql_statements(sql);

        assert_eq!(statements.len(), 3);
        assert!(statements[0].starts_with("CREATE INDEX CONCURRENTLY"));
        assert!(statements[1].starts_with("DO $$"));
        assert!(statements[2].starts_with("CREATE OR REPLACE FUNCTION"));
    }

    #[test]
    fn non_transactional_split_ignores_comment_and_quoted_semicolons() {
        let sql = r#"
        -- comment; with delimiter
        CREATE INDEX CONCURRENTLY IF NOT EXISTS one_idx ON public.examples ("semi;colon");
        /* block; comment */
        CREATE INDEX CONCURRENTLY IF NOT EXISTS two_idx ON public.examples ((lower('x;y')));
        "#;

        let statements = split_sql_statements(sql);

        assert_eq!(statements.len(), 2);
        assert!(statements[0].contains("one_idx"));
        assert!(statements[1].contains("two_idx"));
    }
}
