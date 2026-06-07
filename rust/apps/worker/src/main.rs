use anyhow::Context;
use chum_mem_app::{init_tracing, shutdown_signal};
use chum_mem_config::{AppConfig, VectorStoreBackend};
use chum_mem_contracts::CanonicalEventType;
use chum_mem_db::{
    Database, RepositoryContext, WorkerJobRecord, apply_repository_context, check_readiness,
    claim_next_worker_job, complete_worker_job, fail_worker_job, load_candidate_completed_sessions,
    load_pckc_memory_edges, load_session_events_limited, mark_session_replay_ready,
};
use chum_mem_pipeline::{
    KnowledgeEdge, MemoryNodeInput, SessionEventRecord, TurboVecStore, UpsertMemory,
    VectorStoreItem, assign_communities_with_budget, build_knowledge_graph,
    generate_knowledge_report, merge_graphs, to_node_link_json, to_persistable_memory_edge,
    upsert_chroma_memories_typed, vector_from_f64,
};
use reqwest::Client;
use serde_json::{Value, json};
use sqlx::{Postgres, Row, Transaction};
use std::sync::Arc;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

const WORKER_JOB_TYPES: &[&str] = &[
    "derive-session-memories",
    "reconcile-claim-state",
    "sync-chroma-index",
    "replay-failed-session",
    "build-knowledge-graph",
    "detect-communities",
    "generate-knowledge-report",
    "export-knowledge-snapshot",
];

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing("chum_mem_worker");

    let config = AppConfig::from_env().context("loading worker configuration")?;
    let db = Database::connect(&config)
        .await
        .context("connecting worker database pool")?;
    db.migrate_if_enabled(&config)
        .await
        .context("running worker startup migrations")?;

    let readiness = check_readiness(db.pool(), &config)
        .await
        .context("checking worker readiness")?;
    if !readiness.is_ready() {
        warn!("worker dependencies are not fully ready yet; continuing with polling loop");
    }

    let http_client = Client::builder()
        .build()
        .context("building shared worker HTTP client")?;
    let scope = RepositoryContext::from_config(&config);
    let worker_id = format!("worker:{}", std::process::id());
    let concurrency = config.worker_concurrency.max(1);
    let mut interval = tokio::time::interval(config.worker_poll_interval());
    let mut ticks: u64 = 0;

    // Wrap shared state in Arc for concurrent task access.
    let db = Arc::new(db);
    let config = Arc::new(config);
    let scope = Arc::new(scope);
    let http_client = Arc::new(http_client);
    let worker_id = Arc::new(worker_id);

    info!(
        poll_interval_ms = config.worker_poll_interval_ms,
        concurrency,
        vector_store_enabled = config.vector_store_enabled(),
        project_scoped = config.project_id.is_some(),
        organization_id = %scope.organization_id,
        team_id = %scope.team_id,
        worker_id = %worker_id,
        "starting Rust worker"
    );

    loop {
        tokio::select! {
            _ = shutdown_signal("chum-mem-worker") => {
                info!(ticks, worker_id = %worker_id, "worker shutdown complete");
                break;
            }
            _ = interval.tick() => {
                ticks += 1;
                // Claim up to `concurrency` jobs and process them in parallel.
                loop {
                    let batch_processed = run_batch(
                        &db, &config, &scope, &worker_id, &http_client, concurrency,
                    ).await;
                    debug!(ticks, batch_processed, "worker poll tick");
                    if batch_processed == 0 {
                        break;
                    }
                }
            }
        }
    }

    Ok(())
}

/// Heavy job types that must not run concurrently — they load the full
/// project graph into memory and would OOM if multiple run at once.
const HEAVY_JOB_TYPES: &[&str] = &["build-knowledge-graph"];

/// Claim up to `concurrency` jobs and process them concurrently.
/// Heavy jobs (graph builds) are serialized — at most one runs per batch.
/// Returns the number of jobs processed in this batch.
async fn run_batch(
    db: &Arc<Database>,
    config: &Arc<AppConfig>,
    scope: &Arc<RepositoryContext>,
    worker_id: &Arc<String>,
    http_client: &Arc<Client>,
    concurrency: usize,
) -> usize {
    // Phase 1: claim up to N jobs sequentially (claim is a single-row lock).
    let mut claimed_jobs = Vec::with_capacity(concurrency);
    let mut has_heavy = false;
    for _ in 0..concurrency {
        match claim_one_job(db, scope, worker_id).await {
            Some(job) => {
                let is_heavy = HEAVY_JOB_TYPES.contains(&job.job_type.as_str());
                // Only allow one heavy job per batch to avoid OOM.
                if is_heavy && has_heavy {
                    // Put it back — release the claim so it's picked up next tick.
                    if let Err(e) = release_job_claim(db, scope, &job).await {
                        warn!(job_id = %job.id, error = %e, "failed to release excess heavy job claim");
                    }
                    continue;
                }
                if is_heavy {
                    has_heavy = true;
                }
                claimed_jobs.push(job);
            }
            None => break, // no more pending jobs
        }
    }

    if claimed_jobs.is_empty() {
        return 0;
    }

    let batch_size = claimed_jobs.len();

    // Phase 2: split into heavy (serial) and light (concurrent) jobs.
    let mut heavy_jobs = Vec::new();
    let mut light_jobs = Vec::new();
    for job in claimed_jobs {
        if HEAVY_JOB_TYPES.contains(&job.job_type.as_str()) {
            heavy_jobs.push(job);
        } else {
            light_jobs.push(job);
        }
    }

    // Run light jobs concurrently.
    let mut handles = Vec::with_capacity(light_jobs.len());
    for job in light_jobs {
        let db = Arc::clone(db);
        let config = Arc::clone(config);
        let scope = Arc::clone(scope);
        let worker_id = Arc::clone(worker_id);
        let http_client = Arc::clone(http_client);

        handles.push(tokio::spawn(async move {
            run_single_job(&db, &config, &scope, &worker_id, &http_client, job).await;
        }));
    }

    // Run heavy jobs serially (at most 1, but future-proof).
    for job in heavy_jobs {
        run_single_job(db, config, scope, worker_id, http_client, job).await;
    }

    // Wait for all light jobs to finish.
    for handle in handles {
        let _ = handle.await;
    }

    batch_size
}

