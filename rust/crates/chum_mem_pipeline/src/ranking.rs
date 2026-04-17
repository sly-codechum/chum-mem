use std::collections::{BTreeMap, HashMap};

use chum_mem_contracts::{
    AuthorityClass, DisclosureLevel, MemoryType, ProofHandle, ProofType, ProvenanceHandle,
    RetrievalIntent, VerificationStatus,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RankedMemory {
    pub id: Uuid,
    pub project_id: Uuid,
    #[serde(rename = "type")]
    pub memory_type: MemoryType,
    pub title: String,
    pub summary: String,
    pub score: f64,
    pub created_at: String,
    #[serde(default)]
    pub session_ids: Vec<Uuid>,
    #[serde(default)]
    pub provenance: Vec<ProvenanceHandle>,
    #[serde(default)]
    pub proof_handles: Vec<ProofHandle>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lexical_score: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub semantic_score: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exact_session_match: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_relevance_score: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub graph_proximity_score: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recency_score: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub importance_score: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence_score: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub freshness_penalty: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub superseded_penalty: Option<f64>,
    /// v2.2.2: Community-aware retrieval — relevance score (0..1) of the
    /// community this memory belongs to, w.r.t. the current query.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub community_score: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repo_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub superseded_at: Option<String>,
    #[serde(default)]
    pub related_memory_ids: Vec<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_class: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ranking_role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub claim_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub claim_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub claim_type: Option<MemoryType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authority_class: Option<AuthorityClass>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verification_status: Option<VerificationStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proof_type: Option<ProofType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub valid_from: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub valid_to: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub superseded_by: Option<Uuid>,
    #[serde(default)]
    pub active_conflict_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SemanticQueryResult {
    pub id: Uuid,
    pub distance: f64,
    pub document: Option<String>,
    pub metadata: Option<Value>,
}

pub type RankingContext = RankingContextInner;

#[derive(Debug, Clone, Default)]
pub struct RankingContextInner {
    pub session_id: Option<Uuid>,
    pub repo_url: Option<String>,
    pub branch: Option<String>,
    pub session_graph_weights: HashMap<Uuid, f64>,
    pub retrieval_intent: RetrievalIntent,
    pub query_text: Option<String>,
    pub now: Option<time::OffsetDateTime>,
    /// v2.2.2: Requested claim types for type-fit scoring. Empty = all types.
    pub requested_types: Vec<String>,
    /// v2.2.2: Community-aware retrieval — relevance score per community_id.
    /// Empty means the caller did not supply community information; scoring
    /// skips the community contribution in that case.
    pub community_relevance: HashMap<usize, f64>,
    /// v2.2.2: Memory → community_id lookup. Memories absent from this map
    /// are treated as unaffiliated and receive no community boost.
    pub memory_community: HashMap<Uuid, usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressiveDisclosureResult {
    pub level: DisclosureLevel,
    pub overview: Vec<RankedMemory>,
    pub related: Vec<RankedMemory>,
    pub full: Vec<RankedMemory>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchMetrics {
    pub lexical_count: usize,
    pub semantic_count: usize,
    pub latency_ms: u128,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemorySearchEnvelope {
    pub hits: Vec<RankedMemory>,
    pub disclosure: ProgressiveDisclosureResult,
    pub metrics: SearchMetrics,
}

pub fn merge_hybrid_results(
    lexical_hits: &[RankedMemory],
    semantic_hits: &[SemanticQueryResult],
    context: &RankingContext,
) -> Vec<RankedMemory> {
    let mut by_id = BTreeMap::new();

    for hit in lexical_hits {
        by_id.insert(
            hit.id,
            RankedMemory {
                lexical_score: hit.lexical_score.or(Some(hit.score)),
                ..hit.clone()
            },
        );
    }

    for hit in semantic_hits {
        let semantic_score = 1.0 / (1.0 + hit.distance);
        if let Some(existing) = by_id.get_mut(&hit.id) {
            existing.semantic_score = Some(semantic_score);
            existing.score = existing.score.max(semantic_score);
            if let Some(metadata) = &hit.metadata {
                let session_ids = metadata_session_ids(metadata);
                for session_id in session_ids {
                    if !existing.session_ids.contains(&session_id) {
                        existing.session_ids.push(session_id);
                    }
                }
            }
            continue;
        }

        let metadata = hit.metadata.clone().unwrap_or(Value::Null);
        let project_id = metadata_uuid(&metadata, "projectId").unwrap_or_else(Uuid::nil);
        let memory_type = metadata_memory_type(&metadata).unwrap_or(MemoryType::Summary);
        by_id.insert(
            hit.id,
            RankedMemory {
                id: hit.id,
                project_id,
                memory_type,
                title: metadata_string(&metadata, "title")
                    .or_else(|| hit.document.clone())
                    .unwrap_or_else(|| "Semantic memory hit".to_string()),
                summary: metadata_string(&metadata, "summary")
                    .or_else(|| hit.document.clone())
                    .unwrap_or_default(),
                score: semantic_score,
                created_at: metadata_string(&metadata, "createdAt").unwrap_or_else(now_rfc3339),
                session_ids: metadata_session_ids(&metadata),
                provenance: Vec::new(),
                proof_handles: Vec::new(),
                lexical_score: None,
                semantic_score: Some(semantic_score),
                exact_session_match: None,
                session_relevance_score: None,
                graph_proximity_score: None,
                recency_score: None,
                importance_score: metadata_number(&metadata, "importanceScore"),
                confidence_score: metadata_number(&metadata, "confidenceScore"),
                freshness_penalty: None,
                superseded_penalty: None,
                community_score: None,
                repo_url: metadata_string(&metadata, "repoUrl"),
                branch: metadata_string(&metadata, "branch"),
                superseded_at: metadata_string(&metadata, "supersededAt"),
                related_memory_ids: Vec::new(),
                source_class: metadata_string(&metadata, "sourceClass"),
                ranking_role: metadata_string(&metadata, "rankingRole"),
                claim_id: metadata_uuid(&metadata, "claimId"),
                claim_key: metadata_string(&metadata, "claimKey"),
                claim_type: metadata_memory_type_key(&metadata, "claimType"),
                authority_class: metadata_authority_class(&metadata),
                verification_status: metadata_verification_status(&metadata),
                proof_type: metadata_proof_type(&metadata),
                valid_from: metadata_string(&metadata, "validFrom"),
                valid_to: metadata_string(&metadata, "validTo"),
                superseded_by: metadata_uuid(&metadata, "supersededBy"),
                active_conflict_count: metadata_number(&metadata, "activeConflictCount")
                    .map(|value| value as i64)
                    .unwrap_or(0),
            },
        );
    }

    rank_hybrid_results(&by_id.into_values().collect::<Vec<_>>(), context)
}

pub fn rank_hybrid_results(hits: &[RankedMemory], context: &RankingContext) -> Vec<RankedMemory> {
    let mut ranked = hits
        .iter()
        .cloned()
        .map(|hit| with_ranking_signals(hit, context))
        .collect::<Vec<_>>();
    ranked.sort_by(|left, right| {
        right
            .score
            .partial_cmp(&left.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    diversify_ranked_results(ranked)
}

pub fn progressive_disclosure(
    hits: &[RankedMemory],
    level: DisclosureLevel,
) -> ProgressiveDisclosureResult {
    let ranked = rank_hybrid_results(hits, &RankingContext::default());
    let overview = ranked.iter().take(5).cloned().collect::<Vec<_>>();
    let related = ranked.iter().take(12).cloned().collect::<Vec<_>>();
    let full = ranked.clone();

    match level {
        DisclosureLevel::Overview => ProgressiveDisclosureResult {
            level,
            overview,
            related: Vec::new(),
            full: Vec::new(),
        },
        DisclosureLevel::Related => ProgressiveDisclosureResult {
            level,
            overview,
            related,
            full: Vec::new(),
        },
        DisclosureLevel::Full => ProgressiveDisclosureResult {
            level,
            overview,
            related,
            full,
        },
    }
}

fn with_ranking_signals(mut hit: RankedMemory, context: &RankingContext) -> RankedMemory {
    let source_class = hit
        .source_class
        .clone()
        .unwrap_or_else(|| infer_source_class(&hit));
    let ranking_role = hit
        .ranking_role
        .clone()
        .unwrap_or_else(|| infer_ranking_role(&hit, &source_class));
    let exact_session_match = context
        .session_id
        .is_some_and(|session_id| hit.session_ids.contains(&session_id));
    // Preserve pre-existing signal values (matching Node.js `??` behavior)
    let session_relevance_score = hit
        .session_relevance_score
        .unwrap_or_else(|| calculate_session_relevance(&hit, context));
    let graph_proximity_score = hit
        .graph_proximity_score
        .unwrap_or_else(|| calculate_graph_proximity(&hit.session_ids, context));
    let recency_score = hit
        .recency_score
        .unwrap_or_else(|| calculate_recency_score(&hit.created_at, context.now));
    let freshness_penalty = hit
        .freshness_penalty
        .unwrap_or_else(|| calculate_freshness_penalty(&hit, context.now));
    let superseded_penalty = hit.superseded_penalty.unwrap_or_else(|| {
        if hit.superseded_at.is_some() || hit.superseded_by.is_some() {
            1.0
        } else {
            0.0
        }
    });
    // PCKC v2.2: boost claims by authority class and verification status
    let authority_boost = match hit.authority_class {
        Some(AuthorityClass::Repository) => 0.18,
        Some(AuthorityClass::TestVerified) => 0.15,
        Some(AuthorityClass::ToolVerified) => 0.12,
        Some(AuthorityClass::UserConfirmed) => 0.10,
        Some(AuthorityClass::SessionDerived) => 0.04,
        Some(AuthorityClass::ModelDerived) => 0.0,
        None => 0.0,
    };
    let verification_boost = match hit.verification_status {
        Some(VerificationStatus::Verified) => 0.10,
        Some(VerificationStatus::UserConfirmed) => 0.08,
        Some(VerificationStatus::Inferred) => 0.02,
        Some(VerificationStatus::Contradicted) => -0.20,
        Some(VerificationStatus::Unverified) => 0.0,
        None => 0.0,
    };
    // Penalize claims with active unresolved contradictions
    let conflict_penalty = if hit.active_conflict_count > 0 {
        (hit.active_conflict_count as f64 * 0.08).min(0.24)
    } else {
        0.0
    };
    let lexical = normalize_score(hit.lexical_score.unwrap_or(0.0));
    let semantic = normalize_score(hit.semantic_score.unwrap_or(0.0));
    let source_prior = source_prior(&hit, &source_class, context);

    // v2.2.2: Type-fit boost — when caller requests specific claim types,
    // boost matching types and penalize non-matching.
    let type_fit_boost = if !context.requested_types.is_empty() {
        if context.requested_types.contains(&source_class) {
            0.25
        } else {
            -0.15
        }
    } else {
        0.0
    };

    // v2.2.2: Community-aware retrieval — if the caller provided community
    // relevance scores and the memory's community is known, blend that in.
    let community_score = hit.community_score.or_else(|| {
        context
            .memory_community
            .get(&hit.id)
            .and_then(|cid| context.community_relevance.get(cid))
            .copied()
    });
    let community_boost = community_score.map(normalize_score).unwrap_or(0.0) * 0.15;

    let score = lexical * 0.32
        + semantic * 0.30
        + normalize_score(session_relevance_score) * 0.12
        + normalize_score(graph_proximity_score) * 0.10
        + normalize_score(recency_score) * 0.08
        + normalize_score(hit.importance_score.unwrap_or(0.0)) * 0.08
        + normalize_score(hit.confidence_score.unwrap_or(0.0)) * 0.06
        + source_prior
        + authority_boost
        + verification_boost
        + type_fit_boost
        + community_boost
        - normalize_score(freshness_penalty) * 0.10
        - normalize_score(superseded_penalty) * 0.10
        - conflict_penalty
        + if exact_session_match { 0.5 } else { 0.0 };

    hit.exact_session_match = Some(exact_session_match);
    hit.session_relevance_score = Some(session_relevance_score);
    hit.graph_proximity_score = Some(graph_proximity_score);
    hit.recency_score = Some(recency_score);
    hit.freshness_penalty = Some(freshness_penalty);
    hit.superseded_penalty = Some(superseded_penalty);
    hit.community_score = community_score;
    hit.score = score;
    hit.source_class = Some(source_class);
    hit.ranking_role = Some(ranking_role);
    hit
}

fn infer_source_class(hit: &RankedMemory) -> String {
    let title = hit.title.to_lowercase();
    if title.contains("session reflection") {
        return "reflection".to_string();
    }
    if hit.memory_type == MemoryType::Summary
        && (title.starts_with("session summary")
            || title.starts_with("episode ")
            || title.contains(": debugging -")
            || title.contains(": conversation -")
            || title.contains(": implementation -"))
    {
        return "session_summary".to_string();
    }
    match hit.memory_type {
        MemoryType::Decision => "decision".to_string(),
        MemoryType::Task => "task".to_string(),
        MemoryType::Constraint => "constraint".to_string(),
        MemoryType::Fact => "fact".to_string(),
        MemoryType::ImplementationDetail => "implementation_detail".to_string(),
        MemoryType::Bug => "bug".to_string(),
        MemoryType::Fix => "fix".to_string(),
        MemoryType::OpenQuestion => "open_question".to_string(),
        MemoryType::Risk => "risk".to_string(),
        MemoryType::ChangeLog => "change_log".to_string(),
        MemoryType::Summary => "summary".to_string(),
    }
}

fn infer_ranking_role(hit: &RankedMemory, source_class: &str) -> String {
    match source_class {
        "decision" => "durable_decision".to_string(),
        "task" => "active_task".to_string(),
        "constraint" => "constraint_guardrail".to_string(),
        "fact" => "project_fact".to_string(),
        "implementation_detail" => "implementation_note".to_string(),
        "bug" => "known_bug".to_string(),
        "fix" => "verified_fix".to_string(),
        "open_question" => "open_question".to_string(),
        "reflection" => "reflection_fallback".to_string(),
        "session_summary" => "continuity_fallback".to_string(),
        _ => match hit.memory_type {
            MemoryType::Summary => "summary_fallback".to_string(),
            _ => "general_memory".to_string(),
        },
    }
}

fn source_prior(hit: &RankedMemory, source_class: &str, context: &RankingContext) -> f64 {
    let query = context
        .query_text
        .as_deref()
        .unwrap_or_default()
        .to_lowercase();
    let mentions_reflection = query.contains("reflection") || query.contains("risk");
    let mentions_session = query.contains("session") || query.contains("history");

    match context.retrieval_intent {
        RetrievalIntent::RepositoryOnly => match source_class {
            "decision" | "fact" | "implementation_detail" | "constraint" | "fix" => 0.16,
            "task" => 0.08,
            "open_question" => -0.04,
            "session_summary" | "reflection" => -0.28,
            "summary" => -0.14,
            _ => 0.0,
        },
        RetrievalIntent::MemoryOnly
        | RetrievalIntent::SessionGraphOnly
        | RetrievalIntent::Hybrid => match source_class {
            "decision" | "fact" => 0.22,
            "task" => 0.18,
            "constraint" => 0.2,
            "implementation_detail" => 0.14,
            "bug" => 0.12,
            "fix" => 0.18,
            "open_question" => 0.1,
            "risk" => 0.08,
            "reflection" => {
                if mentions_reflection || mentions_session {
                    0.04
                } else {
                    -0.24
                }
            }
            "session_summary" => {
                if mentions_session {
                    -0.04
                } else {
                    -0.18
                }
            }
            "summary" => -0.08,
            _ => match hit.memory_type {
                MemoryType::Summary => -0.06,
                _ => 0.0,
            },
        },
        RetrievalIntent::None => -0.30,
    }
}

fn diversify_ranked_results(ranked: Vec<RankedMemory>) -> Vec<RankedMemory> {
    let mut result = Vec::with_capacity(ranked.len());
    let mut seen_families = std::collections::HashSet::new();
    let mut generic_session_counts = HashMap::<Uuid, usize>::new();

    for hit in ranked {
        let source_class = hit
            .source_class
            .clone()
            .unwrap_or_else(|| infer_source_class(&hit));
        let family_key = format!("{}:{}", source_class, duplicate_family_key(&hit));
        if !seen_families.insert(family_key) {
            continue;
        }

        if matches!(
            source_class.as_str(),
            "session_summary" | "reflection" | "summary"
        ) && let Some(session_id) = hit.session_ids.first().copied()
        {
            let counter = generic_session_counts.entry(session_id).or_default();
            if *counter >= 1 {
                continue;
            }
            *counter += 1;
        }

        result.push(hit);
    }

    result
}

fn duplicate_family_key(hit: &RankedMemory) -> String {
    let canonical_title = canonicalize_text(&hit.title);
    if !canonical_title.is_empty() {
        return canonical_title;
    }
    canonicalize_text(&hit.summary)
}

fn canonicalize_text(value: &str) -> String {
    let lowered = value.to_lowercase();
    let mut normalized = String::with_capacity(lowered.len());
    let mut last_was_space = false;
    for ch in lowered.chars() {
        let mapped = if ch.is_ascii_digit() {
            ' '
        } else if ch.is_ascii_alphanumeric() || ch == '_' {
            ch
        } else {
            ' '
        };
        if mapped == ' ' {
            if !last_was_space {
                normalized.push(' ');
                last_was_space = true;
            }
        } else {
            normalized.push(mapped);
            last_was_space = false;
        }
    }
    normalized.trim().to_string()
}

fn calculate_session_relevance(hit: &RankedMemory, context: &RankingContext) -> f64 {
    if let Some(session_id) = context.session_id
        && hit.session_ids.contains(&session_id)
    {
        return 1.0;
    }

    let mut score: f64 = 0.0;
    if context.branch.is_some() && context.branch == hit.branch {
        score += 0.45;
    }
    if context.repo_url.is_some() && context.repo_url == hit.repo_url {
        score += 0.2;
    }
    score.min(1.0)
}

fn calculate_graph_proximity(session_ids: &[Uuid], context: &RankingContext) -> f64 {
    session_ids
        .iter()
        .filter_map(|session_id| context.session_graph_weights.get(session_id).copied())
        .fold(0.0_f64, f64::max)
}

fn calculate_recency_score(created_at: &str, now: Option<time::OffsetDateTime>) -> f64 {
    let created_at =
        time::OffsetDateTime::parse(created_at, &time::format_description::well_known::Rfc3339)
            .unwrap_or_else(|_| time::OffsetDateTime::now_utc());
    let now = now.unwrap_or_else(time::OffsetDateTime::now_utc);
    let age_days = ((now - created_at).whole_seconds().max(0) as f64) / 86_400.0;
    if age_days <= 1.0 {
        1.0
    } else if age_days <= 7.0 {
        0.85
    } else if age_days <= 30.0 {
        0.65
    } else if age_days <= 90.0 {
        0.4
    } else {
        0.2
    }
}

fn calculate_freshness_penalty(hit: &RankedMemory, now: Option<time::OffsetDateTime>) -> f64 {
    if hit.superseded_at.is_some() {
        return 1.0;
    }

    let created_at = time::OffsetDateTime::parse(
        &hit.created_at,
        &time::format_description::well_known::Rfc3339,
    )
    .unwrap_or_else(|_| time::OffsetDateTime::now_utc());
    let now = now.unwrap_or_else(time::OffsetDateTime::now_utc);
    let age_days = ((now - created_at).whole_seconds().max(0) as f64) / 86_400.0;

    match hit.memory_type {
        MemoryType::Task | MemoryType::ImplementationDetail | MemoryType::OpenQuestion => {
            if age_days > 14.0 {
                0.5
            } else {
                0.0
            }
        }
        MemoryType::Bug | MemoryType::Fix => {
            if age_days > 30.0 {
                0.3
            } else {
                0.0
            }
        }
        MemoryType::Decision | MemoryType::Fact | MemoryType::Constraint => {
            if age_days > 180.0 {
                0.1
            } else {
                0.0
            }
        }
        _ => {
            if age_days > 90.0 {
                0.2
            } else {
                0.0
            }
        }
    }
}

fn normalize_score(value: f64) -> f64 {
    if !value.is_finite() {
        return 0.0;
    }
    value.clamp(0.0, 1.0)
}

fn metadata_string(metadata: &Value, key: &str) -> Option<String> {
    metadata
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn metadata_number(metadata: &Value, key: &str) -> Option<f64> {
    metadata.get(key).and_then(|value| match value {
        Value::Number(number) => number.as_f64().filter(|n| n.is_finite()),
        _ => None,
    })
}

fn metadata_uuid(metadata: &Value, key: &str) -> Option<Uuid> {
    metadata_string(metadata, key).and_then(|value| Uuid::parse_str(&value).ok())
}

fn metadata_memory_type(metadata: &Value) -> Option<MemoryType> {
    metadata_memory_type_key(metadata, "type")
}

fn metadata_memory_type_key(metadata: &Value, key: &str) -> Option<MemoryType> {
    metadata_string(metadata, key).and_then(|value| match value.as_str() {
        "fact" => Some(MemoryType::Fact),
        "decision" => Some(MemoryType::Decision),
        "task" => Some(MemoryType::Task),
        "constraint" => Some(MemoryType::Constraint),
        "bug" => Some(MemoryType::Bug),
        "fix" => Some(MemoryType::Fix),
        "open_question" => Some(MemoryType::OpenQuestion),
        "summary" => Some(MemoryType::Summary),
        "implementation_detail" => Some(MemoryType::ImplementationDetail),
        "change_log" => Some(MemoryType::ChangeLog),
        "risk" => Some(MemoryType::Risk),
        _ => None,
    })
}

fn metadata_authority_class(metadata: &Value) -> Option<AuthorityClass> {
    metadata_string(metadata, "authorityClass").and_then(|value| match value.as_str() {
        "repository" => Some(AuthorityClass::Repository),
        "user_confirmed" => Some(AuthorityClass::UserConfirmed),
        "tool_verified" => Some(AuthorityClass::ToolVerified),
        "test_verified" => Some(AuthorityClass::TestVerified),
        "session_derived" => Some(AuthorityClass::SessionDerived),
        "model_derived" => Some(AuthorityClass::ModelDerived),
        _ => None,
    })
}

fn metadata_verification_status(metadata: &Value) -> Option<VerificationStatus> {
    metadata_string(metadata, "verificationStatus").and_then(|value| match value.as_str() {
        "verified" => Some(VerificationStatus::Verified),
        "user_confirmed" => Some(VerificationStatus::UserConfirmed),
        "inferred" => Some(VerificationStatus::Inferred),
        "contradicted" => Some(VerificationStatus::Contradicted),
        "unverified" => Some(VerificationStatus::Unverified),
        _ => None,
    })
}

fn metadata_proof_type(metadata: &Value) -> Option<ProofType> {
    metadata_string(metadata, "proofType").and_then(|value| match value.as_str() {
        "repository" => Some(ProofType::Repository),
        "session_event" => Some(ProofType::SessionEvent),
        "tool_result" => Some(ProofType::ToolResult),
        "test_result" => Some(ProofType::TestResult),
        "user_confirmation" => Some(ProofType::UserConfirmation),
        "summary" => Some(ProofType::Summary),
        _ => None,
    })
}

fn metadata_session_ids(metadata: &Value) -> Vec<Uuid> {
    if let Some(array) = metadata.get("sessionIds").and_then(Value::as_array) {
        return array
            .iter()
            .filter_map(Value::as_str)
            .filter_map(|value| Uuid::parse_str(value).ok())
            .collect();
    }

    metadata_uuid(metadata, "sessionId")
        .map(|session_id| vec![session_id])
        .unwrap_or_default()
}

/// Deduplicate hits by provenance: two hits sharing the exact same set of
/// (session_id, session_event_id) provenance handles are considered duplicates.
/// Matches Node.js `dedupeHitsByProvenance`: sorts by score first, then dedupes.
/// Empty-provenance hits share the key "" so only the first (highest-scored) survives.
pub fn dedupe_hits_by_provenance(hits: Vec<RankedMemory>) -> Vec<RankedMemory> {
    // Node.js sorts by blended score before deduplication
    let sorted = rank_hybrid_results(&hits, &RankingContext::default());
    let mut seen = std::collections::HashSet::new();
    let mut result = Vec::with_capacity(sorted.len());
    for hit in sorted {
        let mut handles: Vec<String> = hit
            .provenance
            .iter()
            .map(|p| format!("{}:{}", p.session_id, p.session_event_id))
            .collect();
        handles.sort();
        let key = handles.join("|");
        if seen.insert(key) {
            result.push(hit);
        }
    }
    result
}

fn now_rfc3339() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}
