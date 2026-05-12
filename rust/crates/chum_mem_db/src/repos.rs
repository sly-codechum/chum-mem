use chum_mem_contracts::StartSessionRequest;
use serde_json::{Value, json};
use sqlx::FromRow;
use sqlx::Transaction;
use sqlx::{Postgres, Row};
use uuid::Uuid;

use crate::{DbError, RepositoryContext};

#[derive(Debug, Clone, FromRow)]
pub struct StartedSession {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub team_id: Uuid,
    pub project_id: Uuid,
    pub status: String,
}

#[derive(Debug, Clone, FromRow)]
pub struct SessionRow {
    pub id: Uuid,
    pub provider: String,
    pub project_id: Uuid,
    pub external_session_id: String,
    pub status: String,
    pub branch: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AppendSessionEventParams {
    pub session_id: Uuid,
    pub project_id: Uuid,
    pub provider: String,
    pub event_type: String,
    pub event_time: String,
    pub event_id: String,
    pub idempotency_key: String,
    pub payload: Value,
    pub raw_payload: Value,
    /// v2.2.1: turn-graph identifier. Nullable for historical rows and
    /// providers that don't emit turn boundaries (e.g. Gemini).
    pub turn_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AppendedSessionEvent {
    pub event_id: Uuid,
    pub duplicate: bool,
}

#[derive(Debug, Clone, FromRow)]
pub struct SessionEventRow {
    pub id: Uuid,
    pub event_type: String,
    pub payload: Value,
    pub created_at: time::OffsetDateTime,
}

#[derive(Debug, Clone, FromRow)]
pub struct EpisodeRow {
    pub id: Uuid,
    pub episode_ordinal: i32,
}

#[derive(Debug, Clone)]
pub struct MemoryInsertParams {
    pub session_id: Uuid,
    pub episode_id: Option<Uuid>,
    pub memory_type: String,
    pub title: String,
    pub content: String,
    pub summary: String,
    pub importance_score: f64,
    pub confidence_score: f64,
    pub metadata: Value,
}

#[derive(Debug, Clone, FromRow)]
pub struct SessionEndResult {
    pub id: Uuid,
    pub project_id: Uuid,
    pub status: String,
}

#[derive(Debug, Clone, FromRow)]
pub struct CandidateSessionRow {
    pub id: Uuid,
    pub branch: Option<String>,
}

#[derive(Debug, Clone, FromRow)]
pub struct MemorySearchRow {
    pub id: Uuid,
    pub project_id: Uuid,
    pub memory_type: String,
    pub title: String,
    pub content: String,
    pub summary: String,
    pub metadata: Value,
    pub session_id: Option<Uuid>,
    pub importance_score: f64,
    pub confidence_score: f64,
    pub superseded_at: Option<time::OffsetDateTime>,
    pub created_at: time::OffsetDateTime,
    pub branch: Option<String>,
    pub lexical_score: Option<f64>,
    pub semantic_score: Option<f64>,
    pub claim_id: Option<Uuid>,
    pub claim_type: Option<String>,
    pub claim_key: Option<String>,
    pub claim_subject: Option<String>,
    pub claim_predicate: Option<String>,
    pub claim_object: Option<String>,
    pub claim_polarity: Option<String>,
    pub claim_authority_class: Option<String>,
    pub claim_verification_status: Option<String>,
    pub claim_valid_from: Option<time::OffsetDateTime>,
    pub claim_valid_to: Option<time::OffsetDateTime>,
    pub claim_superseded_by: Option<Uuid>,
    pub active_conflict_count: i64,
    /// v2.2.3: Governance state from claims table.
    pub claim_governance_state: Option<String>,
}

#[derive(Debug, Clone, FromRow)]
pub struct MemoryProvenanceRow {
    pub memory_id: Uuid,
    pub session_id: Uuid,
    pub session_event_id: Uuid,
    pub excerpt: Option<String>,
}

#[derive(Debug, Clone, FromRow)]
pub struct MemoryDetailRow {
    pub id: Uuid,
    pub project_id: Uuid,
    pub memory_type: String,
    pub title: String,
    pub content: String,
    pub summary: String,
    pub metadata: Value,
    pub created_at: time::OffsetDateTime,
    pub claim_id: Option<Uuid>,
    pub claim_type: Option<String>,
    pub claim_key: Option<String>,
    pub claim_subject: Option<String>,
    pub claim_predicate: Option<String>,
    pub claim_object: Option<String>,
    pub claim_polarity: Option<String>,
    pub claim_authority_class: Option<String>,
    pub claim_verification_status: Option<String>,
    pub claim_valid_from: Option<time::OffsetDateTime>,
    pub claim_valid_to: Option<time::OffsetDateTime>,
    pub claim_superseded_by: Option<Uuid>,
    pub active_conflict_count: i64,
    /// v2.2.3: Governance state.
    pub claim_governance_state: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ClaimUpsertParams {
    pub memory_id: Uuid,
    pub session_id: Option<Uuid>,
    pub claim_key: String,
    pub claim_type: String,
    pub subject: String,
    pub predicate: String,
    pub object: String,
    pub claim_polarity: String,
    pub authority_class: String,
    pub verification_status: String,
    pub admitted: bool,
    pub valid_from: Option<String>,
    pub valid_to: Option<String>,
    pub superseded_by: Option<Uuid>,
}

#[derive(Debug, Clone, FromRow)]
pub struct ClaimRow {
    pub id: Uuid,
    pub memory_id: Uuid,
    pub claim_type: String,
    pub claim_key: String,
    pub subject: String,
    pub predicate: String,
    pub object: String,
    pub claim_polarity: String,
    pub authority_class: String,
    pub verification_status: String,
    pub admitted: bool,
    pub valid_from: time::OffsetDateTime,
    pub valid_to: Option<time::OffsetDateTime>,
    pub superseded_by: Option<Uuid>,
}

#[derive(Debug, Clone)]
pub struct ClaimProofInsertParams {
    pub claim_id: Uuid,
    pub memory_id: Uuid,
    pub session_id: Option<Uuid>,
    pub session_event_id: Option<Uuid>,
    pub proof_type: String,
    pub source_ref: String,
    pub excerpt: Option<String>,
    pub authority_class: Option<String>,
    pub verification_status: Option<String>,
    pub proof_time: Option<String>,
}

#[derive(Debug, Clone, FromRow)]
pub struct ClaimProofRow {
    pub memory_id: Uuid,
    pub proof_type: String,
    pub source_ref: String,
    pub excerpt: Option<String>,
    pub session_id: Option<Uuid>,
    pub session_event_id: Option<Uuid>,
    pub authority_class: Option<String>,
    pub verification_status: Option<String>,
}

#[derive(Debug, Clone, FromRow)]
pub struct ClaimRelationRow {
    pub memory_id: Uuid,
    pub claim_id: Uuid,
    pub related_claim_id: Uuid,
    pub related_memory_id: Option<Uuid>,
    pub edge_type: String,
    pub direction: String,
    pub title: Option<String>,
    pub summary: Option<String>,
    pub authority_class: Option<String>,
    pub verification_status: Option<String>,
}

#[derive(Debug, Clone, FromRow)]
pub struct DashboardGraphNodeRow {
    pub id: Uuid,
    pub title: String,
    pub memory_type: String,
    pub summary: String,
}

#[derive(Debug, Clone, FromRow)]
pub struct DashboardGraphEdgeRow {
    pub source: Uuid,
    pub target: Uuid,
    pub edge_type: String,
    pub weight: Option<f64>,
}

#[derive(Debug, Clone, FromRow)]
pub struct WorkerJobRecord {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub team_id: Uuid,
    pub project_id: Uuid,
    pub session_id: Option<Uuid>,
    pub memory_id: Option<Uuid>,
    pub job_type: String,
    pub dedupe_key: String,
    pub status: String,
    pub priority: i32,
    pub attempts: i32,
    pub max_attempts: i32,
    pub available_at: time::OffsetDateTime,
    pub claimed_at: Option<time::OffsetDateTime>,
    pub completed_at: Option<time::OffsetDateTime>,
    pub worker_id: Option<String>,
    pub payload: Value,
    pub last_error: Option<String>,
    pub created_at: time::OffsetDateTime,
    pub updated_at: time::OffsetDateTime,
}

#[derive(Debug, Clone)]
pub struct QueueSummary {
    pub total: i64,
    pub pending: i64,
    pub running: i64,
    pub poisoned: i64,
}

pub async fn ensure_scope_entities(
    tx: &mut Transaction<'_, Postgres>,
    context: &RepositoryContext,
) -> Result<(), DbError> {
    // Use bare ON CONFLICT DO NOTHING (no column target) so that ALL unique
    // constraints on each table are handled at the SQL level.  This prevents
    // PostgreSQL from aborting the transaction — catching the error in Rust
    // is too late because PG already marks the txn as aborted.
    sqlx::query(
        r#"
        insert into public.organizations (id, name, slug)
        values ($1, 'Default Organization', 'default-org')
        on conflict do nothing
        "#,
    )
    .bind(context.organization_id)
    .execute(&mut **tx)
    .await?;

    if let Some(actor_id) = context.actor_id {
        sqlx::query(
            r#"
            insert into public.app_users (id, email, display_name)
            values ($1, null, 'System User')
            on conflict do nothing
            "#,
        )
        .bind(actor_id)
        .execute(&mut **tx)
        .await?;
    }

    sqlx::query(
        r#"
        insert into public.teams (id, organization_id, name, slug)
        values ($1, $2, 'Default Team', 'default-team')
        on conflict do nothing
        "#,
    )
    .bind(context.team_id)
    .bind(context.organization_id)
    .execute(&mut **tx)
    .await?;

    if let Some(actor_id) = context.actor_id {
        // Insert if not exists, then update — split because ON CONFLICT DO NOTHING
        // can't carry a DO UPDATE clause without a conflict target.
        let role_str = match context.team_role {
            chum_mem_contracts::TeamRole::Owner => "owner",
            chum_mem_contracts::TeamRole::Admin => "admin",
            chum_mem_contracts::TeamRole::Member => "member",
        };
        sqlx::query(
            r#"
            insert into public.team_members (organization_id, team_id, user_id, role, status)
            values ($1, $2, $3, $4::public.team_role, 'active'::public.membership_status)
            on conflict do nothing
            "#,
        )
        .bind(context.organization_id)
        .bind(context.team_id)
        .bind(actor_id)
        .bind(role_str)
        .execute(&mut **tx)
        .await?;

        sqlx::query(
            r#"
            update public.team_members
            set role = $1::public.team_role,
                status = 'active'::public.membership_status
            where team_id = $2 and user_id = $3
            "#,
        )
        .bind(role_str)
        .bind(context.team_id)
        .bind(actor_id)
        .execute(&mut **tx)
        .await?;
    }

    if let Some(project_id) = context.project_id {
        sqlx::query(
            r#"
            insert into public.projects (id, organization_id, team_id, name, slug, default_branch)
            values ($1, $2, $3, 'Default Project', 'default-project', null)
            on conflict do nothing
            "#,
        )
        .bind(project_id)
        .bind(context.organization_id)
        .bind(context.team_id)
        .execute(&mut **tx)
        .await?;
    }

    Ok(())
}

pub async fn upsert_ingested_project(
    tx: &mut Transaction<'_, Postgres>,
    context: &RepositoryContext,
    project_id: Uuid,
    branch: Option<&str>,
) -> Result<(), DbError> {
    let slug = format!("project-{}", project_id.simple());
    let slug = &slug[..16.min(slug.len())];
    // Insert with bare ON CONFLICT DO NOTHING to handle both (id) and
    // (team_id, slug) constraints without aborting the transaction.
    sqlx::query(
        r#"
        insert into public.projects (id, organization_id, team_id, name, slug, default_branch)
        values ($1, $2, $3, 'Ingested Project', $4, $5)
        on conflict do nothing
        "#,
    )
    .bind(project_id)
    .bind(context.organization_id)
    .bind(context.team_id)
    .bind(slug)
    .bind(branch)
    .execute(&mut **tx)
    .await?;

    // Update the existing row (if insert was skipped due to conflict).
    sqlx::query(
        r#"
        update public.projects
        set default_branch = coalesce($2, default_branch)
        where id = $1 and slug != 'global'
        "#,
    )
    .bind(project_id)
    .bind(branch)
    .execute(&mut **tx)
    .await?;

    Ok(())
}

pub async fn upsert_session(
    tx: &mut Transaction<'_, Postgres>,
    context: &RepositoryContext,
    input: &StartSessionRequest,
) -> Result<StartedSession, DbError> {
    let mut metadata = match &input.metadata {
        Value::Object(existing) => existing.clone(),
        _ => serde_json::Map::new(),
    };
    metadata.insert("repo".to_string(), json!(input.repo));
    metadata.insert("local".to_string(), json!(input.local));

    let row = sqlx::query_as::<_, StartedSession>(
        r#"
        insert into public.sessions (
          organization_id,
          team_id,
          project_id,
          user_id,
          provider,
          external_session_id,
          branch,
          status,
          metadata
        )
        values ($1, $2, $3, $4, $5, $6, $7, 'active'::public.session_status, $8)
        on conflict (project_id, provider, external_session_id)
        do update set
          status = 'active'::public.session_status,
          branch = excluded.branch,
          metadata = excluded.metadata
        returning id, organization_id, team_id, project_id, status::text as status
        "#,
    )
    .bind(context.organization_id)
    .bind(context.team_id)
    .bind(input.project_id)
    .bind(context.actor_id)
    .bind(input.provider.as_str())
    .bind(&input.external_session_id)
    .bind(input.repo.branch.as_deref())
    .bind(Value::Object(metadata))
    .fetch_one(&mut **tx)
    .await?;

    Ok(row)
}

pub async fn insert_audit_log(
    tx: &mut Transaction<'_, Postgres>,
    context: &RepositoryContext,
    action: &str,
    target_type: &str,
    target_id: Uuid,
    metadata: Value,
) -> Result<(), DbError> {
    sqlx::query(
        r#"
        insert into public.audit_logs (
          organization_id,
          team_id,
          project_id,
          actor_type,
          actor_id,
          action,
          target_type,
          target_id,
          metadata
        )
        values ($1, $2, $3, $4::public.actor_type, $5, $6::public.audit_action, $7, $8, $9)
        "#,
    )
    .bind(context.organization_id)
    .bind(context.team_id)
    .bind(context.project_id)
    .bind(match context.actor_type {
        chum_mem_contracts::ActorType::User => "user",
        chum_mem_contracts::ActorType::Token => "token",
        chum_mem_contracts::ActorType::System => "system",
    })
    .bind(context.actor_id)
    .bind(action)
    .bind(target_type)
    .bind(target_id)
    .bind(metadata)
    .execute(&mut **tx)
    .await?;

    Ok(())
}

pub async fn resolve_session(
    tx: &mut Transaction<'_, Postgres>,
    context: &RepositoryContext,
    session_id: Uuid,
) -> Result<SessionRow, DbError> {
    let row = sqlx::query_as::<_, SessionRow>(
        r#"
        select
          id,
          provider::text as provider,
          project_id,
          external_session_id,
          status::text as status,
          branch
        from public.sessions
        where id = $1
          and organization_id = $2
          and team_id = $3
          and ($4::uuid is null or project_id = $4)
        limit 1
        "#,
    )
    .bind(session_id)
    .bind(context.organization_id)
    .bind(context.team_id)
    .bind(context.project_id)
    .fetch_optional(&mut **tx)
    .await?;

    row.ok_or(DbError::NotFound("session"))
}

pub async fn insert_session_event(
    tx: &mut Transaction<'_, Postgres>,
    context: &RepositoryContext,
    params: &AppendSessionEventParams,
) -> Result<AppendedSessionEvent, DbError> {
    let inserted_id = sqlx::query_scalar::<_, Uuid>(
        r#"
        insert into public.session_events (
          organization_id,
          team_id,
          project_id,
          session_id,
          provider,
          event_type,
          event_time,
          event_id,
          idempotency_key,
          payload,
          raw_payload,
          turn_id
        )
        values ($1, $2, $3, $4, $5, $6, $7::timestamptz, $8, $9, $10, $11, $12)
        on conflict do nothing
        returning id
        "#,
    )
    .bind(context.organization_id)
    .bind(context.team_id)
    .bind(params.project_id)
    .bind(params.session_id)
    .bind(params.provider.as_str())
    .bind(&params.event_type)
    .bind(&params.event_time)
    .bind(&params.event_id)
    .bind(&params.idempotency_key)
    .bind(&params.payload)
    .bind(&params.raw_payload)
    .bind(params.turn_id.as_deref())
    .fetch_optional(&mut **tx)
    .await?;

    // Bare ON CONFLICT DO NOTHING handles both unique constraints:
    //   1. (session_id, idempotency_key)
    //   2. (session_id, event_id)
    // Returns None when either fires — look up the existing row.
    let inserted_id = inserted_id;

    let duplicate = inserted_id.is_none();
    let event_id = match inserted_id {
        Some(id) => id,
        None => sqlx::query(
            r#"
            select id
            from public.session_events
            where session_id = $1
              and (idempotency_key = $2 or event_id = $3)
            limit 1
            "#,
        )
        .bind(params.session_id)
        .bind(&params.idempotency_key)
        .bind(&params.event_id)
        .fetch_one(&mut **tx)
        .await?
        .get("id"),
    };

    Ok(AppendedSessionEvent {
        event_id,
        duplicate,
    })
}

/// Batched session-event insert.
///
/// Collapses the previous per-row INSERT + fallback SELECT loop into a single
/// multi-row INSERT plus (at most) a single fallback SELECT for rows that hit
/// `on conflict do nothing`.
///
/// Ordering: the returned vector is in the same order as `params`, with each
/// entry carrying the same `idempotency_key` back so callers can map duplicates.
pub async fn insert_session_events_batch(
    tx: &mut Transaction<'_, Postgres>,
    context: &RepositoryContext,
    params: &[AppendSessionEventParams],
) -> Result<Vec<AppendedSessionEvent>, DbError> {
    if params.is_empty() {
        return Ok(Vec::new());
    }

    // Step 1: single multi-row INSERT ... RETURNING id, idempotency_key.
    // `on conflict do nothing` handles both unique constraints:
    //   (session_id, idempotency_key) and (session_id, event_id).
    let mut qb = sqlx::QueryBuilder::new(
        "insert into public.session_events (\
         organization_id, team_id, project_id, session_id, provider, event_type, event_time, \
         event_id, idempotency_key, payload, raw_payload, turn_id\
         ) ",
    );
    qb.push_values(params, |mut b, row| {
        b.push_bind(context.organization_id)
            .push_bind(context.team_id)
            .push_bind(row.project_id)
            .push_bind(row.session_id);
        b.push_bind(row.provider.as_str());
        b.push_bind(&row.event_type);
        b.push_bind(&row.event_time);
        b.push_unseparated("::timestamptz");
        b.push_bind(&row.event_id)
            .push_bind(&row.idempotency_key)
            .push_bind(&row.payload)
            .push_bind(&row.raw_payload)
            .push_bind(row.turn_id.as_deref());
    });
    qb.push(" on conflict do nothing returning id, idempotency_key, event_id");

    let inserted_rows = qb
        .build()
        .fetch_all(&mut **tx)
        .await
        .map_err(DbError::from)?;

    // Map insertions by idempotency_key (primary) and event_id (fallback).
    let mut inserted_by_idem: std::collections::HashMap<String, Uuid> =
        std::collections::HashMap::with_capacity(inserted_rows.len());
    let mut inserted_by_event_id: std::collections::HashMap<(Uuid, String), Uuid> =
        std::collections::HashMap::with_capacity(inserted_rows.len());
    for row in &inserted_rows {
        let id: Uuid = row.try_get("id").map_err(DbError::from)?;
        let idem: String = row.try_get("idempotency_key").map_err(DbError::from)?;
        let event_id: String = row.try_get("event_id").map_err(DbError::from)?;
        inserted_by_idem.insert(idem, id);
        // We don't know session_id without re-binding; use (session_id,event_id) per param.
        let _ = inserted_by_event_id.insert((Uuid::nil(), event_id), id);
    }

    // Step 2: collect the subset that did not return a row (duplicates). One
    // batched SELECT resolves them all.
    let mut results: Vec<AppendedSessionEvent> = Vec::with_capacity(params.len());
    let mut missing_indices: Vec<usize> = Vec::new();

    for (idx, row) in params.iter().enumerate() {
        if let Some(&id) = inserted_by_idem.get(&row.idempotency_key) {
            results.push(AppendedSessionEvent {
                event_id: id,
                duplicate: false,
            });
        } else {
            // Placeholder; filled in after fallback lookup.
            results.push(AppendedSessionEvent {
                event_id: Uuid::nil(),
                duplicate: true,
            });
            missing_indices.push(idx);
        }
    }

    if missing_indices.is_empty() {
        return Ok(results);
    }

    // Batched fallback SELECT: resolve duplicates on (session_id, idempotency_key, event_id)
    // for the rows that did not come back from INSERT.
    let mut qb = sqlx::QueryBuilder::new(
        "select id, session_id, idempotency_key, event_id from public.session_events where ",
    );
    qb.push("(session_id, idempotency_key) in (");
    let mut first = true;
    for idx in &missing_indices {
        if !first {
            qb.push(", ");
        }
        first = false;
        qb.push("(");
        qb.push_bind(params[*idx].session_id);
        qb.push(", ");
        qb.push_bind(&params[*idx].idempotency_key);
        qb.push(")");
    }
    qb.push(") or (session_id, event_id) in (");
    let mut first = true;
    for idx in &missing_indices {
        if !first {
            qb.push(", ");
        }
        first = false;
        qb.push("(");
        qb.push_bind(params[*idx].session_id);
        qb.push(", ");
        qb.push_bind(&params[*idx].event_id);
        qb.push(")");
    }
    qb.push(")");

    let resolved = qb
        .build()
        .fetch_all(&mut **tx)
        .await
        .map_err(DbError::from)?;

    // Index by (session_id, idempotency_key) and (session_id, event_id).
    let mut by_idem: std::collections::HashMap<(Uuid, String), Uuid> =
        std::collections::HashMap::with_capacity(resolved.len());
    let mut by_event_id: std::collections::HashMap<(Uuid, String), Uuid> =
        std::collections::HashMap::with_capacity(resolved.len());
    for row in &resolved {
        let id: Uuid = row.try_get("id").map_err(DbError::from)?;
        let session_id: Uuid = row.try_get("session_id").map_err(DbError::from)?;
        let idem: String = row.try_get("idempotency_key").map_err(DbError::from)?;
        let event_id: String = row.try_get("event_id").map_err(DbError::from)?;
        by_idem.insert((session_id, idem), id);
        by_event_id.insert((session_id, event_id), id);
    }

    for idx in missing_indices {
        let row = &params[idx];
        let key_a = (row.session_id, row.idempotency_key.clone());
        let key_b = (row.session_id, row.event_id.clone());
        let resolved_id = by_idem
            .get(&key_a)
            .copied()
            .or_else(|| by_event_id.get(&key_b).copied())
            .ok_or_else(|| DbError::Sqlx(sqlx::Error::RowNotFound))?;
        results[idx].event_id = resolved_id;
        // results[idx].duplicate is already true
    }

    Ok(results)
}

/// High-throughput bulk insert using COPY FROM STDIN (CSV) through an UNLOGGED
/// staging table, with deferred unique-constraint checking.
///
/// Flow:
///   1. CREATE UNLOGGED staging table in public schema (unique per call).
///   2. COPY rows into staging via PgPoolCopyExt (CSV format).
///   3. SET CONSTRAINTS … DEFERRED + INSERT INTO … SELECT … ON CONFLICT DO NOTHING.
///   4. DROP staging table.
///
/// Returns (inserted, duplicates).
pub async fn bulk_insert_session_events_copy(
    pool: &sqlx::PgPool,
    context: &RepositoryContext,
    params: Vec<AppendSessionEventParams>,
) -> Result<(usize, usize), DbError> {
    use sqlx::Executor;
    use sqlx::postgres::PgPoolCopyExt;

    if params.is_empty() {
        return Ok((0, 0));
    }

    let total = params.len();
    let staging = format!("_staging_se_{}", Uuid::new_v4().simple());

    // 1. Create UNLOGGED staging table (no indexes, no constraints, no RLS).
    let create_sql = format!(
        "CREATE UNLOGGED TABLE public.{staging} (\
           organization_id uuid, team_id uuid, project_id uuid, session_id uuid, \
           provider text, event_type text, event_time timestamptz, \
           event_id text, idempotency_key text, payload jsonb, raw_payload jsonb, \
           turn_id text\
         )"
    );
    pool.execute(create_sql.as_str())
        .await
        .map_err(DbError::Sqlx)?;

    // Build CSV payload in memory. For typical batch sizes (200-2000) this is fine.
    let mut csv_buf = String::with_capacity(total * 512);
    for row in &params {
        let provider_str = row.provider.as_str();
        let payload_json = row.payload.to_string();
        let raw_payload_json = row.raw_payload.to_string();
        let turn_id = row.turn_id.as_deref().unwrap_or("");

        use std::fmt::Write;
        writeln!(
            csv_buf,
            "{org},{team},{proj},{sess},\"{prov}\",\"{etype}\",\"{etime}\",\"{eid}\",\"{idem}\",\"{payload}\",\"{raw}\",\"{turn}\"",
            org = context.organization_id,
            team = context.team_id,
            proj = row.project_id,
            sess = row.session_id,
            prov = provider_str,
            etype = row.event_type.replace('"', "\"\""),
            etime = row.event_time.replace('"', "\"\""),
            eid = row.event_id.replace('"', "\"\""),
            idem = row.idempotency_key.replace('"', "\"\""),
            payload = payload_json.replace('"', "\"\""),
            raw = raw_payload_json.replace('"', "\"\""),
            turn = turn_id.replace('"', "\"\""),
        )
        .expect("write to String cannot fail");
    }

    // 2. COPY rows into staging via pool-level copy_in_raw.
    let copy_stmt = format!(
        "COPY public.{staging} (organization_id, team_id, project_id, session_id, \
         provider, event_type, event_time, event_id, idempotency_key, \
         payload, raw_payload, turn_id) \
         FROM STDIN WITH (FORMAT csv, NULL '')"
    );

    let copy_bytes = csv_buf.into_bytes();

    let mut copy_in = pool.copy_in_raw(&copy_stmt).await.map_err(DbError::Sqlx)?;
    let send_result = copy_in.send(copy_bytes.as_slice()).await;
    if let Err(e) = send_result {
        let _ = copy_in.abort("send failed").await;
        let drop_sql = format!("DROP TABLE IF EXISTS public.{staging}");
        let _ = pool.execute(drop_sql.as_str()).await;
        return Err(DbError::Sqlx(e));
    }
    if let Err(e) = copy_in.finish().await {
        let drop_sql = format!("DROP TABLE IF EXISTS public.{staging}");
        let _ = pool.execute(drop_sql.as_str()).await;
        return Err(DbError::Sqlx(e));
    }

    // 3. Merge from staging into session_events (ON CONFLICT DO NOTHING for idempotency).
    let merge_sql = format!(
        "INSERT INTO public.session_events \
           (organization_id, team_id, project_id, session_id, provider, event_type, \
            event_time, event_id, idempotency_key, payload, raw_payload, turn_id) \
         SELECT organization_id, team_id, project_id, session_id, \
                provider, event_type, event_time, \
                event_id, idempotency_key, payload, raw_payload, \
                NULLIF(turn_id, '') \
         FROM public.{staging} \
         ON CONFLICT DO NOTHING"
    );
    let result = pool
        .execute(merge_sql.as_str())
        .await
        .map_err(DbError::Sqlx)?;
    let inserted = result.rows_affected() as usize;

    // 4. Cleanup staging table.
    let drop_sql = format!("DROP TABLE IF EXISTS public.{staging}");
    let _ = pool.execute(drop_sql.as_str()).await;

    let duplicates = total - inserted;
    Ok((inserted, duplicates))
}

/// Drop non-unique indexes on session_events for bulk import windows.
pub async fn drop_session_events_indexes(pool: &sqlx::PgPool) -> Result<(), DbError> {
    sqlx::raw_sql("SELECT public.drop_session_events_bulk_indexes()")
        .execute(pool)
        .await
        .map_err(DbError::Sqlx)?;
    Ok(())
}

/// Recreate non-unique indexes on session_events after bulk import.
pub async fn create_session_events_indexes(pool: &sqlx::PgPool) -> Result<(), DbError> {
    sqlx::raw_sql("SELECT public.create_session_events_bulk_indexes()")
        .execute(pool)
        .await
        .map_err(DbError::Sqlx)?;
    Ok(())
}

pub async fn load_session_events(
    tx: &mut Transaction<'_, Postgres>,
    session_id: Uuid,
) -> Result<Vec<SessionEventRow>, DbError> {
    load_session_events_limited(tx, session_id, None).await
}

/// Load session events with an optional row limit.
/// Node.js uses limit 1000 in the worker and limit 100 for candidate edge derivation.
pub async fn load_session_events_limited(
    tx: &mut Transaction<'_, Postgres>,
    session_id: Uuid,
    limit: Option<i64>,
) -> Result<Vec<SessionEventRow>, DbError> {
    sqlx::query_as::<_, SessionEventRow>(
        r#"
        select id, event_type, payload, created_at
        from public.session_events
        where session_id = $1
        order by created_at asc
        limit $2
        "#,
    )
    .bind(session_id)
    .bind(limit)
    .fetch_all(&mut **tx)
    .await
    .map_err(DbError::from)
}

pub async fn upsert_session_episode(
    tx: &mut Transaction<'_, Postgres>,
    context: &RepositoryContext,
    project_id: Uuid,
    session_id: Uuid,
    episode_ordinal: i32,
    episode_type: &str,
    title: &str,
    summary: &str,
    started_at: &str,
    ended_at: &str,
    metadata: &Value,
) -> Result<EpisodeRow, DbError> {
    sqlx::query_as::<_, EpisodeRow>(
        r#"
        insert into public.session_episodes (
          organization_id, team_id, project_id, session_id, episode_ordinal, episode_type,
          title, summary, started_at, ended_at, metadata
        )
        values ($1, $2, $3, $4, $5, $6, $7, $8, $9::timestamptz, $10::timestamptz, $11)
        on conflict (session_id, episode_ordinal) do update set
          episode_type = excluded.episode_type,
          title = excluded.title,
          summary = excluded.summary,
          started_at = excluded.started_at,
          ended_at = excluded.ended_at,
          metadata = excluded.metadata
        returning id, episode_ordinal
        "#,
    )
    .bind(context.organization_id)
    .bind(context.team_id)
    .bind(project_id)
    .bind(session_id)
    .bind(episode_ordinal)
    .bind(episode_type)
    .bind(title)
    .bind(summary)
    .bind(started_at)
    .bind(ended_at)
    .bind(metadata)
    .fetch_one(&mut **tx)
    .await
    .map_err(DbError::from)
}

/// Batch upsert session episodes in a single query, returning IDs.
pub async fn upsert_session_episodes_batch(
    tx: &mut Transaction<'_, Postgres>,
    context: &RepositoryContext,
    project_id: Uuid,
    session_id: Uuid,
    episodes: &[EpisodeBatchRow],
) -> Result<Vec<EpisodeRow>, DbError> {
    if episodes.is_empty() {
        return Ok(Vec::new());
    }
    let mut qb = sqlx::QueryBuilder::new(
        "insert into public.session_episodes (organization_id, team_id, project_id, session_id, episode_ordinal, episode_type, title, summary, started_at, ended_at, metadata) ",
    );
    qb.push_values(episodes, |mut b, ep| {
        b.push_bind(context.organization_id)
            .push_bind(context.team_id)
            .push_bind(project_id)
            .push_bind(session_id)
            .push_bind(ep.episode_ordinal)
            .push_bind(&ep.episode_type)
            .push_bind(&ep.title)
            .push_bind(&ep.summary);
        b.push_bind(&ep.started_at);
        b.push_unseparated("::timestamptz");
        b.push_bind(&ep.ended_at);
        b.push_unseparated("::timestamptz");
        b.push_bind(&ep.metadata);
    });
    qb.push(
        " on conflict (session_id, episode_ordinal) do update set \
         episode_type = excluded.episode_type, \
         title = excluded.title, \
         summary = excluded.summary, \
         started_at = excluded.started_at, \
         ended_at = excluded.ended_at, \
         metadata = excluded.metadata \
         returning id, episode_ordinal",
    );
    qb.build_query_as::<EpisodeRow>()
        .fetch_all(&mut **tx)
        .await
        .map_err(DbError::from)
}

pub struct EpisodeBatchRow {
    pub episode_ordinal: i32,
    pub episode_type: String,
    pub title: String,
    pub summary: String,
    pub started_at: String,
    pub ended_at: String,
    pub metadata: Value,
}

pub async fn insert_memory(
    tx: &mut Transaction<'_, Postgres>,
    context: &RepositoryContext,
    project_id: Uuid,
    created_by: Option<Uuid>,
    params: &MemoryInsertParams,
) -> Result<Uuid, DbError> {
    sqlx::query_scalar::<_, Uuid>(
        r#"
        insert into public.memories (
          organization_id, team_id, project_id, session_id, episode_id, type, title, content,
          summary, importance_score, confidence_score, metadata, created_by
        )
        values ($1, $2, $3, $4, $5, $6::public.memory_type, $7, $8, $9, $10, $11, $12, $13)
        returning id
        "#,
    )
    .bind(context.organization_id)
    .bind(context.team_id)
    .bind(project_id)
    .bind(params.session_id)
    .bind(params.episode_id)
    .bind(&params.memory_type)
    .bind(&params.title)
    .bind(&params.content)
    .bind(&params.summary)
    .bind(params.importance_score)
    .bind(params.confidence_score)
    .bind(&params.metadata)
    .bind(created_by)
    .fetch_one(&mut **tx)
    .await
    .map_err(DbError::from)
}

pub async fn upsert_embedding(
    tx: &mut Transaction<'_, Postgres>,
    context: &RepositoryContext,
    project_id: Uuid,
    memory_id: Uuid,
    model: &str,
    embedding_literal: &str,
) -> Result<(), DbError> {
    sqlx::query(
        r#"
        insert into public.embeddings (
          organization_id, team_id, project_id, memory_id, model, embedding
        )
        values ($1, $2, $3, $4, $5, $6::vector)
        on conflict (memory_id, model) do update set
          embedding = excluded.embedding
        "#,
    )
    .bind(context.organization_id)
    .bind(context.team_id)
    .bind(project_id)
    .bind(memory_id)
    .bind(model)
    .bind(embedding_literal)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn append_memory_provenance(
    tx: &mut Transaction<'_, Postgres>,
    context: &RepositoryContext,
    project_id: Uuid,
    memory_id: Uuid,
    session_event_id: Uuid,
    excerpt: Option<&str>,
) -> Result<(), DbError> {
    sqlx::query(
        r#"
        insert into public.memory_provenance (
          organization_id, team_id, project_id, memory_id, session_event_id, excerpt
        )
        values ($1, $2, $3, $4, $5, $6)
        on conflict (memory_id, session_event_id) do nothing
        "#,
    )
    .bind(context.organization_id)
    .bind(context.team_id)
    .bind(project_id)
    .bind(memory_id)
    .bind(session_event_id)
    .bind(excerpt)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn append_memory_provenance_preview(
    tx: &mut Transaction<'_, Postgres>,
    context: &RepositoryContext,
    project_id: Uuid,
    memory_id: Uuid,
    session_id: Option<Uuid>,
    session_event_id: Option<Uuid>,
    excerpt: Option<&str>,
) -> Result<(), DbError> {
    sqlx::query(
        r#"
        insert into public.memory_provenance_preview (
          memory_id, organization_id, team_id, project_id, session_id, session_event_id, excerpt
        )
        values ($1, $2, $3, $4, $5, $6, $7)
        on conflict (memory_id) do update set
          session_id = excluded.session_id,
          session_event_id = excluded.session_event_id,
          excerpt = excluded.excerpt,
          computed_at = now()
        "#,
    )
    .bind(memory_id)
    .bind(context.organization_id)
    .bind(context.team_id)
    .bind(project_id)
    .bind(session_id)
    .bind(session_event_id)
    .bind(excerpt)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn mark_session_completed(
    tx: &mut Transaction<'_, Postgres>,
    session_id: Uuid,
    metadata_patch: &Value,
) -> Result<SessionEndResult, DbError> {
    sqlx::query_as::<_, SessionEndResult>(
        r#"
        update public.sessions
        set
          status = 'completed'::public.session_status,
          ended_at = now(),
          metadata = coalesce(metadata, '{}'::jsonb) || $2
        where id = $1
        returning id, project_id, status::text as status
        "#,
    )
    .bind(session_id)
    .bind(metadata_patch)
    .fetch_one(&mut **tx)
    .await
    .map_err(DbError::from)
}

pub async fn load_candidate_completed_sessions(
    tx: &mut Transaction<'_, Postgres>,
    context: &RepositoryContext,
    project_id: Uuid,
    session_id: Uuid,
) -> Result<Vec<CandidateSessionRow>, DbError> {
    sqlx::query_as::<_, CandidateSessionRow>(
        r#"
        select id, branch
        from public.sessions
        where organization_id = $1
          and team_id = $2
          and project_id = $3
          and id <> $4
          and status = 'completed'::public.session_status
        order by started_at desc
        limit 20
        "#,
    )
    .bind(context.organization_id)
    .bind(context.team_id)
    .bind(project_id)
    .bind(session_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(DbError::from)
}

pub async fn resolve_session_events_for_candidate(
    tx: &mut Transaction<'_, Postgres>,
    session_id: Uuid,
) -> Result<Vec<SessionEventRow>, DbError> {
    // Match Node.js: limit 100 events for candidate edge derivation
    load_session_events_limited(tx, session_id, Some(100)).await
}

pub async fn upsert_session_edge(
    tx: &mut Transaction<'_, Postgres>,
    context: &RepositoryContext,
    project_id: Uuid,
    from_session_id: Uuid,
    to_session_id: Uuid,
    edge_type: &str,
    weight: f64,
    metadata: &Value,
) -> Result<(), DbError> {
    sqlx::query(
        r#"
        insert into public.session_edges (
          organization_id, team_id, project_id, from_session_id, to_session_id, edge_type, weight, metadata
        )
        values ($1, $2, $3, $4, $5, $6, $7, $8)
        on conflict (from_session_id, to_session_id, edge_type) do update set
          weight = excluded.weight,
          metadata = excluded.metadata
        "#,
    )
    .bind(context.organization_id)
    .bind(context.team_id)
    .bind(project_id)
    .bind(from_session_id)
    .bind(to_session_id)
    .bind(edge_type)
    .bind(weight)
    .bind(metadata)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn upsert_memory_edge(
    tx: &mut Transaction<'_, Postgres>,
    context: &RepositoryContext,
    project_id: Uuid,
    from_memory_id: Uuid,
    to_memory_id: Uuid,
    edge_type: &str,
    evidence: &str,
    weight: f64,
    metadata: &Value,
) -> Result<(), DbError> {
    sqlx::query(
        r#"
        insert into public.memory_edges (
          organization_id, team_id, project_id, from_memory_id, to_memory_id, edge_type, evidence, weight, metadata
        )
        values ($1, $2, $3, $4, $5, $6::public.memory_edge_type, $7::public.evidence_level, $8, $9)
        on conflict (from_memory_id, to_memory_id, edge_type) do update set
          evidence = excluded.evidence,
          weight = excluded.weight,
          metadata = excluded.metadata
        "#,
    )
    .bind(context.organization_id)
    .bind(context.team_id)
    .bind(project_id)
    .bind(from_memory_id)
    .bind(to_memory_id)
    .bind(edge_type)
    .bind(evidence)
    .bind(weight)
    .bind(metadata)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn upsert_claim(
    tx: &mut Transaction<'_, Postgres>,
    context: &RepositoryContext,
    project_id: Uuid,
    params: &ClaimUpsertParams,
) -> Result<ClaimRow, DbError> {
    sqlx::query_as::<_, ClaimRow>(
        r#"
        insert into public.claims (
          organization_id, team_id, project_id, memory_id, session_id, claim_key, claim_type,
          subject, predicate, object, claim_polarity, authority_class, verification_status,
          admitted, valid_from, valid_to, superseded_by
        )
        values (
          $1, $2, $3, $4, $5, $6, $7::public.memory_type, $8, $9, $10, $11, $12, $13, $14,
          coalesce($15::timestamptz, now()), $16::timestamptz, $17
        )
        on conflict (memory_id) do update set
          session_id = excluded.session_id,
          claim_key = excluded.claim_key,
          claim_type = excluded.claim_type,
          subject = excluded.subject,
          predicate = excluded.predicate,
          object = excluded.object,
          claim_polarity = excluded.claim_polarity,
          authority_class = excluded.authority_class,
          verification_status = excluded.verification_status,
          admitted = excluded.admitted,
          valid_from = excluded.valid_from,
          valid_to = excluded.valid_to,
          superseded_by = excluded.superseded_by,
          updated_at = now()
        returning
          id, memory_id, claim_type::text as claim_type, claim_key, subject, predicate, object,
          claim_polarity, authority_class, verification_status, admitted, valid_from, valid_to, superseded_by
        "#,
    )
    .bind(context.organization_id)
    .bind(context.team_id)
    .bind(project_id)
    .bind(params.memory_id)
    .bind(params.session_id)
    .bind(&params.claim_key)
    .bind(&params.claim_type)
    .bind(&params.subject)
    .bind(&params.predicate)
    .bind(&params.object)
    .bind(&params.claim_polarity)
    .bind(&params.authority_class)
    .bind(&params.verification_status)
    .bind(params.admitted)
    .bind(params.valid_from.as_deref())
    .bind(params.valid_to.as_deref())
    .bind(params.superseded_by)
    .fetch_one(&mut **tx)
    .await
    .map_err(DbError::from)
}

pub async fn replace_claim_proofs(
    tx: &mut Transaction<'_, Postgres>,
    context: &RepositoryContext,
    project_id: Uuid,
    claim_id: Uuid,
    proofs: &[ClaimProofInsertParams],
) -> Result<(), DbError> {
    sqlx::query(
        r#"
        delete from public.claim_proofs
        where organization_id = $1
          and team_id = $2
          and project_id = $3
          and claim_id = $4
        "#,
    )
    .bind(context.organization_id)
    .bind(context.team_id)
    .bind(project_id)
    .bind(claim_id)
    .execute(&mut **tx)
    .await?;

    if proofs.is_empty() {
        return Ok(());
    }

    // Single multi-row INSERT via QueryBuilder::push_values to collapse the
    // previous per-row loop into one round trip per session_end draft.
    let mut qb = sqlx::QueryBuilder::new(
        "insert into public.claim_proofs (\
         organization_id, team_id, project_id, claim_id, memory_id, session_id, session_event_id, \
         proof_type, source_ref, excerpt, authority_class, verification_status, proof_time\
         ) ",
    );
    qb.push_values(proofs, |mut b, proof| {
        b.push_bind(context.organization_id)
            .push_bind(context.team_id)
            .push_bind(project_id)
            .push_bind(proof.claim_id)
            .push_bind(proof.memory_id)
            .push_bind(proof.session_id)
            .push_bind(proof.session_event_id)
            .push_bind(&proof.proof_type)
            .push_bind(&proof.source_ref)
            .push_bind(&proof.excerpt)
            .push_bind(&proof.authority_class)
            .push_bind(&proof.verification_status);
        b.push_bind(proof.proof_time.as_deref());
        b.push_unseparated("::timestamptz");
    });
    // `delete` above already purged prior proofs for this claim, but multiple
    // drafts in the same batch can share `(claim_id, source_ref)` tuples when
    // the same session_event is cited; keep the unique constraint safe.
    qb.push(" on conflict (claim_id, source_ref) do nothing");

    qb.build().execute(&mut **tx).await.map_err(DbError::from)?;

    Ok(())
}

/// Batched provenance insert used by session_end derivation.
///
/// Collapses the previous per-row `append_memory_provenance` loop inside
/// `derive_and_persist_session_memories` into one multi-row INSERT per
/// session_end (or chunk of drafts).
pub async fn append_memory_provenance_batch(
    tx: &mut Transaction<'_, Postgres>,
    context: &RepositoryContext,
    project_id: Uuid,
    rows: &[(Uuid, Uuid, Option<String>)],
) -> Result<(), DbError> {
    if rows.is_empty() {
        return Ok(());
    }
    let mut qb = sqlx::QueryBuilder::new(
        "insert into public.memory_provenance (\
         organization_id, team_id, project_id, memory_id, session_event_id, excerpt\
         ) ",
    );
    qb.push_values(rows, |mut b, (memory_id, session_event_id, excerpt)| {
        b.push_bind(context.organization_id)
            .push_bind(context.team_id)
            .push_bind(project_id)
            .push_bind(*memory_id)
            .push_bind(*session_event_id)
            .push_bind(excerpt.clone());
    });
    qb.push(" on conflict (memory_id, session_event_id) do nothing");

    qb.build().execute(&mut **tx).await.map_err(DbError::from)?;
    Ok(())
}

pub async fn update_claim_verification_status(
    tx: &mut Transaction<'_, Postgres>,
    context: &RepositoryContext,
    project_id: Uuid,
    claim_id: Uuid,
    verification_status: &str,
) -> Result<(), DbError> {
    sqlx::query(
        r#"
        update public.claims
        set verification_status = $5, updated_at = now()
        where id = $1
          and organization_id = $2
          and team_id = $3
          and project_id = $4
        "#,
    )
    .bind(claim_id)
    .bind(context.organization_id)
    .bind(context.team_id)
    .bind(project_id)
    .bind(verification_status)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn mark_claim_superseded(
    tx: &mut Transaction<'_, Postgres>,
    context: &RepositoryContext,
    project_id: Uuid,
    claim_id: Uuid,
    superseded_by: Uuid,
) -> Result<(), DbError> {
    sqlx::query(
        r#"
        update public.claims
        set superseded_by = $5, valid_to = now(), updated_at = now()
        where id = $1
          and organization_id = $2
          and team_id = $3
          and project_id = $4
          and superseded_by is null
        "#,
    )
    .bind(claim_id)
    .bind(context.organization_id)
    .bind(context.team_id)
    .bind(project_id)
    .bind(superseded_by)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn upsert_claim_edge(
    tx: &mut Transaction<'_, Postgres>,
    context: &RepositoryContext,
    project_id: Uuid,
    from_claim_id: Uuid,
    to_claim_id: Uuid,
    edge_type: &str,
    weight: f64,
    metadata: &Value,
) -> Result<(), DbError> {
    sqlx::query(
        r#"
        insert into public.claim_edges (
          organization_id, team_id, project_id, from_claim_id, to_claim_id, edge_type, weight, metadata
        )
        values ($1, $2, $3, $4, $5, $6::public.memory_edge_type, $7, $8)
        on conflict (from_claim_id, to_claim_id, edge_type) do update set
          weight = excluded.weight,
          metadata = excluded.metadata
        "#,
    )
    .bind(context.organization_id)
    .bind(context.team_id)
    .bind(project_id)
    .bind(from_claim_id)
    .bind(to_claim_id)
    .bind(edge_type)
    .bind(weight)
    .bind(metadata)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn mark_memory_superseded(
    tx: &mut Transaction<'_, Postgres>,
    context: &RepositoryContext,
    project_id: Uuid,
    memory_id: Uuid,
) -> Result<(), DbError> {
    sqlx::query(
        r#"
        update public.memories
        set superseded_at = now()
        where id = $1
          and organization_id = $2
          and team_id = $3
          and project_id = $4
          and superseded_at is null
        "#,
    )
    .bind(memory_id)
    .bind(context.organization_id)
    .bind(context.team_id)
    .bind(project_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn enqueue_worker_job(
    tx: &mut Transaction<'_, Postgres>,
    context: &RepositoryContext,
    project_id: Uuid,
    session_id: Option<Uuid>,
    memory_id: Option<Uuid>,
    job_type: &str,
    dedupe_key: &str,
    priority: i32,
    max_attempts: i32,
    available_at: Option<&str>,
    payload: &Value,
) -> Result<WorkerJobRecord, DbError> {
    sqlx::query_as::<_, WorkerJobRecord>(
        r#"
        insert into public.worker_jobs (
          organization_id, team_id, project_id, session_id, memory_id, job_type, dedupe_key,
          priority, max_attempts, available_at, payload
        )
        values ($1, $2, $3, $4, $5, $6, $7, $8, $9, coalesce($10::timestamptz, now()), $11)
        on conflict (project_id, job_type, dedupe_key) where status in ('pending', 'running')
        do update set
          payload = excluded.payload,
          available_at = least(public.worker_jobs.available_at, excluded.available_at),
          priority = least(public.worker_jobs.priority, excluded.priority),
          updated_at = now()
        returning
          id, organization_id, team_id, project_id, session_id, memory_id, job_type, dedupe_key,
          status::text as status, priority, attempts, max_attempts, available_at, claimed_at,
          completed_at, worker_id, payload, last_error, created_at, updated_at
        "#,
    )
    .bind(context.organization_id)
    .bind(context.team_id)
    .bind(project_id)
    .bind(session_id)
    .bind(memory_id)
    .bind(job_type)
    .bind(dedupe_key)
    .bind(priority)
    .bind(max_attempts)
    .bind(available_at)
    .bind(payload)
    .fetch_one(&mut **tx)
    .await
    .map_err(DbError::from)
}

pub async fn create_session_replay(
    tx: &mut Transaction<'_, Postgres>,
    context: &RepositoryContext,
    project_id: Uuid,
    session_id: Uuid,
    worker_job_id: Uuid,
    reason: &str,
    metadata: &Value,
) -> Result<(), DbError> {
    sqlx::query(
        r#"
        insert into public.session_replays (
          organization_id, team_id, project_id, session_id, worker_job_id, reason, metadata
        )
        values ($1, $2, $3, $4, $5, $6, $7)
        on conflict (session_id) where status in ('queued', 'ready')
        do update set
          status = 'queued'::public.session_replay_status,
          worker_job_id = excluded.worker_job_id,
          reason = excluded.reason,
          metadata = excluded.metadata,
          queued_at = now(),
          prepared_at = null,
          completed_at = null
        "#,
    )
    .bind(context.organization_id)
    .bind(context.team_id)
    .bind(project_id)
    .bind(session_id)
    .bind(worker_job_id)
    .bind(reason)
    .bind(metadata)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn claim_next_worker_job(
    tx: &mut Transaction<'_, Postgres>,
    context: &RepositoryContext,
    worker_id: &str,
    allowed_types: &[&str],
) -> Result<Option<WorkerJobRecord>, DbError> {
    sqlx::query_as::<_, WorkerJobRecord>(
        r#"
        with candidate as (
          select id
          from public.worker_jobs
          where organization_id = $1
            and team_id = $2
            and ($3::uuid is null or project_id = $3)
            and status = 'pending'::public.worker_job_status
            and available_at <= now()
            and job_type = any($5)
          order by priority asc, created_at asc
          for update skip locked
          limit 1
        )
        update public.worker_jobs as jobs
        set
          status = 'running'::public.worker_job_status,
          worker_id = $4,
          claimed_at = now(),
          attempts = jobs.attempts + 1,
          updated_at = now()
        from candidate
        where jobs.id = candidate.id
        returning
          jobs.id, jobs.organization_id, jobs.team_id, jobs.project_id, jobs.session_id,
          jobs.memory_id, jobs.job_type, jobs.dedupe_key, jobs.status::text as status,
          jobs.priority, jobs.attempts, jobs.max_attempts, jobs.available_at, jobs.claimed_at,
          jobs.completed_at, jobs.worker_id, jobs.payload, jobs.last_error, jobs.created_at,
          jobs.updated_at
        "#,
    )
    .bind(context.organization_id)
    .bind(context.team_id)
    .bind(context.project_id)
    .bind(worker_id)
    .bind(allowed_types)
    .fetch_optional(&mut **tx)
    .await
    .map_err(DbError::from)
}

pub async fn complete_worker_job(
    tx: &mut Transaction<'_, Postgres>,
    job: &WorkerJobRecord,
) -> Result<(), DbError> {
    sqlx::query(
        r#"
        update public.worker_jobs
        set status = 'completed'::public.worker_job_status, completed_at = now(), updated_at = now(), last_error = null
        where id = $1
        "#,
    )
    .bind(job.id)
    .execute(&mut **tx)
    .await?;

    sqlx::query(
        r#"
        insert into public.worker_job_attempts (
          organization_id, team_id, project_id, worker_job_id, attempt_number, worker_id, outcome, started_at
        )
        values ($1, $2, $3, $4, $5, $6, 'completed', $7)
        on conflict (worker_job_id, attempt_number) do update set
          outcome = excluded.outcome,
          worker_id = excluded.worker_id,
          error = null,
          started_at = excluded.started_at,
          finished_at = now()
        "#,
    )
    .bind(job.organization_id)
    .bind(job.team_id)
    .bind(job.project_id)
    .bind(job.id)
    .bind(job.attempts)
    .bind(&job.worker_id)
    .bind(job.claimed_at)
    .execute(&mut **tx)
    .await?;

    Ok(())
}

pub async fn fail_worker_job(
    tx: &mut Transaction<'_, Postgres>,
    job: &WorkerJobRecord,
    error_message: &str,
) -> Result<String, DbError> {
    let failure_state = next_failure_state(job.attempts, job.max_attempts);
    let available_at = time::OffsetDateTime::now_utc()
        + time::Duration::milliseconds(failure_state.delay_ms.into());
    sqlx::query(
        r#"
        update public.worker_jobs
        set
          status = $2::public.worker_job_status,
          available_at = case when $2 = 'pending' then $3 else available_at end,
          completed_at = case when $2 = 'poisoned' then now() else null end,
          updated_at = now(),
          last_error = $4
        where id = $1
        "#,
    )
    .bind(job.id)
    .bind(&failure_state.status)
    .bind(available_at)
    .bind(error_message)
    .execute(&mut **tx)
    .await?;

    sqlx::query(
        r#"
        insert into public.worker_job_attempts (
          organization_id, team_id, project_id, worker_job_id, attempt_number, worker_id, outcome, error, started_at
        )
        values ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        on conflict (worker_job_id, attempt_number) do update set
          outcome = excluded.outcome,
          worker_id = excluded.worker_id,
          error = excluded.error,
          started_at = excluded.started_at,
          finished_at = now()
        "#,
    )
    .bind(job.organization_id)
    .bind(job.team_id)
    .bind(job.project_id)
    .bind(job.id)
    .bind(job.attempts)
    .bind(&job.worker_id)
    .bind(if failure_state.status == "poisoned" {
        "poisoned"
    } else {
        "failed"
    })
    .bind(error_message)
    .bind(job.claimed_at)
    .execute(&mut **tx)
    .await?;

    Ok(failure_state.status)
}

pub async fn mark_session_replay_ready(
    tx: &mut Transaction<'_, Postgres>,
    context: &RepositoryContext,
    session_id: Uuid,
    metadata: &Value,
) -> Result<(), DbError> {
    sqlx::query(
        r#"
        update public.session_replays
        set
          status = 'ready'::public.session_replay_status,
          prepared_at = now(),
          metadata = public.session_replays.metadata || $5,
          worker_job_id = null
        where organization_id = $1
          and team_id = $2
          and ($3::uuid is null or project_id = $3)
          and session_id = $4
          and status = 'queued'::public.session_replay_status
        "#,
    )
    .bind(context.organization_id)
    .bind(context.team_id)
    .bind(context.project_id)
    .bind(session_id)
    .bind(metadata)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn load_queue_summary(
    tx: &mut Transaction<'_, Postgres>,
    context: &RepositoryContext,
) -> Result<QueueSummary, DbError> {
    let rows = sqlx::query(
        r#"
        select status::text as status, count(*)::bigint as count
        from public.worker_jobs
        where organization_id = $1
          and team_id = $2
          and ($3::uuid is null or project_id = $3)
        group by status
        "#,
    )
    .bind(context.organization_id)
    .bind(context.team_id)
    .bind(context.project_id)
    .fetch_all(&mut **tx)
    .await?;

    let mut counts = std::collections::HashMap::new();
    for row in rows {
        counts.insert(
            row.try_get::<String, _>("status")?,
            row.try_get::<i64, _>("count")?,
        );
    }
    Ok(QueueSummary {
        total: counts.values().sum(),
        pending: *counts.get("pending").unwrap_or(&0),
        running: *counts.get("running").unwrap_or(&0),
        poisoned: *counts.get("poisoned").unwrap_or(&0),
    })
}

pub async fn load_dashboard_summary(
    tx: &mut Transaction<'_, Postgres>,
    context: &RepositoryContext,
) -> Result<(i64, i64, i64, i64), DbError> {
    let row = sqlx::query(
        r#"
        select
          (select count(*)::bigint from public.memories where organization_id = $1 and team_id = $2) as total_memories,
          (select count(*)::bigint from public.sessions where organization_id = $1 and team_id = $2) as total_sessions,
          (select count(*)::bigint from public.projects where organization_id = $1 and team_id = $2) as total_projects,
          (
            select coalesce(sum(greatest(length(content) - length(summary), 0) / 4), 0)::bigint
            from public.memories
            where organization_id = $1 and team_id = $2
          ) as estimated_token_savings
        "#,
    )
    .bind(context.organization_id)
    .bind(context.team_id)
    .fetch_one(&mut **tx)
    .await?;
    Ok((
        row.try_get("total_memories")?,
        row.try_get("total_sessions")?,
        row.try_get("total_projects")?,
        row.try_get("estimated_token_savings")?,
    ))
}

pub async fn load_memory_graph_nodes(
    tx: &mut Transaction<'_, Postgres>,
    context: &RepositoryContext,
    limit: i64,
) -> Result<Vec<DashboardGraphNodeRow>, DbError> {
    sqlx::query_as::<_, DashboardGraphNodeRow>(
        r#"
        select id, title, type::text as memory_type, summary
        from public.memories
        where organization_id = $1
          and team_id = $2
          and ($3::uuid is null or project_id = $3)
        order by created_at desc
        limit $4
        "#,
    )
    .bind(context.organization_id)
    .bind(context.team_id)
    .bind(context.project_id)
    .bind(limit)
    .fetch_all(&mut **tx)
    .await
    .map_err(DbError::from)
}

pub async fn load_memory_graph_edges(
    tx: &mut Transaction<'_, Postgres>,
    context: &RepositoryContext,
    limit: i64,
) -> Result<Vec<DashboardGraphEdgeRow>, DbError> {
    sqlx::query_as::<_, DashboardGraphEdgeRow>(
        r#"
        select
          from_memory_id as source,
          to_memory_id as target,
          edge_type::text as edge_type,
          weight::float8 as weight
        from public.memory_edges
        where organization_id = $1
          and team_id = $2
          and ($3::uuid is null or project_id = $3)
        limit $4
        "#,
    )
    .bind(context.organization_id)
    .bind(context.team_id)
    .bind(context.project_id)
    .bind(limit)
    .fetch_all(&mut **tx)
    .await
    .map_err(DbError::from)
}

pub async fn load_memory_search_rows(
    tx: &mut Transaction<'_, Postgres>,
    context: &RepositoryContext,
    query: &str,
    project_id: Option<Uuid>,
    session_id: Option<Uuid>,
    provider: Option<&str>,
    branch: Option<&str>,
    from: Option<&str>,
    to: Option<&str>,
    memory_types: &[String],
    include_historical: bool,
    limit: i64,
) -> Result<Vec<MemorySearchRow>, DbError> {
    sqlx::query_as::<_, MemorySearchRow>(
        r#"
        select
          m.id,
          m.project_id,
          m.type::text as memory_type,
          m.title,
          m.content,
          m.summary,
          m.metadata,
          m.session_id,
          m.importance_score::float8 as importance_score,
          m.confidence_score::float8 as confidence_score,
          m.superseded_at,
          m.created_at,
          s.branch,
          ts_rank_cd(m.search_vector, websearch_to_tsquery('english', $4))::float8 as lexical_score,
          null::float8 as semantic_score,
          c.id as claim_id,
          c.claim_type::text as claim_type,
          c.claim_key,
          c.subject as claim_subject,
          c.predicate as claim_predicate,
          c.object as claim_object,
          c.claim_polarity,
          c.authority_class as claim_authority_class,
          c.verification_status as claim_verification_status,
          c.valid_from as claim_valid_from,
          c.valid_to as claim_valid_to,
          c.superseded_by as claim_superseded_by,
          coalesce(c.active_conflict_count, 0)::bigint as active_conflict_count,
          c.governance_state as claim_governance_state
        from public.memories m
        left join public.sessions s on s.id = m.session_id
        left join lateral (
          select
            cl.id,
            cl.claim_type,
            cl.claim_key,
            cl.subject,
            cl.predicate,
            cl.object,
            cl.claim_polarity,
            cl.authority_class,
            cl.verification_status,
            cl.valid_from,
            cl.valid_to,
            cl.superseded_by,
            cl.admitted,
            cl.governance_state,
            (
              select count(*)::bigint
              from public.claim_edges ce
              join public.claims related on related.id = case
                when ce.from_claim_id = cl.id then ce.to_claim_id
                else ce.from_claim_id
              end
              where (ce.from_claim_id = cl.id or ce.to_claim_id = cl.id)
                and ce.edge_type = 'contradicts'::public.memory_edge_type
                and related.admitted = true
                and related.superseded_by is null
                and related.valid_to is null
            ) as active_conflict_count
          from public.claims cl
          where cl.memory_id = m.id
          order by
            (cl.admitted = true and cl.superseded_by is null and cl.valid_to is null)::int desc,
            cl.valid_from desc nulls last,
            cl.created_at desc
          limit 1
        ) c on true
        where m.organization_id = $1
          and m.team_id = $2
          and ($3::uuid is null or m.project_id = $3)
          and ($5::uuid is null or m.project_id = $5)
          and ($6::uuid is null or m.session_id = $6)
          and ($7::text is null or s.provider::text = $7)
          and ($8::text is null or s.branch = $8)
          and ($9::timestamptz is null or m.created_at >= $9::timestamptz)
          and ($10::timestamptz is null or m.created_at <= $10::timestamptz)
          and (cardinality($11::text[]) = 0 or m.type::text = any($11))
          -- v2.2.2: Also filter by claim_type when type filter is specified
          and (cardinality($11::text[]) = 0 or c.id is null or c.claim_type::text = any($11))
          and (
            $13::boolean
            or c.id is null
            or (
              c.admitted = true
              and c.superseded_by is null
              and c.valid_to is null
              and c.verification_status <> 'contradicted'
              and coalesce(c.governance_state, 'active') not in ('archived', 'rejected')
            )
          )
          and m.search_vector @@ websearch_to_tsquery('english', $4)
        order by lexical_score desc, m.created_at desc
        limit $12
        "#,
    )
    .bind(context.organization_id)
    .bind(context.team_id)
    .bind(context.project_id)
    .bind(query)
    .bind(project_id)
    .bind(session_id)
    .bind(provider)
    .bind(branch)
    .bind(from)
    .bind(to)
    .bind(memory_types)
    .bind(limit)
    .bind(include_historical)
    .fetch_all(&mut **tx)
    .await
    .map_err(DbError::from)
}

pub async fn load_memory_provenance(
    tx: &mut Transaction<'_, Postgres>,
    memory_ids: &[Uuid],
    limit_per_memory: i64,
) -> Result<Vec<MemoryProvenanceRow>, DbError> {
    if memory_ids.is_empty() {
        return Ok(Vec::new());
    }

    sqlx::query_as::<_, MemoryProvenanceRow>(
        r#"
        with ranked as (
          select
            mp.memory_id,
            se.session_id,
            mp.session_event_id,
            mp.excerpt,
            row_number() over (partition by mp.memory_id order by mp.created_at asc) as rn
          from public.memory_provenance mp
          join public.session_events se on se.id = mp.session_event_id
          where mp.memory_id = any($1)
        )
        select memory_id, session_id, session_event_id, excerpt
        from ranked
        where rn <= $2
        order by memory_id, rn asc
        "#,
    )
    .bind(memory_ids)
    .bind(limit_per_memory)
    .fetch_all(&mut **tx)
    .await
    .map_err(DbError::from)
}

pub async fn load_session_graph_weights(
    tx: &mut Transaction<'_, Postgres>,
    context: &RepositoryContext,
    session_id: Uuid,
    candidate_session_ids: &[Uuid],
) -> Result<Vec<(Uuid, f64)>, DbError> {
    if candidate_session_ids.is_empty() {
        return Ok(Vec::new());
    }

    let rows = sqlx::query(
        r#"
        select
          case when from_session_id = $1 then to_session_id else from_session_id end as linked_session_id,
          max(weight)::float8 as max_weight
        from public.session_edges
        where (
            (from_session_id = $1 and to_session_id = any($4))
            or (to_session_id = $1 and from_session_id = any($4))
          )
          and organization_id = $2
          and team_id = $3
          and ($5::uuid is null or project_id = $5)
        group by linked_session_id
        "#,
    )
    .bind(session_id)
    .bind(context.organization_id)
    .bind(context.team_id)
    .bind(candidate_session_ids)
    .bind(context.project_id)
    .fetch_all(&mut **tx)
    .await?;

    rows.into_iter()
        .map(|row| {
            Ok((
                row.try_get("linked_session_id")?,
                row.try_get("max_weight")?,
            ))
        })
        .collect()
}

pub async fn load_memory(
    tx: &mut Transaction<'_, Postgres>,
    context: &RepositoryContext,
    memory_id: Uuid,
) -> Result<Option<MemoryDetailRow>, DbError> {
    sqlx::query_as::<_, MemoryDetailRow>(
        r#"
        select
          m.id,
          m.project_id,
          m.type::text as memory_type,
          m.title,
          m.content,
          m.summary,
          m.metadata,
          m.created_at,
          c.id as claim_id,
          c.claim_type::text as claim_type,
          c.claim_key,
          c.subject as claim_subject,
          c.predicate as claim_predicate,
          c.object as claim_object,
          c.claim_polarity,
          c.authority_class as claim_authority_class,
          c.verification_status as claim_verification_status,
          c.valid_from as claim_valid_from,
          c.valid_to as claim_valid_to,
          c.superseded_by as claim_superseded_by,
          coalesce((
            select count(*)::bigint
            from public.claim_edges ce
            join public.claims related on related.id = case
              when ce.from_claim_id = c.id then ce.to_claim_id
              else ce.from_claim_id
            end
            where c.id is not null
              and (ce.from_claim_id = c.id or ce.to_claim_id = c.id)
              and ce.edge_type = 'contradicts'::public.memory_edge_type
              and related.admitted = true
              and related.superseded_by is null
              and related.valid_to is null
          ), 0)::bigint as active_conflict_count,
          c.governance_state as claim_governance_state
        from public.memories m
        left join public.claims c on c.memory_id = m.id
        where m.id = $1
          and m.organization_id = $2
          and m.team_id = $3
          and ($4::uuid is null or m.project_id = $4)
        limit 1
        "#,
    )
    .bind(memory_id)
    .bind(context.organization_id)
    .bind(context.team_id)
    .bind(context.project_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(DbError::from)
}

pub async fn load_memories_batch(
    tx: &mut Transaction<'_, Postgres>,
    context: &RepositoryContext,
    ids: &[Uuid],
) -> Result<Vec<MemoryDetailRow>, DbError> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }

    sqlx::query_as::<_, MemoryDetailRow>(
        r#"
        select
          m.id,
          m.project_id,
          m.type::text as memory_type,
          m.title,
          m.content,
          m.summary,
          m.metadata,
          m.created_at,
          c.id as claim_id,
          c.claim_type::text as claim_type,
          c.claim_key,
          c.subject as claim_subject,
          c.predicate as claim_predicate,
          c.object as claim_object,
          c.claim_polarity,
          c.authority_class as claim_authority_class,
          c.verification_status as claim_verification_status,
          c.valid_from as claim_valid_from,
          c.valid_to as claim_valid_to,
          c.superseded_by as claim_superseded_by,
          coalesce((
            select count(*)::bigint
            from public.claim_edges ce
            join public.claims related on related.id = case
              when ce.from_claim_id = c.id then ce.to_claim_id
              else ce.from_claim_id
            end
            where c.id is not null
              and (ce.from_claim_id = c.id or ce.to_claim_id = c.id)
              and ce.edge_type = 'contradicts'::public.memory_edge_type
              and related.admitted = true
              and related.superseded_by is null
              and related.valid_to is null
          ), 0)::bigint as active_conflict_count,
          c.governance_state as claim_governance_state
        from public.memories m
        left join public.claims c on c.memory_id = m.id
        where m.id = any($1)
          and m.organization_id = $2
          and m.team_id = $3
          and ($4::uuid is null or m.project_id = $4)
        order by m.created_at desc
        "#,
    )
    .bind(ids)
    .bind(context.organization_id)
    .bind(context.team_id)
    .bind(context.project_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(DbError::from)
}

pub async fn load_memory_edges_for_ids(
    tx: &mut Transaction<'_, Postgres>,
    ids: &[Uuid],
) -> Result<Vec<(Uuid, Uuid)>, DbError> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }

    let rows = sqlx::query(
        r#"
        select from_memory_id, to_memory_id
        from public.memory_edges
        where from_memory_id = any($1) or to_memory_id = any($1)
        limit 400
        "#,
    )
    .bind(ids)
    .fetch_all(&mut **tx)
    .await?;

    rows.into_iter()
        .map(|row| Ok((row.try_get("from_memory_id")?, row.try_get("to_memory_id")?)))
        .collect()
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PckcMemoryEdgeRow {
    pub from_memory_id: Uuid,
    pub to_memory_id: Uuid,
    pub edge_type: String,
    pub weight: f64,
    pub metadata: Value,
}

/// Load PCKC relationship edges (supersedes, contradicts, confirms) from memory_edges for a project.
/// These are the edges written by claim reconciliation during session/end.
pub async fn load_pckc_memory_edges(
    tx: &mut Transaction<'_, Postgres>,
    context: &RepositoryContext,
    project_id: Uuid,
) -> Result<Vec<PckcMemoryEdgeRow>, DbError> {
    // Limit to 20K rows to avoid OOM on large projects (477K+ edges).
    // The worker caps injection further; this limit avoids loading hundreds
    // of thousands of rows into memory just to discard most of them.
    sqlx::query_as::<_, PckcMemoryEdgeRow>(
        r#"
        select
          from_memory_id,
          to_memory_id,
          edge_type::text as edge_type,
          weight::float8 as weight,
          metadata
        from public.memory_edges
        where organization_id = $1
          and team_id = $2
          and project_id = $3
          and edge_type in (
            'supersedes'::public.memory_edge_type,
            'contradicts'::public.memory_edge_type,
            'confirms'::public.memory_edge_type
          )
        order by weight desc
        limit 20000
        "#,
    )
    .bind(context.organization_id)
    .bind(context.team_id)
    .bind(project_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(DbError::from)
}

pub async fn load_memories_for_chroma(
    tx: &mut Transaction<'_, Postgres>,
    context: &RepositoryContext,
    project_id: Uuid,
) -> Result<Vec<MemorySearchRow>, DbError> {
    load_memories_for_chroma_scoped(tx, context, project_id, None).await
}

/// Load memories for Chroma sync, optionally scoped to a single session.
/// When `session_id` is Some, only that session's memories are returned (fast).
/// When None, all project memories up to 25K are returned (initial bulk sync).
pub async fn load_memories_for_chroma_scoped(
    tx: &mut Transaction<'_, Postgres>,
    context: &RepositoryContext,
    project_id: Uuid,
    session_id: Option<Uuid>,
) -> Result<Vec<MemorySearchRow>, DbError> {
    sqlx::query_as::<_, MemorySearchRow>(
        r#"
        select
          m.id,
          m.project_id,
          m.type::text as memory_type,
          m.title,
          m.content,
          m.summary,
          m.metadata,
          m.session_id,
          m.importance_score::float8 as importance_score,
          m.confidence_score::float8 as confidence_score,
          m.superseded_at,
          m.created_at,
          s.branch,
          null::float8 as lexical_score,
          null::float8 as semantic_score,
          null::uuid as claim_id,
          null::text as claim_type,
          null::text as claim_key,
          null::text as claim_subject,
          null::text as claim_predicate,
          null::text as claim_object,
          null::boolean as claim_polarity,
          null::text as claim_authority_class,
          null::text as claim_verification_status,
          null::timestamptz as claim_valid_from,
          null::timestamptz as claim_valid_to,
          null::uuid as claim_superseded_by,
          0::bigint as active_conflict_count,
          null::text as claim_governance_state
        from public.memories m
        left join public.sessions s on s.id = m.session_id
        where m.organization_id = $1
          and m.team_id = $2
          and m.project_id = $3
          and ($4::uuid is null or m.session_id = $4)
        order by m.created_at desc
        limit 25000
        "#,
    )
    .bind(context.organization_id)
    .bind(context.team_id)
    .bind(project_id)
    .bind(session_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(DbError::from)
}

pub async fn load_claim_proofs(
    tx: &mut Transaction<'_, Postgres>,
    memory_ids: &[Uuid],
) -> Result<Vec<ClaimProofRow>, DbError> {
    if memory_ids.is_empty() {
        return Ok(Vec::new());
    }

    sqlx::query_as::<_, ClaimProofRow>(
        r#"
        select
          cp.memory_id,
          cp.proof_type,
          cp.source_ref,
          cp.excerpt,
          cp.session_id,
          cp.session_event_id,
          cp.authority_class,
          cp.verification_status
        from public.claim_proofs cp
        where cp.memory_id = any($1)
        order by cp.memory_id, cp.created_at asc
        "#,
    )
    .bind(memory_ids)
    .fetch_all(&mut **tx)
    .await
    .map_err(DbError::from)
}

pub async fn load_claim_relations_for_memory_ids(
    tx: &mut Transaction<'_, Postgres>,
    memory_ids: &[Uuid],
) -> Result<Vec<ClaimRelationRow>, DbError> {
    if memory_ids.is_empty() {
        return Ok(Vec::new());
    }

    sqlx::query_as::<_, ClaimRelationRow>(
        r#"
        with base as (
          select memory_id, id as claim_id
          from public.claims
          where memory_id = any($1)
        )
        select
          base.memory_id,
          base.claim_id,
          other.id as related_claim_id,
          other.memory_id as related_memory_id,
          ce.edge_type::text as edge_type,
          case when ce.from_claim_id = base.claim_id then 'outgoing' else 'incoming' end as direction,
          related_memory.title,
          related_memory.summary,
          other.authority_class,
          other.verification_status
        from base
        join public.claim_edges ce
          on ce.from_claim_id = base.claim_id or ce.to_claim_id = base.claim_id
        join public.claims other
          on other.id = case when ce.from_claim_id = base.claim_id then ce.to_claim_id else ce.from_claim_id end
        left join public.memories related_memory on related_memory.id = other.memory_id
        order by base.memory_id, ce.created_at asc
        "#,
    )
    .bind(memory_ids)
    .fetch_all(&mut **tx)
    .await
    .map_err(DbError::from)
}

fn calculate_retry_delay_ms(attempt_number: i32) -> i32 {
    let normalized = attempt_number.max(1);
    (5_000 * 2_i32.pow((normalized - 1) as u32)).min(60_000)
}

fn next_failure_state(attempt_number: i32, max_attempts: i32) -> FailureState {
    if attempt_number >= max_attempts {
        FailureState {
            status: "poisoned".to_string(),
            delay_ms: 0,
        }
    } else {
        FailureState {
            status: "pending".to_string(),
            delay_ms: calculate_retry_delay_ms(attempt_number),
        }
    }
}

struct FailureState {
    status: String,
    delay_ms: i32,
}