/// Release a claimed job back to pending so it can be picked up later.
async fn release_job_claim(
    db: &Database,
    scope: &RepositoryContext,
    job: &WorkerJobRecord,
) -> Result<(), String> {
    let mut tx = db.pool().begin().await.map_err(|e| e.to_string())?;
    apply_repository_context(&mut *tx, scope)
        .await
        .map_err(|e| e.to_string())?;
    sqlx::query(
        "UPDATE public.worker_jobs SET status = 'pending'::public.worker_job_status, \
         worker_id = NULL, claimed_at = NULL, updated_at = now() WHERE id = $1",
    )
    .bind(job.id)
    .execute(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;
    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(())
}

/// Claim a single job from the queue. Returns None if no jobs are available.
async fn claim_one_job(
    db: &Database,
    scope: &RepositoryContext,
    worker_id: &str,
) -> Option<WorkerJobRecord> {
    let mut tx = match db.pool().begin().await {
        Ok(tx) => tx,
        Err(error) => {
            error!(error = %error, "failed to begin worker transaction");
            return None;
        }
    };
    if let Err(error) = apply_repository_context(&mut *tx, scope).await {
        error!(error = %error, "failed to apply worker repository context");
        return None;
    }

    let job = match claim_next_worker_job(&mut tx, scope, worker_id, WORKER_JOB_TYPES).await {
        Ok(job) => job,
        Err(error) => {
            error!(error = %error, "failed to claim worker job");
            return None;
        }
    };
    if let Err(error) = tx.commit().await {
        error!(error = %error, "failed to commit worker claim transaction");
        return None;
    }
    job
}

/// Process a single claimed job end-to-end: execute + settle success/failure.
async fn run_single_job(
    db: &Database,
    config: &AppConfig,
    scope: &RepositoryContext,
    worker_id: &str,
    http_client: &Client,
    job: WorkerJobRecord,
) {
    match process_worker_job(db, config, scope, worker_id, http_client, &job).await {
        Ok(()) => {
            if let Err(error) = settle_success(db, scope, &job).await {
                error!(error = %error, job_id = %job.id, "failed to settle worker success");
            } else {
                info!(job_id = %job.id, job_type = %job.job_type, "worker job completed");
            }
        }
        Err(error_message) => {
            if let Err(error) = settle_failure(db, scope, &job, &error_message).await {
                error!(error = %error, job_id = %job.id, "failed to settle worker failure");
            } else {
                error!(
                    job_id = %job.id,
                    job_type = %job.job_type,
                    error = %error_message,
                    "worker job failed"
                );
            }
        }
    }
}

async fn process_worker_job(
    db: &Database,
    config: &AppConfig,
    scope: &RepositoryContext,
    worker_id: &str,
    http_client: &Client,
    job: &WorkerJobRecord,
) -> Result<(), String> {
    match job.job_type.as_str() {
        "derive-session-memories" => {
            derive_session_memories_job(db, config, scope, http_client, job).await
        }
        "reconcile-claim-state" => reconcile_claim_state_job(db, scope, job).await,
        "sync-chroma-index" => sync_chroma_index(db, config, scope, http_client, job).await,
        "replay-failed-session" => prepare_session_replay(db, scope, worker_id, job).await,
        "build-knowledge-graph" => {
            build_knowledge_graph_job_with_dedup(db, config, scope, job).await
        }
        "detect-communities" | "generate-knowledge-report" | "export-knowledge-snapshot" => Ok(()),
        other => Err(format!("Unsupported worker job type: {other}")),
    }
}

/// Async claim reconciliation job.
///
/// Consumes a `{projectId, sessionId, newClaimIds}` payload enqueued by
/// `derive_and_persist_session_memories` at session_end time. Runs in bounded
/// sub-transactions under a per-project xact-scoped advisory lock so the
/// per-xact lock budget stays O(chunk_size), not O(session_drafts).
///
/// Idempotency: each chunk's sub-transaction commits independently. A crash
/// mid-job leaves the reconciliation partially applied; on retry the worker
/// re-runs the SAME payload — `upsert_memory_edge`/`upsert_claim_edge` are
/// ON CONFLICT DO UPDATE and `mark_memory_superseded`/`mark_claim_superseded`
/// are idempotent (WHERE superseded_by IS NULL / superseded_at IS NULL).
async fn reconcile_claim_state_job(
    db: &Database,
    scope: &RepositoryContext,
    job: &WorkerJobRecord,
) -> Result<(), String> {
    let scoped = RepositoryContext {
        project_id: Some(job.project_id),
        ..scope.clone()
    };
    let claim_ids: Vec<Uuid> = job
        .payload
        .get("newClaimIds")
        .and_then(Value::as_array)
        .map(|array| {
            array
                .iter()
                .filter_map(|value| value.as_str().and_then(|s| Uuid::parse_str(s).ok()))
                .collect()
        })
        .unwrap_or_default();

    if claim_ids.is_empty() {
        return Ok(());
    }

    let advisory_key = chum_mem_db::reconcile::reconcile_project_advisory_key(job.project_id);

    let chunk_size = chum_mem_db::reconcile::RECONCILE_CHUNK_SIZE;
    let mut totals = chum_mem_db::reconcile::ReconcileOutcome::default();
    for chunk in claim_ids.chunks(chunk_size) {
        let mut tx = db.pool().begin().await.map_err(|error| error.to_string())?;
        apply_repository_context(&mut *tx, &scoped)
            .await
            .map_err(|error| error.to_string())?;
        // Per-chunk, per-project advisory lock: one slot, not one per claim.
        sqlx::query("select pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind(&advisory_key)
            .execute(&mut *tx)
            .await
            .map_err(|error| error.to_string())?;
        let outcome = chum_mem_db::reconcile::reconcile_claim_state_for_claims(
            &mut tx,
            &scoped,
            job.project_id,
            chunk,
        )
        .await
        .map_err(|error| error.to_string())?;
        tx.commit().await.map_err(|error| error.to_string())?;
        totals.merge(outcome);
    }

    info!(
        job_id = %job.id,
        project_id = %job.project_id,
        claims_processed = totals.claims_processed,
        supersedes_edges = totals.supersedes_edges,
        contradicts_edges = totals.contradicts_edges,
        confirms_edges = totals.confirms_edges,
        "reconcile-claim-state completed"
    );
    Ok(())
}

async fn derive_session_memories_job(
    _db: &Database,
    config: &AppConfig,
    _scope: &RepositoryContext,
    http_client: &Client,
    job: &WorkerJobRecord,
) -> Result<(), String> {
    let session_id = job
        .session_id
        .ok_or("derive-session-memories job missing session_id")?;
    let summary = job.payload.get("summary").and_then(Value::as_str);
    let metadata = job.payload.get("metadata").cloned().unwrap_or(json!({}));
    let body = json!({
        "sessionId": session_id,
        "summary": summary,
        "metadata": metadata,
        "defer": false,
    });
    let url = format!("{}v1/ingest/session/end", config.dashboard_api_url.as_str());
    let response = http_client
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|error| format!("derive-session-memories HTTP request failed: {error}"))?;
    if !response.status().is_success() {
        let status = response.status();
        let text = response
            .text()
            .await
            .unwrap_or_else(|_| "<no body>".to_string());
        return Err(format!(
            "derive-session-memories API returned {status}: {text}"
        ));
    }
    info!(
        session_id = %session_id,
        "derive-session-memories completed via API callback"
    );
    Ok(())
}

async fn sync_chroma_index(
    db: &Database,
    config: &AppConfig,
    scope: &RepositoryContext,
    http_client: &Client,
    job: &WorkerJobRecord,
) -> Result<(), String> {
    if !config.vector_store_enabled() {
        return Ok(());
    }
    let session_id = job.session_id;
    let scoped = RepositoryContext {
        project_id: Some(job.project_id),
        ..scope.clone()
    };

    // Bulk-complete sibling sync jobs for the same project — they'd each
    // re-sync the same session memories redundantly.
    let deduped =
        bulk_complete_sibling_jobs(db, scope, job.project_id, job.id, "sync-chroma-index").await;
    if deduped > 0 {
        info!(
            project_id = %job.project_id,
            deduped_count = deduped,
            "bulk-completed redundant sync-chroma-index jobs"
        );
    }

    let mut tx = db.pool().begin().await.map_err(|error| error.to_string())?;
    apply_repository_context(&mut *tx, &scoped)
        .await
        .map_err(|error| error.to_string())?;
    // Scope to session when available (fast path: ~50-200 memories).
    // Falls back to full project sync when session_id is absent.
    let memories =
        chum_mem_db::load_memories_for_chroma_scoped(&mut tx, &scoped, job.project_id, session_id)
            .await
            .map_err(|error| error.to_string())?;
    tx.commit().await.map_err(|error| error.to_string())?;

    info!(
        project_id = %job.project_id,
        session_id = ?session_id,
        memories_count = memories.len(),
        backend = ?config.vector_store_backend,
        "syncing memories to configured vector store"
    );

    let payload = memories
        .into_iter()
        .map(|memory| UpsertMemory {
            id: memory.id,
            document: format!("{}\n{}", memory.title, memory.summary),
            metadata: serde_json::json!({
                "title": memory.title,
                "summary": memory.summary,
                "type": memory.memory_type,
                "projectId": memory.project_id,
                "createdAt": memory.created_at,
                "sessionId": memory.session_id,
                "branch": memory.branch,
                "importanceScore": memory.importance_score,
                "confidenceScore": memory.confidence_score,
            }),
        })
        .collect::<Vec<_>>();

    match config.vector_store_backend {
        VectorStoreBackend::Chroma => {
            let Some(chroma_url) = config.chroma_url.as_ref().map(|value| value.as_str()) else {
                return Ok(());
            };
            // v2.2.2 §3.3: Fan out to per-type partitions in addition to the
            // all-types collection so typed mem_search can hit a narrow index.
            upsert_chroma_memories_typed(
                http_client,
                chroma_url,
                &config.chroma_collection,
                &payload,
            )
            .await
            .map_err(|error| error.to_string())
        }
        VectorStoreBackend::TurboVec => {
            let Some(root) = config.turbovec_path.as_ref() else {
                return Ok(());
            };
            let store =
                TurboVecStore::new(root, &config.chroma_collection, config.turbovec_bit_width);
            let items = payload
                .into_iter()
                .map(|memory| {
                    let vector = vector_from_f64(&chum_mem_pipeline::embed_text(&memory.document))
                        .map_err(|error| error.to_string())?;
                    Ok(VectorStoreItem {
                        id: memory.id,
                        vector,
                        document: memory.document,
                        metadata: memory.metadata,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            store
                .upsert_typed(&items)
                .map_err(|error| error.to_string())
        }
    }
}

/// Bulk-complete all pending jobs of a given type for a project,
/// except the one we're processing. Used for sync-chroma-index dedup.
async fn bulk_complete_sibling_jobs(
    db: &Database,
    scope: &RepositoryContext,
    project_id: Uuid,
    exclude_job_id: Uuid,
    job_type: &str,
) -> i64 {
    let result = async {
        let mut tx = db.pool().begin().await.map_err(|e| e.to_string())?;
        let scoped = RepositoryContext {
            project_id: Some(project_id),
            ..scope.clone()
        };
        apply_repository_context(&mut *tx, &scoped)
            .await
            .map_err(|e| e.to_string())?;
        let rows = sqlx::query_scalar::<_, Uuid>(
            r#"
            with batch as (
              select id
              from public.worker_jobs
              where project_id = $1
                and job_type = $3
                and status = 'pending'::public.worker_job_status
                and id != $2
              for update skip locked
            )
            update public.worker_jobs as j
            set status = 'completed'::public.worker_job_status,
                completed_at = now(),
                updated_at = now(),
                worker_id = 'dedup-collapsed'
            from batch
            where j.id = batch.id
            returning j.id
            "#,
        )
        .bind(project_id)
        .bind(exclude_job_id)
        .bind(job_type)
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
        let count = rows.len() as i64;
        tx.commit().await.map_err(|e| e.to_string())?;
        Ok::<i64, String>(count)
    }
    .await;

    match result {
        Ok(count) => count,
        Err(error) => {
            warn!(error = %error, "failed to bulk-complete sibling jobs (non-fatal)");
            0
        }
    }
}

async fn prepare_session_replay(
    db: &Database,
    scope: &RepositoryContext,
    worker_id: &str,
    job: &WorkerJobRecord,
) -> Result<(), String> {
    let session_id = job
        .session_id
        .ok_or_else(|| format!("Replay job {} missing session_id", job.id))?;
    let scoped = RepositoryContext {
        project_id: Some(job.project_id),
        ..scope.clone()
    };
    let mut tx = db.pool().begin().await.map_err(|error| error.to_string())?;
    apply_repository_context(&mut *tx, &scoped)
        .await
        .map_err(|error| error.to_string())?;
    mark_session_replay_ready(
        &mut tx,
        &scoped,
        session_id,
        &serde_json::json!({
            "preparedBy": worker_id,
            "preparedAt": time::OffsetDateTime::now_utc(),
            "jobId": job.id,
        }),
    )
    .await
    .map_err(|error| error.to_string())?;
    tx.commit().await.map_err(|error| error.to_string())
}

/// Build a knowledge graph for a session, batch-merging any other pending jobs
/// for the same project to avoid O(n²) load→merge→persist cycles.
///
/// Strategy: claim all pending `build-knowledge-graph` jobs for this project,
/// build each session's graph, merge them all with the existing snapshot, and
/// persist once. Each session's unique nodes/edges are preserved.
async fn build_knowledge_graph_job_with_dedup(
    db: &Database,
    config: &AppConfig,
    scope: &RepositoryContext,
    job: &WorkerJobRecord,
) -> Result<(), String> {
    // Claim sibling pending jobs for the same project (lock them so no other
    // worker picks them up). We'll process them all in this single pass.
    let sibling_jobs = claim_sibling_graph_jobs(db, scope, job.project_id, job.id).await;
    if !sibling_jobs.is_empty() {
        info!(
            project_id = %job.project_id,
            batch_size = sibling_jobs.len() + 1,
            "batch-merging build-knowledge-graph jobs"
        );
    }

    // Build graphs for all sessions (current job + siblings) and batch-merge.
    build_knowledge_graph_job_batched(db, config, scope, job, &sibling_jobs).await
}

/// Claim and lock all pending `build-knowledge-graph` jobs for a project,
/// returning their session IDs and job IDs. The jobs are marked completed
/// with worker_id 'batch-merged'.
async fn claim_sibling_graph_jobs(
    db: &Database,
    scope: &RepositoryContext,
    project_id: Uuid,
    exclude_job_id: Uuid,
) -> Vec<(Uuid, Uuid)> {
    let result = async {
        let mut tx = db.pool().begin().await.map_err(|e| e.to_string())?;
        let scoped = RepositoryContext {
            project_id: Some(project_id),
            ..scope.clone()
        };
        apply_repository_context(&mut *tx, &scoped)
            .await
            .map_err(|e| e.to_string())?;
        let rows = sqlx::query(
            r#"
            with batch as (
              select id
              from public.worker_jobs
              where project_id = $1
                and job_type = 'build-knowledge-graph'
                and status = 'pending'::public.worker_job_status
                and id != $2
              for update skip locked
            )
            update public.worker_jobs as j
            set status = 'completed'::public.worker_job_status,
                completed_at = now(),
                updated_at = now(),
                worker_id = 'batch-merged'
            from batch
            where j.id = batch.id
            returning j.id, j.session_id, j.payload
            "#,
        )
        .bind(project_id)
        .bind(exclude_job_id)
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
        let pairs: Vec<(Uuid, Uuid)> = rows
            .iter()
            .filter_map(|row| {
                let job_id: Uuid = row.get("id");
                // Extract session_id from column or payload (same as resolve_job_session_id)
                let session_id: Option<Uuid> = row.get("session_id");
                if let Some(sid) = session_id {
                    return Some((job_id, sid));
                }
                let payload: Value = row.get("payload");
                payload
                    .get("sessionId")
                    .and_then(Value::as_str)
                    .and_then(|s| Uuid::parse_str(s).ok())
                    .map(|sid| (job_id, sid))
            })
            .collect();
        tx.commit().await.map_err(|e| e.to_string())?;
        Ok::<Vec<(Uuid, Uuid)>, String>(pairs)
    }
    .await;

    match result {
        Ok(pairs) => pairs,
        Err(error) => {
            warn!(error = %error, "failed to claim sibling graph jobs (non-fatal, will process single)");
            Vec::new()
        }
    }
}

/// Build knowledge graphs for the primary session + all claimed siblings, then
/// merge everything with the existing snapshot and persist once.
async fn build_knowledge_graph_job_batched(
    db: &Database,
    config: &AppConfig,
    scope: &RepositoryContext,
    job: &WorkerJobRecord,
    sibling_jobs: &[(Uuid, Uuid)], // (job_id, session_id)
) -> Result<(), String> {
    let primary_session_id = resolve_job_session_id(job)?;
    let scoped = RepositoryContext {
        project_id: Some(job.project_id),
        ..scope.clone()
    };
    let mut tx = db.pool().begin().await.map_err(|error| error.to_string())?;
    apply_repository_context(&mut *tx, &scoped)
        .await
        .map_err(|error| error.to_string())?;

    // Acquire advisory lock BEFORE reading existing graph (prevents TOCTOU race)
    let lock_key = {
        let s = job.project_id.to_string();
        s.chars().fold(0_i32, |h, c| {
            ((h << 5).wrapping_sub(h)).wrapping_add(c as i32)
        }) as i64
    };
    sqlx::query("select pg_advisory_xact_lock($1)")
        .bind(lock_key)
        .execute(&mut *tx)
        .await
        .map_err(|error| error.to_string())?;

    // Load existing session snapshot once
    let existing = load_latest_knowledge_graph(&mut tx, scope, job.project_id, Some("session"))
        .await
        .map_err(|error| error.to_string())?;

    // Collect all session IDs to process (primary + siblings, deduplicated)
    let mut all_session_ids = vec![primary_session_id];
    for &(_, sid) in sibling_jobs {
        if !all_session_ids.contains(&sid) {
            all_session_ids.push(sid);
        }
    }

    // Build a graph for each session and merge incrementally
    let mut accumulated = existing;
    for &session_id in &all_session_ids {
        let session_graph = build_session_graph(&mut tx, scope, job.project_id, session_id).await?;
        accumulated = Some(match accumulated {
            Some(base) => merge_graphs(&base, &session_graph),
            None => session_graph,
        });
    }

    let raw_merged = accumulated.unwrap(); // At least one session always produces a graph

    info!(
        project_id = %job.project_id,
        sessions_merged = all_session_ids.len(),
        nodes = raw_merged.nodes.len(),
        edges = raw_merged.edges.len(),
        "batch-merged session knowledge graphs"
    );

    // Inject PCKC claim relationship edges (supersedes, contradicts, confirms).
    // Budget is additive: inject up to PCKC_EDGE_BUDGET edges on top of the
    // structural graph, regardless of how many structural edges already exist.
    const PCKC_EDGE_BUDGET: usize = 15_000;
    let pckc_scoped = RepositoryContext {
        project_id: Some(job.project_id),
        ..scope.clone()
    };
    let pckc_edges = load_pckc_memory_edges(&mut tx, &pckc_scoped, job.project_id)
        .await
        .map_err(|error| error.to_string())?;
    let mut with_pckc = raw_merged;
    let existing_node_ids: std::collections::HashSet<&str> =
        with_pckc.nodes.iter().map(|n| n.id.as_str()).collect();
    let mut injected = 0usize;
    for row in &pckc_edges {
        if injected >= PCKC_EDGE_BUDGET {
            break;
        }
        let source = format!("memory:{}", row.from_memory_id);
        let target = format!("memory:{}", row.to_memory_id);
        if existing_node_ids.contains(source.as_str())
            && existing_node_ids.contains(target.as_str())
        {
            with_pckc.edges.push(KnowledgeEdge {
                source,
                target,
                relation: row.edge_type.clone(),
                evidence: "extracted".to_string(),
                weight: row.weight,
                source_file: None,
                metadata: row.metadata.clone(),
            });
            injected += 1;
        }
    }
    if !pckc_edges.is_empty() {
        info!(
            pckc_edges_total = pckc_edges.len(),
            pckc_edges_injected = injected,
            pckc_edges_budget = PCKC_EDGE_BUDGET,
            pckc_edges_capped = injected >= PCKC_EDGE_BUDGET,
            "injected PCKC claim relationship edges into knowledge graph"
        );
    }

    // Assign communities only once on the final merged graph
    let merged = assign_communities_with_budget(
        &with_pckc,
        config.knowledge_graph_max_cluster_nodes as usize,
        config.knowledge_graph_max_cluster_edges as usize,
    );
    persist_knowledge_graph(&mut tx, scope, job.project_id, &merged, "session")
        .await
        .map_err(|error| error.to_string())?;
    tx.commit().await.map_err(|error| error.to_string())?;
    Ok(())
}

/// Build a knowledge graph for a single session (no merge, no persist).
async fn build_session_graph(
    tx: &mut Transaction<'_, Postgres>,
    scope: &RepositoryContext,
    project_id: Uuid,
    session_id: Uuid,
) -> Result<chum_mem_pipeline::KnowledgeGraph, String> {
    let scoped = RepositoryContext {
        project_id: Some(project_id),
        ..scope.clone()
    };
    let event_rows = load_session_events_limited(tx, session_id, Some(1000))
        .await
        .map_err(|error| error.to_string())?;
    let events = event_rows
        .iter()
        .map(map_session_event_record)
        .collect::<Vec<_>>();
    let episodes = load_session_episode_rows(tx, session_id)
        .await
        .map_err(|error| error.to_string())?;
    let memories = load_session_memory_rows(tx, scope, project_id, session_id)
        .await
        .map_err(|error| error.to_string())?;
    let prior_session_ids = load_candidate_completed_sessions(tx, &scoped, project_id, session_id)
        .await
        .map_err(|error| error.to_string())?
        .into_iter()
        .filter_map(|candidate| {
            if candidate.id == session_id {
                None
            } else {
                Some(candidate.id)
            }
        })
        .collect::<Vec<_>>();

    Ok(build_knowledge_graph(
        project_id,
        session_id,
        &events,
        &episodes,
        &memories,
        &prior_session_ids,
    ))
}

fn resolve_job_session_id(job: &WorkerJobRecord) -> Result<Uuid, String> {
    if let Some(session_id) = job.session_id {
        return Ok(session_id);
    }
    let payload = &job.payload;
    let session_id = payload
        .get("sessionId")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("Job {} missing sessionId", job.id))?;
    Uuid::parse_str(session_id).map_err(|error| error.to_string())
}

fn map_session_event_record(row: &chum_mem_db::SessionEventRow) -> SessionEventRecord {
    SessionEventRecord {
        id: row.id,
        event_type: match row.event_type.as_str() {
            "prompt" => CanonicalEventType::Prompt,
            "response" => CanonicalEventType::Response,
            "tool_call" => CanonicalEventType::ToolCall,
            "tool_result" => CanonicalEventType::ToolResult,
            "file_change" => CanonicalEventType::FileChange,
            "command" => CanonicalEventType::Command,
            "test_result" => CanonicalEventType::TestResult,
            "summary" => CanonicalEventType::Summary,
            "error" => CanonicalEventType::Error,
            _ => CanonicalEventType::Annotation,
        },
        payload: serde_json::from_value(row.payload.clone()).unwrap_or_default(),
        created_at: row
            .created_at
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_else(|_| row.created_at.unix_timestamp().to_string()),
    }
}

async fn load_session_episode_rows(
    tx: &mut Transaction<'_, Postgres>,
    session_id: Uuid,
) -> Result<Vec<chum_mem_pipeline::SessionEpisodeDraft>, sqlx::Error> {
    let rows = sqlx::query(
        r#"
        select
          episode_ordinal,
          episode_type::text as episode_type,
          title,
          summary,
          started_at,
          ended_at,
          metadata
        from public.session_episodes
        where session_id = $1
        order by episode_ordinal asc
        "#,
    )
    .bind(session_id)
    .fetch_all(&mut **tx)
    .await?;

    rows.into_iter()
        .map(|row| {
            let started_at = row.try_get::<time::OffsetDateTime, _>("started_at")?;
            let ended_at = row.try_get::<time::OffsetDateTime, _>("ended_at")?;
            Ok(chum_mem_pipeline::SessionEpisodeDraft {
                episode_ordinal: row.try_get("episode_ordinal")?,
                episode_type: row.try_get("episode_type")?,
                title: row.try_get("title")?,
                summary: row.try_get("summary")?,
                started_at: started_at
                    .format(&time::format_description::well_known::Rfc3339)
                    .unwrap_or_else(|_| started_at.unix_timestamp().to_string()),
                ended_at: ended_at
                    .format(&time::format_description::well_known::Rfc3339)
                    .unwrap_or_else(|_| ended_at.unix_timestamp().to_string()),
                provenance_event_ids: Vec::new(),
                metadata: row.try_get("metadata")?,
            })
        })
        .collect()
}

async fn load_session_memory_rows(
    tx: &mut Transaction<'_, Postgres>,
    scope: &RepositoryContext,
    project_id: Uuid,
    session_id: Uuid,
) -> Result<Vec<MemoryNodeInput>, sqlx::Error> {
    let rows = sqlx::query(
        r#"
        select
          m.id,
          m.type::text as memory_type,
          m.title,
          m.content,
          m.summary,
          m.importance_score::float8 as importance_score,
          m.metadata
        from public.memories m
        where m.organization_id = $1
          and m.team_id = $2
          and m.project_id = $3
          and m.id in (
            select mp.memory_id
            from public.memory_provenance mp
            join public.session_events se on se.id = mp.session_event_id
            where se.session_id = $4
          )
        "#,
    )
    .bind(scope.organization_id)
    .bind(scope.team_id)
    .bind(project_id)
    .bind(session_id)
    .fetch_all(&mut **tx)
    .await?;

    rows.into_iter()
        .map(|row| {
            Ok(MemoryNodeInput {
                id: row.try_get("id")?,
                memory_type: row.try_get("memory_type")?,
                title: row.try_get("title")?,
                content: row.try_get("content")?,
                summary: row.try_get("summary")?,
                importance_score: row.try_get("importance_score")?,
                metadata: row.try_get("metadata")?,
            })
        })
        .collect()
}

async fn load_latest_knowledge_graph(
    tx: &mut Transaction<'_, Postgres>,
    scope: &RepositoryContext,
    project_id: Uuid,
    snapshot_type: Option<&str>,
) -> Result<Option<chum_mem_pipeline::KnowledgeGraph>, sqlx::Error> {
    let row = sqlx::query(
        r#"
        select snapshot
        from public.knowledge_snapshots
        where organization_id = $1
          and team_id = $2
          and project_id = $3
          and ($4::text is null or snapshot_type = $4)
        order by created_at desc
        limit 1
        "#,
    )
    .bind(scope.organization_id)
    .bind(scope.team_id)
    .bind(project_id)
    .bind(snapshot_type)
    .fetch_optional(&mut **tx)
    .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let snapshot = row.try_get::<Value, _>("snapshot")?;
    Ok(serde_json::from_value(snapshot).ok())
}

async fn persist_knowledge_graph(
    tx: &mut Transaction<'_, Postgres>,
    scope: &RepositoryContext,
    project_id: Uuid,
    graph: &chum_mem_pipeline::KnowledgeGraph,
    snapshot_type: &str,
) -> Result<(), sqlx::Error> {
    // Advisory lock already acquired by caller before loading existing graph

    let snapshot_id = sqlx::query_scalar::<_, Uuid>(
        r#"
        insert into public.knowledge_snapshots (
          organization_id, team_id, project_id, snapshot, node_count, edge_count, community_count,
          snapshot_type
        ) values ($1, $2, $3, $4, $5, $6, $7, $8)
        returning id
        "#,
    )
    .bind(scope.organization_id)
    .bind(scope.team_id)
    .bind(project_id)
    .bind(json!(graph))
    .bind(graph.statistics.node_count as i32)
    .bind(graph.statistics.edge_count as i32)
    .bind(graph.statistics.community_count as i32)
    .bind(snapshot_type)
    .fetch_one(&mut **tx)
    .await?;

    sqlx::query(
        r#"
        insert into public.knowledge_snapshot_heads (
          project_id, organization_id, team_id, snapshot_id, snapshot_type, updated_at
        ) values ($1, $2, $3, $4, $5, now())
        on conflict (project_id, organization_id, team_id, snapshot_type) do update set
          snapshot_id = excluded.snapshot_id,
          updated_at = now()
        "#,
    )
    .bind(project_id)
    .bind(scope.organization_id)
    .bind(scope.team_id)
    .bind(snapshot_id)
    .bind(snapshot_type)
    .execute(&mut **tx)
    .await?;

    sqlx::query(
        r#"
        insert into public.knowledge_snapshot_artifacts (
          snapshot_id, organization_id, team_id, project_id, report_markdown, node_link_json,
          node_count, edge_count, community_count, snapshot_type
        ) values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
        on conflict (snapshot_id) do update set
          report_markdown = excluded.report_markdown,
          node_link_json = excluded.node_link_json,
          node_count = excluded.node_count,
          edge_count = excluded.edge_count,
          community_count = excluded.community_count,
          computed_at = now()
        "#,
    )
    .bind(snapshot_id)
    .bind(scope.organization_id)
    .bind(scope.team_id)
    .bind(project_id)
    .bind(generate_knowledge_report(graph))
    .bind(to_node_link_json(graph))
    .bind(graph.statistics.node_count as i32)
    .bind(graph.statistics.edge_count as i32)
    .bind(graph.statistics.community_count as i32)
    .bind(snapshot_type)
    .execute(&mut **tx)
    .await?;

    sqlx::query(
        r#"
        delete from public.knowledge_communities
        where organization_id = $1 and team_id = $2 and project_id = $3
        "#,
    )
    .bind(scope.organization_id)
    .bind(scope.team_id)
    .bind(project_id)
    .execute(&mut **tx)
    .await?;

    // Batch community inserts (matching Node.js batch size of 500)
    for chunk in graph.communities.chunks(500) {
        let mut qb = sqlx::QueryBuilder::new(
            "insert into public.knowledge_communities (organization_id, team_id, project_id, community_id, label, cohesion_score, node_count, representative_nodes, bridge_nodes, level, community_path) ",
        );
        qb.push_values(chunk, |mut b, community| {
            b.push_bind(scope.organization_id)
                .push_bind(scope.team_id)
                .push_bind(project_id)
                .push_bind(community.community_id as i32)
                .push_bind(&community.label)
                .push_bind(community.cohesion_score)
                .push_bind(community.node_count as i32)
                .push_bind(json!(&community.representative_nodes))
                .push_bind(json!(&community.bridge_nodes))
                .push_bind(community.level as i32)
                .push_bind(community.community_path.as_deref());
        });
        qb.build().execute(&mut **tx).await?;
    }

    // Batch memory edge inserts (matching Node.js batch size of 200)
    let persistable_edges: Vec<_> = graph
        .edges
        .iter()
        .filter_map(|edge| to_persistable_memory_edge(edge))
        .collect();
    for chunk in persistable_edges.chunks(200) {
        let mut qb = sqlx::QueryBuilder::new(
            "insert into public.memory_edges (organization_id, team_id, project_id, from_memory_id, to_memory_id, edge_type, evidence, weight, metadata) ",
        );
        qb.push_values(
            chunk,
            |mut b, (from_id, to_id, relation, evidence, weight, metadata)| {
                b.push_bind(scope.organization_id)
                    .push_bind(scope.team_id)
                    .push_bind(project_id)
                    .push_bind(from_id)
                    .push_bind(to_id);
                b.push_bind(relation);
                b.push_unseparated("::public.memory_edge_type");
                b.push_bind(evidence);
                b.push_unseparated("::public.evidence_level");
                b.push_bind(*weight).push_bind(metadata);
            },
        );
        qb.push(" on conflict do nothing");
        qb.build().execute(&mut **tx).await?;
    }

    Ok(())
}

async fn settle_success(
    db: &Database,
    scope: &RepositoryContext,
    job: &WorkerJobRecord,
) -> Result<(), String> {
    let scoped = RepositoryContext {
        project_id: Some(job.project_id),
        ..scope.clone()
    };
    let mut tx = db.pool().begin().await.map_err(|error| error.to_string())?;
    apply_repository_context(&mut *tx, &scoped)
        .await
        .map_err(|error| error.to_string())?;
    complete_worker_job(&mut tx, job)
        .await
        .map_err(|error| error.to_string())?;
    tx.commit().await.map_err(|error| error.to_string())
}

async fn settle_failure(
    db: &Database,
    scope: &RepositoryContext,
    job: &WorkerJobRecord,
    error_message: &str,
) -> Result<(), String> {
    let scoped = RepositoryContext {
        project_id: Some(job.project_id),
        ..scope.clone()
    };
    let mut tx = db.pool().begin().await.map_err(|error| error.to_string())?;
    apply_repository_context(&mut *tx, &scoped)
        .await
        .map_err(|error| error.to_string())?;
    fail_worker_job(&mut tx, job, error_message)
        .await
        .map_err(|error| error.to_string())?;
    tx.commit().await.map_err(|error| error.to_string())
}
