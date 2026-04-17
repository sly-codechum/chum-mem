//! Async claim reconciliation (supersedes / contradicts / confirms).
//!
//! Extracted from `rust/apps/api/src/main.rs::reconcile_claim_memory_state`
//! as part of the v2.2.1 ingestion-choke fix. The session_end writer path no
//! longer calls this inline; the `reconcile-claim-state` worker job does, under
//! a per-project advisory lock, in bounded chunks of at most
//! `RECONCILE_CHUNK_SIZE` claims per sub-transaction.

use chum_mem_pipeline::reconcile::{
    claim_strength, current_supersedes_prior, parse_authority_class, parse_memory_type,
    parse_verification_status,
};
use serde_json::json;
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

use crate::{
    DbError, RepositoryContext, mark_claim_superseded, mark_memory_superseded,
    update_claim_verification_status, upsert_claim_edge, upsert_memory_edge,
};

/// Maximum number of claims processed per sub-transaction.
///
/// Chosen to keep the per-xact relation/advisory lock count well under the
/// configured `max_locks_per_transaction`: each reconciliation step touches
/// `claims`, `memories`, `memory_edges`, `claim_edges` for ~12 prior candidates,
/// so 50 claims per chunk stays under ~250 active row locks.
pub const RECONCILE_CHUNK_SIZE: usize = 50;

/// Advisory key used by the worker to serialize reconciliation runs for a
/// single project. Unlike the pre-fix code this lock is xact-scoped and takes
/// exactly ONE slot per sub-transaction, regardless of claim fan-out.
pub fn reconcile_project_advisory_key(project_id: Uuid) -> String {
    format!("chum-mem:reconcile-claim-state:{project_id}")
}

#[derive(Debug, Clone)]
pub struct ReconcileOutcome {
    pub claims_processed: usize,
    pub supersedes_edges: usize,
    pub contradicts_edges: usize,
    pub confirms_edges: usize,
}

impl ReconcileOutcome {
    pub fn merge(&mut self, other: Self) {
        self.claims_processed += other.claims_processed;
        self.supersedes_edges += other.supersedes_edges;
        self.contradicts_edges += other.contradicts_edges;
        self.confirms_edges += other.confirms_edges;
    }
}

impl Default for ReconcileOutcome {
    fn default() -> Self {
        Self {
            claims_processed: 0,
            supersedes_edges: 0,
            contradicts_edges: 0,
            confirms_edges: 0,
        }
    }
}

#[derive(Debug, Clone)]
struct ClaimSnapshot {
    claim_id: Uuid,
    memory_id: Uuid,
    claim_type: String,
    claim_key: String,
    subject: String,
    claim_polarity: Option<String>,
    authority_class: Option<String>,
    verification_status: Option<String>,
}

/// Reconcile a single chunk of newly-created claims against prior admitted
/// claims in the same project. This function runs inside the caller's
/// transaction and expects the per-project advisory lock to already be held.
pub async fn reconcile_claim_state_for_claims(
    tx: &mut Transaction<'_, Postgres>,
    context: &RepositoryContext,
    project_id: Uuid,
    claim_ids: &[Uuid],
) -> Result<ReconcileOutcome, DbError> {
    let mut outcome = ReconcileOutcome::default();
    if claim_ids.is_empty() {
        return Ok(outcome);
    }

    // 1. Load the snapshot rows for the new claims.
    let new_rows = sqlx::query(
        r#"
        select
          id as claim_id,
          memory_id,
          claim_type::text as claim_type,
          claim_key,
          subject,
          claim_polarity,
          authority_class,
          verification_status
        from public.claims
        where organization_id = $1
          and team_id = $2
          and project_id = $3
          and id = any($4)
        "#,
    )
    .bind(context.organization_id)
    .bind(context.team_id)
    .bind(project_id)
    .bind(claim_ids)
    .fetch_all(&mut **tx)
    .await?;

    for row in &new_rows {
        let current = ClaimSnapshot {
            claim_id: row.try_get("claim_id")?,
            memory_id: row.try_get("memory_id")?,
            claim_type: row.try_get("claim_type")?,
            claim_key: row.try_get("claim_key")?,
            subject: row.try_get("subject")?,
            claim_polarity: row.try_get("claim_polarity")?,
            authority_class: row.try_get("authority_class")?,
            verification_status: row.try_get("verification_status")?,
        };
        outcome.merge(
            reconcile_single_claim(tx, context, project_id, &current).await?,
        );
    }

    Ok(outcome)
}

async fn reconcile_single_claim(
    tx: &mut Transaction<'_, Postgres>,
    context: &RepositoryContext,
    project_id: Uuid,
    current: &ClaimSnapshot,
) -> Result<ReconcileOutcome, DbError> {
    let mut outcome = ReconcileOutcome {
        claims_processed: 1,
        ..ReconcileOutcome::default()
    };

    // Match the original 12-row lookup. This SELECT is now backed by the
    // partial index `idx_claims_active_admitted_lookup` from migration 0013.
    let rows = sqlx::query(
        r#"
        select
          id as claim_id,
          memory_id,
          claim_type::text as claim_type,
          claim_polarity,
          authority_class,
          verification_status
        from public.claims
        where organization_id = $1
          and team_id = $2
          and project_id = $3
          and id <> $4
          and (claim_key = $5 or subject = $6)
          and admitted = true
        order by valid_from desc, created_at desc, id desc
        limit 12
        "#,
    )
    .bind(context.organization_id)
    .bind(context.team_id)
    .bind(project_id)
    .bind(current.claim_id)
    .bind(&current.claim_key)
    .bind(&current.subject)
    .fetch_all(&mut **tx)
    .await?;

    let current_polarity = current.claim_polarity.clone();
    let current_verification = current
        .verification_status
        .as_deref()
        .and_then(parse_verification_status);
    let current_authority = current
        .authority_class
        .as_deref()
        .and_then(parse_authority_class);
    let current_memory_type = parse_memory_type(&current.claim_type);

    for row in rows {
        let prior_claim_id: Uuid = row.try_get("claim_id")?;
        let prior_memory_id: Uuid = row.try_get("memory_id")?;
        let prior_type: String = row.try_get("claim_type")?;
        let prior_polarity: Option<String> = row.try_get("claim_polarity")?;
        let prior_authority_str: Option<String> = row.try_get("authority_class")?;
        let prior_verification_str: Option<String> = row.try_get("verification_status")?;
        let prior_memory_type = parse_memory_type(&prior_type);
        let prior_authority = prior_authority_str.as_deref().and_then(parse_authority_class);
        let prior_verification = prior_verification_str
            .as_deref()
            .and_then(parse_verification_status);

        let should_supersede = current_supersedes_prior(
            current_memory_type,
            prior_memory_type,
            current_verification,
            prior_verification,
            current_authority,
            prior_authority,
        );

        if should_supersede {
            mark_memory_superseded(tx, context, project_id, prior_memory_id).await?;
            mark_claim_superseded(tx, context, project_id, prior_claim_id, current.claim_id)
                .await?;
            upsert_memory_edge(
                tx,
                context,
                project_id,
                current.memory_id,
                prior_memory_id,
                "supersedes",
                "inferred",
                0.9,
                &json!({
                    "claimKey": current.claim_key,
                    "reason": "claim_key_replacement",
                }),
            )
            .await?;
            upsert_claim_edge(
                tx,
                context,
                project_id,
                current.claim_id,
                prior_claim_id,
                "supersedes",
                0.9,
                &json!({
                    "claimKey": current.claim_key,
                    "reason": "claim_key_or_subject_replacement",
                }),
            )
            .await?;
            outcome.supersedes_edges += 1;
        }

        if current_polarity.is_some()
            && prior_polarity.is_some()
            && current_polarity.as_ref() != prior_polarity.as_ref()
        {
            upsert_memory_edge(
                tx,
                context,
                project_id,
                current.memory_id,
                prior_memory_id,
                "contradicts",
                "inferred",
                0.86,
                &json!({
                    "claimKey": current.claim_key,
                    "currentPolarity": current_polarity,
                    "priorPolarity": prior_polarity,
                }),
            )
            .await?;
            upsert_claim_edge(
                tx,
                context,
                project_id,
                current.claim_id,
                prior_claim_id,
                "contradicts",
                0.86,
                &json!({
                    "claimKey": current.claim_key,
                    "currentPolarity": current_polarity,
                    "priorPolarity": prior_polarity,
                }),
            )
            .await?;
            match claim_strength(current_authority, current_verification)
                .cmp(&claim_strength(prior_authority, prior_verification))
            {
                std::cmp::Ordering::Greater => {
                    update_claim_verification_status(
                        tx,
                        context,
                        project_id,
                        prior_claim_id,
                        "contradicted",
                    )
                    .await?;
                }
                std::cmp::Ordering::Less => {
                    update_claim_verification_status(
                        tx,
                        context,
                        project_id,
                        current.claim_id,
                        "contradicted",
                    )
                    .await?;
                }
                std::cmp::Ordering::Equal => {}
            }
            outcome.contradicts_edges += 1;
        } else {
            upsert_memory_edge(
                tx,
                context,
                project_id,
                current.memory_id,
                prior_memory_id,
                "confirms",
                "inferred",
                0.62,
                &json!({
                    "claimKey": current.claim_key,
                }),
            )
            .await?;
            upsert_claim_edge(
                tx,
                context,
                project_id,
                current.claim_id,
                prior_claim_id,
                "confirms",
                0.62,
                &json!({ "claimKey": current.claim_key }),
            )
            .await?;
            outcome.confirms_edges += 1;
        }
    }

    Ok(outcome)
}
