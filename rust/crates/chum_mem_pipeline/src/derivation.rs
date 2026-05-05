use indexmap::IndexMap;
use std::collections::HashSet;

use chum_mem_contracts::{
    CanonicalEventType, EndSessionRequest, MemoryType, Provider, SessionEventPayload,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::CHROMA_EMBEDDING_DIMENSIONS;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionEventRecord {
    pub id: Uuid,
    pub event_type: CanonicalEventType,
    pub payload: SessionEventPayload,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DerivedMemoryDraft {
    #[serde(rename = "type")]
    pub memory_type: MemoryType,
    pub title: String,
    pub content: String,
    pub summary: String,
    pub importance_score: f64,
    pub confidence_score: f64,
    pub provenance_event_ids: Vec<Uuid>,
    pub metadata: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionEpisodeDraft {
    pub episode_ordinal: i32,
    pub episode_type: String,
    pub title: String,
    pub summary: String,
    pub started_at: String,
    pub ended_at: String,
    pub provenance_event_ids: Vec<Uuid>,
    pub metadata: Value,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSimilaritySignals {
    pub file_paths: Vec<String>,
    pub error_signatures: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionRelationshipScore {
    pub weight: f64,
    pub edge_type: String,
    pub reasons: Vec<String>,
}

pub fn derive_session_episodes(
    session_id: Uuid,
    provider: Provider,
    end_request: &EndSessionRequest,
    events: &[SessionEventRecord],
) -> Vec<SessionEpisodeDraft> {
    if events.is_empty() {
        return Vec::new();
    }

    let first = match events.first() {
        Some(first) => first,
        None => return Vec::new(),
    };

    let mut episodes = Vec::new();
    let mut current = EpisodeBucket::new(1, classify_event(first));

    for event in events {
        let next_type = classify_event(event);
        if should_start_new_episode(&current.events, event, &next_type) {
            episodes.push(materialize_episode(session_id, &current));
            current = EpisodeBucket::new(current.episode_ordinal + 1, next_type);
        }
        current.events.push(event.clone());
    }

    if !current.events.is_empty() {
        episodes.push(materialize_episode(session_id, &current));
    }

    let _ = provider;
    let _ = end_request;
    episodes
}

pub fn derive_memories_from_session(
    session_id: Uuid,
    provider: Provider,
    end_request: &EndSessionRequest,
    events: &[SessionEventRecord],
    episodes: Option<&[SessionEpisodeDraft]>,
) -> Vec<DerivedMemoryDraft> {
    let owned_episodes;
    let episodes = if let Some(episodes) = episodes {
        episodes
    } else {
        owned_episodes = derive_session_episodes(session_id, provider, end_request, events);
        &owned_episodes
    };

    let mut memories = derive_atomic_claim_memories(session_id, end_request, events, episodes);
    let has_atomic_claims = memories.iter().any(|memory| {
        !matches!(
            memory.memory_type,
            MemoryType::Summary | MemoryType::Risk | MemoryType::ChangeLog
        )
    });

    if !has_atomic_claims {
        let mut rollup_summary = end_request
            .summary
            .as_ref()
            .map(|value| value.trim().to_string())
            .unwrap_or_default();
        if rollup_summary.is_empty() {
            rollup_summary = episodes
                .iter()
                .map(|episode| format!("{}: {}", episode.title, episode.summary))
                .collect::<Vec<_>>()
                .join("\n");
            rollup_summary = truncate(rollup_summary, 3000);
        }

        if !rollup_summary.is_empty() {
            memories.push(DerivedMemoryDraft {
                memory_type: MemoryType::Summary,
                title: format!("Session summary ({})", provider_str(provider)),
                content: rollup_summary.clone(),
                summary: truncate(rollup_summary.clone(), 300),
                importance_score: 0.75,
                confidence_score: 0.8,
                provenance_event_ids: episodes
                    .iter()
                    .flat_map(|episode| episode.provenance_event_ids.iter().copied())
                    .collect(),
                metadata: json!({
                    "derivation": "session_episode_rollup_v2",
                    "sessionId": session_id,
                    "proofType": "summary",
                    "authorityClass": "model_derived",
                    "verificationStatus": "unverified",
                    "belief": { "admit": false },
                }),
            });
        }

        if let Some(reflection) = derive_reflection_memory(session_id, episodes) {
            memories.push(reflection);
        }

        for episode in episodes {
            memories.push(DerivedMemoryDraft {
                memory_type: MemoryType::Summary,
                title: episode.title.clone(),
                content: episode.summary.clone(),
                summary: truncate(episode.summary.clone(), 300),
                importance_score: if episode.episode_type == "debugging" {
                    0.8
                } else {
                    0.65
                },
                confidence_score: 0.8,
                provenance_event_ids: episode.provenance_event_ids.clone(),
                metadata: json!({
                    "derivation": "session_episode_summary_v1",
                    "sessionId": session_id,
                    "episodeOrdinal": episode.episode_ordinal,
                    "episodeType": episode.episode_type,
                    "proofType": "summary",
                    "authorityClass": "session_derived",
                    "verificationStatus": "unverified",
                    "belief": { "admit": false },
                }),
            });
        }
    }

    dedupe_derived_memories(memories)
}

pub fn extract_session_signals(events: &[SessionEventRecord]) -> SessionSimilaritySignals {
    use indexmap::IndexSet;

    let mut file_paths: IndexSet<String> = IndexSet::new();
    let mut error_signatures: IndexSet<String> = IndexSet::new();

    for event in events {
        if let Some(file_path) = event.payload.file_path.as_deref() {
            let trimmed = file_path.trim();
            if !trimmed.is_empty() {
                file_paths.insert(trimmed.to_string());
            }
        }

        let text = event_text(event).to_lowercase();
        let is_error = event.event_type == CanonicalEventType::Error
            || event.payload.exit_code.is_some_and(|code| code != 0)
            || text.contains("failed");
        if is_error {
            let normalized = normalize_error_signature(&event_text(event));
            if !normalized.is_empty() {
                error_signatures.insert(normalized);
            }
        }
    }

    SessionSimilaritySignals {
        file_paths: file_paths.into_iter().collect(),
        error_signatures: error_signatures.into_iter().collect(),
    }
}

pub fn score_session_relationship(
    current_repo_url: Option<&str>,
    current_branch: Option<&str>,
    current_signals: &SessionSimilaritySignals,
    candidate_repo_url: Option<&str>,
    candidate_branch: Option<&str>,
    candidate_signals: &SessionSimilaritySignals,
) -> Option<SessionRelationshipScore> {
    let mut weight = 0.0;
    let mut reasons = Vec::new();

    let current_repo = current_repo_url.filter(|s| !s.is_empty());
    let candidate_repo = candidate_repo_url.filter(|s| !s.is_empty());
    if current_repo.is_some() && candidate_repo.is_some() && current_repo == candidate_repo {
        weight += 0.2;
        reasons.push("same_repo".to_string());
    }

    let current_br = current_branch.filter(|s| !s.is_empty());
    let candidate_br = candidate_branch.filter(|s| !s.is_empty());
    if current_br.is_some() && candidate_br.is_some() && current_br == candidate_br {
        weight += 0.35;
        reasons.push("same_branch".to_string());
    }

    let shared_files = intersect_count(&current_signals.file_paths, &candidate_signals.file_paths);
    if shared_files > 0 {
        weight += (shared_files as f64 * 0.15).min(0.3);
        reasons.push(format!("shared_files:{shared_files}"));
    }

    let shared_errors = intersect_count(
        &current_signals.error_signatures,
        &candidate_signals.error_signatures,
    );
    if shared_errors > 0 {
        weight += (shared_errors as f64 * 0.15).min(0.3);
        reasons.push(format!("shared_errors:{shared_errors}"));
    }

    if weight < 0.35 {
        return None;
    }

    Some(SessionRelationshipScore {
        weight: ((weight.min(1.0)) * 10_000.0).round() / 10_000.0,
        edge_type: if reasons.iter().any(|reason| reason == "same_branch") {
            "same_branch".to_string()
        } else {
            "related_to".to_string()
        },
        reasons,
    })
}

pub fn event_text(event: &SessionEventRecord) -> String {
    [
        event.payload.message.as_deref().unwrap_or(""),
        event.payload.command.as_deref().unwrap_or(""),
        event.payload.tool_name.as_deref().unwrap_or(""),
        event.payload.file_path.as_deref().unwrap_or(""),
    ]
    .into_iter()
    .map(str::trim)
    .filter(|value| !value.is_empty())
    .collect::<Vec<_>>()
    .join(" | ")
}

pub fn embed_text(text: &str) -> Vec<f64> {
    let mut vector = vec![0.0_f64; CHROMA_EMBEDDING_DIMENSIONS];
    let normalized = text.to_lowercase();
    let tokens = normalized
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .filter(|token| !token.is_empty());

    for token in tokens {
        let hash = fnv1a32(token);
        let index = (hash as usize) % CHROMA_EMBEDDING_DIMENSIONS;
        let sign = if (hash >> 31) & 1 == 0 { 1.0 } else { -1.0 };
        vector[index] += sign;
    }

    let magnitude = vector.iter().map(|value| value * value).sum::<f64>().sqrt();
    if magnitude == 0.0 {
        return vector;
    }

    vector.into_iter().map(|value| value / magnitude).collect()
}

fn provider_str(provider: Provider) -> &'static str {
    match provider {
        Provider::Claude => "claude",
        Provider::Codex => "codex",
        Provider::Gemini => "gemini",
    }
}

fn derive_atomic_claim_memories(
    session_id: Uuid,
    end_request: &EndSessionRequest,
    events: &[SessionEventRecord],
    episodes: &[SessionEpisodeDraft],
) -> Vec<DerivedMemoryDraft> {
    let mut memories = Vec::new();

    for event in events {
        let episode = episodes
            .iter()
            .find(|episode| episode.provenance_event_ids.contains(&event.id));
        let text = event_text(event);
        memories.extend(extract_claims_from_text(
            session_id,
            episode,
            Some(event),
            &text,
            vec![event.id],
            false,
        ));
    }

    if let Some(summary) = end_request.summary.as_deref() {
        let provenance_event_ids = episodes
            .iter()
            .flat_map(|episode| episode.provenance_event_ids.iter().copied())
            .collect::<Vec<_>>();
        memories.extend(extract_claims_from_text(
            session_id,
            None,
            None,
            summary,
            provenance_event_ids,
            true,
        ));
    }

    memories
}

fn extract_claims_from_text(
    session_id: Uuid,
    episode: Option<&SessionEpisodeDraft>,
    event: Option<&SessionEventRecord>,
    text: &str,
    provenance_event_ids: Vec<Uuid>,
    from_summary: bool,
) -> Vec<DerivedMemoryDraft> {
    // v2.2.1 belief gate: Reasoning and TurnContext carry signal but must
    // never originate a durable claim — regardless of what the text says.
    // AgentMessage flows through the classifiers and is rejected downstream
    // as model_derived. See docs/research/v2.2.1-pckc/DESIGN.md §2.
    if event.is_some_and(|evt| {
        matches!(
            evt.event_type,
            CanonicalEventType::Reasoning | CanonicalEventType::TurnContext
        )
    }) {
        return Vec::new();
    }

    let mut claims = Vec::new();
    for segment in claim_segments(text) {
        let Some(memory_type) = classify_claim_type(event, &segment) else {
            continue;
        };
        let lower = segment.to_lowercase();
        let proof_type = classify_proof_type(event, from_summary);
        let authority_class = classify_authority_class(event, proof_type, from_summary);
        let verification_status = classify_verification_status(&lower, event, from_summary);
        let admit = should_admit_claim(memory_type, authority_class, verification_status, &lower);
        let claim_key = claim_key(memory_type, &segment, event);
        let title = claim_title(memory_type, &segment);
        let importance_score = claim_importance(memory_type, verification_status);
        let confidence_score = claim_confidence(verification_status, from_summary);
        let claim_polarity = if is_negative_claim(&lower) {
            "negative"
        } else {
            "positive"
        };
        claims.push(DerivedMemoryDraft {
            memory_type,
            title,
            content: truncate(segment.clone(), 3000),
            summary: truncate(segment.clone(), 300),
            importance_score,
            confidence_score,
            provenance_event_ids: provenance_event_ids.clone(),
            metadata: json!({
                "derivation": derivation_name(memory_type, from_summary),
                "sessionId": session_id,
                "episodeOrdinal": episode.map(|value| value.episode_ordinal),
                "episodeType": episode.map(|value| value.episode_type.clone()),
                "claimKey": claim_key,
                "claimPolarity": claim_polarity,
                "proofType": proof_type,
                "authorityClass": authority_class,
                "verificationStatus": verification_status,
                "sourceClass": source_class_for_memory_type(memory_type),
                "rankingRole": ranking_role_for_memory_type(memory_type),
                "belief": { "admit": admit },
                "answerCritical": !matches!(
                    memory_type,
                    MemoryType::ImplementationDetail | MemoryType::Summary | MemoryType::Risk
                ),
            }),
        });
    }
    claims
}

fn claim_segments(text: &str) -> Vec<String> {
    text.split(['\n', '|'])
        .flat_map(|line| line.split(" - "))
        .map(str::trim)
        .filter(|segment| segment.len() >= 12)
        .map(|segment| segment.trim_matches(|ch: char| ch == '-' || ch == '*' || ch == '•'))
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn classify_claim_type(event: Option<&SessionEventRecord>, segment: &str) -> Option<MemoryType> {
    let lower = segment.to_lowercase();
    let is_errorish = event.is_some_and(|evt| {
        evt.event_type == CanonicalEventType::Error
            || evt.payload.exit_code.is_some_and(|code| code != 0)
    }) || lower.contains("failed")
        || lower.contains("error")
        || lower.contains("exception")
        || lower.contains("bug:");
    if lower.contains("open question:")
        || lower.contains("question:")
        || lower.ends_with('?')
        || lower.contains("unknown whether")
    {
        return Some(MemoryType::OpenQuestion);
    }
    if lower.contains("constraint:")
        || lower.contains("must ")
        || lower.contains("do not ")
        || lower.contains("should not ")
        || lower.contains("required")
        || lower.contains("fallback only")
    {
        return Some(MemoryType::Constraint);
    }
    if lower.contains("decision:")
        || lower.contains("decision update")
        || lower.contains("we decided")
        || lower.contains("policy:")
    {
        return Some(MemoryType::Decision);
    }
    if lower.contains("task:")
        || lower.contains("todo:")
        || lower.contains("next:")
        || lower.contains("follow up")
        || lower.contains("need to ")
        || lower.contains("continue ")
    {
        return Some(MemoryType::Task);
    }
    if lower.contains("fix:")
        || lower.contains("fixed")
        || lower.contains("resolved")
        || lower.contains("verified fix")
        || lower.contains("confirmed fix")
    {
        return Some(MemoryType::Fix);
    }
    if is_errorish {
        return Some(MemoryType::Bug);
    }
    if lower.contains("verified") || lower.contains("confirmed") || lower.contains("current truth")
    {
        return Some(MemoryType::Fact);
    }
    if event.is_some_and(|evt| {
        matches!(
            evt.event_type,
            CanonicalEventType::Command
                | CanonicalEventType::ToolCall
                | CanonicalEventType::ToolResult
                | CanonicalEventType::FileChange
        )
    }) {
        return Some(MemoryType::ImplementationDetail);
    }
    None
}

fn classify_proof_type(event: Option<&SessionEventRecord>, from_summary: bool) -> &'static str {
    if from_summary {
        return "summary";
    }
    match event.map(|value| value.event_type) {
        Some(CanonicalEventType::Prompt | CanonicalEventType::Annotation) => "user_confirmation",
        Some(
            CanonicalEventType::ToolResult
            | CanonicalEventType::Command
            | CanonicalEventType::FileChange,
        ) => "tool_result",
        Some(CanonicalEventType::TestResult | CanonicalEventType::Error) => "test_result",
        _ => "session_event",
    }
}

fn classify_authority_class(
    event: Option<&SessionEventRecord>,
    proof_type: &str,
    from_summary: bool,
) -> &'static str {
    if from_summary {
        return "model_derived";
    }
    match proof_type {
        "user_confirmation" => "user_confirmed",
        "tool_result" => "tool_verified",
        "test_result" => "test_verified",
        _ => match event.map(|value| value.event_type) {
            // AgentMessage is structured assistant output; Reasoning and
            // TurnContext are defense-in-depth in case the hard gate is
            // bypassed (it should not be).
            Some(
                CanonicalEventType::Response
                | CanonicalEventType::AgentMessage
                | CanonicalEventType::Reasoning
                | CanonicalEventType::TurnContext,
            ) => "model_derived",
            _ => "session_derived",
        },
    }
}

fn classify_verification_status(
    lower: &str,
    event: Option<&SessionEventRecord>,
    from_summary: bool,
) -> &'static str {
    if lower.contains("hypothesis") || lower.contains("guess") || lower.contains("might be") {
        return "unverified";
    }
    if from_summary {
        return "inferred";
    }
    match event.map(|value| value.event_type) {
        Some(CanonicalEventType::Prompt | CanonicalEventType::Annotation) => "user_confirmed",
        Some(
            CanonicalEventType::ToolResult
            | CanonicalEventType::Command
            | CanonicalEventType::FileChange,
        ) => "verified",
        Some(CanonicalEventType::TestResult | CanonicalEventType::Error) => "verified",
        Some(CanonicalEventType::Response) => "inferred",
        _ => "inferred",
    }
}

fn should_admit_claim(
    memory_type: MemoryType,
    authority_class: &str,
    verification_status: &str,
    lower: &str,
) -> bool {
    if lower.contains("hypothesis") || lower.contains("guess") || lower.contains("might be") {
        return false;
    }
    let explicit_task_marker = ["task:", "todo:", "next:", "follow up", "continue "]
        .iter()
        .any(|marker| lower.contains(marker));
    let explicit_question_marker = ["open question:", "question:", "unknown whether", "?"]
        .iter()
        .any(|marker| lower.contains(marker));
    match memory_type {
        MemoryType::Decision | MemoryType::Constraint => {
            matches!(verification_status, "user_confirmed" | "verified")
                && authority_class != "model_derived"
        }
        MemoryType::Task => {
            (matches!(verification_status, "user_confirmed" | "verified")
                || (verification_status == "inferred"
                    && authority_class == "session_derived"
                    && explicit_task_marker))
                && authority_class != "model_derived"
        }
        MemoryType::OpenQuestion => {
            (matches!(verification_status, "user_confirmed" | "verified")
                || (verification_status == "inferred"
                    && authority_class == "session_derived"
                    && explicit_question_marker))
                && authority_class != "model_derived"
        }
        MemoryType::Fact | MemoryType::Bug | MemoryType::Fix | MemoryType::ImplementationDetail => {
            matches!(verification_status, "verified" | "user_confirmed")
                || matches!(
                    authority_class,
                    "tool_verified" | "test_verified" | "user_confirmed"
                )
        }
        MemoryType::Summary | MemoryType::Risk | MemoryType::ChangeLog => false,
    }
}

fn claim_title(memory_type: MemoryType, segment: &str) -> String {
    let prefix = match memory_type {
        MemoryType::Fact => "Fact",
        MemoryType::Decision => "Decision",
        MemoryType::Task => "Task",
        MemoryType::Constraint => "Constraint",
        MemoryType::Bug => "Bug",
        MemoryType::Fix => "Fix",
        MemoryType::OpenQuestion => "Open question",
        MemoryType::Summary => "Summary",
        MemoryType::ImplementationDetail => "Implementation detail",
        MemoryType::ChangeLog => "Change log",
        MemoryType::Risk => "Risk",
    };
    format!("{prefix}: {}", truncate(strip_claim_prefixes(segment), 96))
}

fn claim_importance(memory_type: MemoryType, verification_status: &str) -> f64 {
    let base = match memory_type {
        MemoryType::Decision | MemoryType::Constraint | MemoryType::Fix => 0.92,
        MemoryType::Task | MemoryType::Bug | MemoryType::Fact => 0.86,
        MemoryType::OpenQuestion => 0.7,
        MemoryType::ImplementationDetail => 0.74,
        MemoryType::Summary | MemoryType::Risk | MemoryType::ChangeLog => 0.45,
    };
    match verification_status {
        "verified" | "user_confirmed" => base,
        "inferred" => (base - 0.12).max(0.2),
        "unverified" | "contradicted" => (base - 0.25).max(0.1),
        _ => base,
    }
}

fn claim_confidence(verification_status: &str, from_summary: bool) -> f64 {
    let base: f64 = match verification_status {
        "verified" => 0.9,
        "user_confirmed" => 0.86,
        "inferred" => 0.68,
        "contradicted" => 0.24,
        "unverified" => 0.2,
        _ => 0.5,
    };
    if from_summary {
        (base - 0.08).max(0.1)
    } else {
        base
    }
}

fn claim_key(memory_type: MemoryType, segment: &str, event: Option<&SessionEventRecord>) -> String {
    let anchor = event
        .and_then(|value| value.payload.file_path.clone())
        .unwrap_or_else(|| "global".to_string());
    let canonical = canonical_claim_text(strip_claim_prefixes(segment).as_str());
    format!(
        "{}:{}:{}",
        memory_type_str(memory_type),
        canonical_claim_text(&anchor),
        canonical
    )
}

fn canonical_claim_text(value: &str) -> String {
    value
        .to_lowercase()
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .filter(|token| token.len() >= 3)
        .take(10)
        .collect::<Vec<_>>()
        .join("_")
}

fn strip_claim_prefixes(value: &str) -> String {
    let lowered = value.trim();
    let prefixes = [
        "decision:",
        "decision update:",
        "task:",
        "todo:",
        "next:",
        "constraint:",
        "open question:",
        "question:",
        "bug:",
        "fix:",
        "fact:",
    ];
    for prefix in prefixes {
        if lowered.to_lowercase().starts_with(prefix) {
            return lowered[prefix.len()..].trim().to_string();
        }
    }
    lowered.to_string()
}

fn derivation_name(memory_type: MemoryType, from_summary: bool) -> &'static str {
    match (memory_type, from_summary) {
        (_, true) => "pckc_summary_claim_v1",
        (MemoryType::Decision, false) => "pckc_decision_claim_v1",
        (MemoryType::Task, false) => "pckc_task_claim_v1",
        (MemoryType::Constraint, false) => "pckc_constraint_claim_v1",
        (MemoryType::Bug, false) => "pckc_bug_claim_v1",
        (MemoryType::Fix, false) => "pckc_fix_claim_v1",
        (MemoryType::Fact, false) => "pckc_fact_claim_v1",
        (MemoryType::OpenQuestion, false) => "pckc_open_question_claim_v1",
        (MemoryType::ImplementationDetail, false) => "pckc_implementation_claim_v1",
        _ => "pckc_claim_v1",
    }
}

fn source_class_for_memory_type(memory_type: MemoryType) -> &'static str {
    match memory_type {
        MemoryType::Decision => "decision",
        MemoryType::Task => "task",
        MemoryType::Constraint => "constraint",
        MemoryType::Fact => "fact",
        MemoryType::Bug => "bug",
        MemoryType::Fix => "fix",
        MemoryType::OpenQuestion => "open_question",
        MemoryType::ImplementationDetail => "implementation_detail",
        MemoryType::Summary | MemoryType::ChangeLog => "session_summary",
        MemoryType::Risk => "risk",
    }
}

fn ranking_role_for_memory_type(memory_type: MemoryType) -> &'static str {
    match memory_type {
        MemoryType::Decision => "durable_decision",
        MemoryType::Task => "active_task",
        MemoryType::Constraint => "constraint_guardrail",
        MemoryType::Fact => "project_fact",
        MemoryType::Bug => "known_bug",
        MemoryType::Fix => "verified_fix",
        MemoryType::OpenQuestion => "open_question",
        MemoryType::ImplementationDetail => "implementation_note",
        MemoryType::Summary | MemoryType::ChangeLog => "summary_fallback",
        MemoryType::Risk => "risk",
    }
}

fn is_negative_claim(lower: &str) -> bool {
    [
        "do not",
        "should not",
        "not ",
        "disabled",
        "rejected",
        "fallback only",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
}

fn derive_reflection_memory(
    session_id: Uuid,
    episodes: &[SessionEpisodeDraft],
) -> Option<DerivedMemoryDraft> {
    if episodes.len() < 2 {
        return None;
    }

    // Use insertion-ordered map to match Node.js tie-breaking behavior:
    // Object.entries() returns keys in insertion order, and sort is stable,
    // so on ties the first-inserted key wins (conversation > implementation > debugging).
    let mut type_counts: IndexMap<String, u32> = IndexMap::new();
    type_counts.insert("conversation".to_string(), 0);
    type_counts.insert("implementation".to_string(), 0);
    type_counts.insert("debugging".to_string(), 0);
    for episode in episodes {
        *type_counts.entry(episode.episode_type.clone()).or_default() += 1;
    }

    // Sort descending by count; IndexMap preserves insertion order for equal elements
    let dominant_type = {
        let mut entries: Vec<_> = type_counts.iter().collect();
        entries.sort_by(|a, b| b.1.cmp(a.1));
        entries
            .first()
            .map(|(k, _)| (*k).clone())
            .unwrap_or_else(|| "conversation".to_string())
    };
    let recent = episodes.iter().rev().take(3).collect::<Vec<_>>();
    let unresolved_risk = has_trailing_debugging_without_implementation(episodes);
    let content = truncate(
        [
            format!("Dominant work mode: {dominant_type}"),
            format!(
                "Episode mix: conversation={}, implementation={}, debugging={}",
                type_counts.get("conversation").copied().unwrap_or_default(),
                type_counts
                    .get("implementation")
                    .copied()
                    .unwrap_or_default(),
                type_counts.get("debugging").copied().unwrap_or_default()
            ),
            format!(
                "Recent episode summaries: {}",
                recent
                    .iter()
                    .rev()
                    .map(|episode| truncate(episode.summary.clone(), 120))
                    .collect::<Vec<_>>()
                    .join(" || ")
            ),
            if unresolved_risk {
                "Risk: session ended with unresolved debugging context.".to_string()
            } else {
                "Risk: no unresolved debugging signal detected.".to_string()
            },
        ]
        .join("\n"),
        3000,
    );

    Some(DerivedMemoryDraft {
        memory_type: if unresolved_risk {
            MemoryType::Risk
        } else {
            MemoryType::Summary
        },
        title: if unresolved_risk {
            "Session reflection risk".to_string()
        } else {
            "Session reflection summary".to_string()
        },
        summary: truncate(content.clone(), 300),
        content,
        importance_score: if unresolved_risk { 0.82 } else { 0.68 },
        confidence_score: 0.72,
        provenance_event_ids: episodes
            .iter()
            .flat_map(|episode| episode.provenance_event_ids.iter().copied())
            .collect(),
        metadata: json!({
            "derivation": "session_reflection_v1",
            "sessionId": session_id,
            "dominantEpisodeType": dominant_type,
            "unresolvedRisk": unresolved_risk,
            "proofType": "summary",
            "authorityClass": "session_derived",
            "verificationStatus": "unverified",
            "sourceClass": "reflection",
            "rankingRole": "reflection_fallback",
            "belief": { "admit": false },
        }),
    })
}

#[derive(Debug, Clone)]
struct EpisodeBucket {
    episode_ordinal: i32,
    episode_type: String,
    events: Vec<SessionEventRecord>,
}

impl EpisodeBucket {
    fn new(episode_ordinal: i32, episode_type: String) -> Self {
        Self {
            episode_ordinal,
            episode_type,
            events: Vec::new(),
        }
    }
}

fn classify_event(event: &SessionEventRecord) -> String {
    let lower = event_text(event).to_lowercase();
    let is_errorish = event.event_type == CanonicalEventType::Error
        || event.payload.exit_code.is_some_and(|code| code != 0)
        || lower.contains("failed")
        || lower.contains("error")
        || lower.contains("exception");
    if is_errorish {
        return "debugging".to_string();
    }

    let is_impl = matches!(
        event.event_type,
        CanonicalEventType::Command
            | CanonicalEventType::ToolCall
            | CanonicalEventType::ToolResult
            | CanonicalEventType::FileChange
    ) || lower.contains("apply_patch");
    if is_impl {
        return "implementation".to_string();
    }

    "conversation".to_string()
}

fn should_start_new_episode(
    current_events: &[SessionEventRecord],
    next_event: &SessionEventRecord,
    next_type: &str,
) -> bool {
    if current_events.is_empty() {
        return false;
    }

    let previous = match current_events.last() {
        Some(previous) => previous,
        None => return false,
    };

    if classify_event(previous) != next_type {
        return true;
    }

    let previous_time = time::OffsetDateTime::parse(
        &previous.created_at,
        &time::format_description::well_known::Rfc3339,
    )
    .ok();
    let next_time = time::OffsetDateTime::parse(
        &next_event.created_at,
        &time::format_description::well_known::Rfc3339,
    )
    .ok();
    let gap_minutes = match (previous_time, next_time) {
        (Some(previous_time), Some(next_time)) => {
            let gap = next_time - previous_time;
            (gap.whole_seconds().max(0) as f64) / 60.0
        }
        _ => 0.0,
    };

    current_events.len() >= 8 || gap_minutes > 15.0
}

fn materialize_episode(session_id: Uuid, bucket: &EpisodeBucket) -> SessionEpisodeDraft {
    let texts = bucket
        .events
        .iter()
        .map(event_text)
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    let summary = truncate(texts.join("\n"), 3000);
    let first_text = texts
        .first()
        .cloned()
        .unwrap_or_else(|| bucket.episode_type.clone());
    let title = format!(
        "Episode {}: {} - {}",
        bucket.episode_ordinal,
        bucket.episode_type,
        truncate(first_text, 80)
    );

    SessionEpisodeDraft {
        episode_ordinal: bucket.episode_ordinal,
        episode_type: bucket.episode_type.clone(),
        title,
        summary,
        started_at: bucket
            .events
            .first()
            .map(|event| event.created_at.clone())
            .unwrap_or_else(now_rfc3339),
        ended_at: bucket
            .events
            .last()
            .map(|event| event.created_at.clone())
            .unwrap_or_else(now_rfc3339),
        provenance_event_ids: bucket.events.iter().map(|event| event.id).collect(),
        metadata: {
            use indexmap::IndexSet;
            let event_types: Vec<&str> = bucket
                .events
                .iter()
                .map(|event| canonical_event_type_str(event.event_type))
                .collect::<IndexSet<_>>()
                .into_iter()
                .collect();
            json!({
                "derivation": "session_episode_compaction_v1",
                "sessionId": session_id,
                "eventCount": bucket.events.len(),
                "eventTypes": event_types,
            })
        },
    }
}

fn canonical_event_type_str(event_type: CanonicalEventType) -> &'static str {
    match event_type {
        CanonicalEventType::Prompt => "prompt",
        CanonicalEventType::Response => "response",
        CanonicalEventType::ToolCall => "tool_call",
        CanonicalEventType::ToolResult => "tool_result",
        CanonicalEventType::FileChange => "file_change",
        CanonicalEventType::Command => "command",
        CanonicalEventType::TestResult => "test_result",
        CanonicalEventType::Summary => "summary",
        CanonicalEventType::Error => "error",
        CanonicalEventType::Annotation => "annotation",
        CanonicalEventType::Reasoning => "reasoning",
        CanonicalEventType::TurnContext => "turn_context",
        CanonicalEventType::AgentMessage => "agent_message",
    }
}

fn dedupe_derived_memories(memories: Vec<DerivedMemoryDraft>) -> Vec<DerivedMemoryDraft> {
    let mut keyed: IndexMap<String, DerivedMemoryDraft> = IndexMap::new();
    for memory in memories {
        let episode_ordinal = match memory.metadata.get("episodeOrdinal") {
            Some(Value::Number(n)) => n.to_string(),
            _ => "session".to_string(),
        };
        let type_str = memory_type_str(memory.memory_type);
        let claim_key = memory
            .metadata
            .get("claimKey")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let key = format!(
            "{type_str}:{episode_ordinal}:{claim_key}:{}:{}",
            memory.title, memory.summary
        );
        keyed.entry(key).or_insert(memory);
    }
    keyed.into_values().collect()
}

fn memory_type_str(memory_type: MemoryType) -> &'static str {
    match memory_type {
        MemoryType::Summary => "summary",
        MemoryType::Decision => "decision",
        MemoryType::Task => "task",
        MemoryType::Constraint => "constraint",
        MemoryType::Bug => "bug",
        MemoryType::Fix => "fix",
        MemoryType::OpenQuestion => "open_question",
        MemoryType::ImplementationDetail => "implementation_detail",
        MemoryType::Risk => "risk",
        MemoryType::Fact => "fact",
        MemoryType::ChangeLog => "change_log",
    }
}

fn normalize_error_signature(value: &str) -> String {
    use regex::Regex;
    use std::sync::LazyLock;

    static HEX_RUN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[0-9a-f]{7,}").unwrap());
    static WHITESPACE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+").unwrap());

    let lowered = value.to_lowercase();
    let without_hex = HEX_RUN.replace_all(&lowered, "");
    let collapsed = WHITESPACE.replace_all(&without_hex, " ");
    let trimmed = collapsed.trim();
    trimmed.chars().take(160).collect()
}

fn intersect_count(left: &[String], right: &[String]) -> usize {
    let right_set = right.iter().collect::<HashSet<_>>();
    left.iter()
        .filter(|value| right_set.contains(value))
        .count()
}

fn has_trailing_debugging_without_implementation(episodes: &[SessionEpisodeDraft]) -> bool {
    for episode in episodes.iter().rev() {
        if episode.episode_type == "implementation" {
            return false;
        }
        if episode.episode_type == "debugging" {
            return true;
        }
    }
    false
}

fn truncate(mut value: String, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value;
    }
    value = value.chars().take(max_chars).collect();
    value
}

fn now_rfc3339() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

fn fnv1a32(input: &str) -> u32 {
    let mut hash = 0x811c_9dc5_u32;
    for byte in input.as_bytes() {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;
    use chum_mem_contracts::{CanonicalEventType, EndSessionRequest, SessionEventPayload};

    #[test]
    fn derives_atomic_bug_claim_for_debugging_session() {
        let session_id = Uuid::nil();
        let events = vec![
            SessionEventRecord {
                id: Uuid::from_u128(1),
                event_type: CanonicalEventType::Command,
                payload: SessionEventPayload {
                    command: Some("cargo test".to_string()),
                    ..SessionEventPayload::default()
                },
                created_at: "2026-04-10T00:00:00Z".to_string(),
            },
            SessionEventRecord {
                id: Uuid::from_u128(2),
                event_type: CanonicalEventType::Error,
                payload: SessionEventPayload {
                    message: Some("tests failed".to_string()),
                    ..SessionEventPayload::default()
                },
                created_at: "2026-04-10T00:01:00Z".to_string(),
            },
        ];
        let end_request = EndSessionRequest {
            session_id,
            summary: None,
            metadata: json!({}),
            defer: None,
        };

        let memories =
            derive_memories_from_session(session_id, Provider::Codex, &end_request, &events, None);

        assert!(
            memories
                .iter()
                .any(|memory| memory.memory_type == MemoryType::Bug)
        );
    }

    #[test]
    fn rejects_model_derived_open_question_from_response() {
        let session_id = Uuid::nil();
        let events = vec![SessionEventRecord {
            id: Uuid::from_u128(3),
            event_type: CanonicalEventType::Response,
            payload: SessionEventPayload {
                message: Some(
                    "Open question: maybe the reranker should ignore summaries?".to_string(),
                ),
                ..SessionEventPayload::default()
            },
            created_at: "2026-04-10T00:02:00Z".to_string(),
        }];
        let end_request = EndSessionRequest {
            session_id,
            summary: None,
            metadata: json!({}),
            defer: None,
        };

        let memories =
            derive_memories_from_session(session_id, Provider::Codex, &end_request, &events, None);

        assert!(!memories.iter().any(|memory| {
            memory.memory_type == MemoryType::OpenQuestion
                && memory
                    .metadata
                    .get("belief")
                    .and_then(|value| value.get("admit"))
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
        }));
    }

    #[test]
    fn admits_explicit_task_from_user_prompt() {
        let session_id = Uuid::nil();
        let events = vec![SessionEventRecord {
            id: Uuid::from_u128(4),
            event_type: CanonicalEventType::Prompt,
            payload: SessionEventPayload {
                message: Some("Task: finish the proof compiler for context_build.".to_string()),
                ..SessionEventPayload::default()
            },
            created_at: "2026-04-10T00:03:00Z".to_string(),
        }];
        let end_request = EndSessionRequest {
            session_id,
            summary: None,
            metadata: json!({}),
            defer: None,
        };

        let memories =
            derive_memories_from_session(session_id, Provider::Codex, &end_request, &events, None);

        assert!(memories.iter().any(|memory| {
            memory.memory_type == MemoryType::Task
                && memory
                    .metadata
                    .get("belief")
                    .and_then(|value| value.get("admit"))
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
        }));
    }

    // ── v2.2.1 belief gate tests ────────────────────────────────────

    fn end_request_default(session_id: Uuid) -> EndSessionRequest {
        EndSessionRequest {
            session_id,
            summary: None,
            metadata: json!({}),
            defer: None,
        }
    }

    #[test]
    fn reasoning_event_never_originates_a_claim() {
        // Even when the reasoning text explicitly contains "Decision: ..." —
        // the belief gate must reject it by construction.
        let session_id = Uuid::nil();
        let events = vec![SessionEventRecord {
            id: Uuid::from_u128(10),
            event_type: CanonicalEventType::Reasoning,
            payload: SessionEventPayload {
                message: Some(
                    "Decision: switch to weighted set-cover for context_compile.".to_string(),
                ),
                ..SessionEventPayload::default()
            },
            created_at: "2026-04-15T00:00:00Z".to_string(),
        }];

        let memories = derive_memories_from_session(
            session_id,
            Provider::Codex,
            &end_request_default(session_id),
            &events,
            None,
        );

        // No memories whose provenance references the reasoning event,
        // and definitely no admitted decision.
        assert!(!memories.iter().any(|memory| {
            memory.memory_type == MemoryType::Decision
                && memory.provenance_event_ids.contains(&Uuid::from_u128(10))
        }));
    }

    #[test]
    fn turn_context_event_never_originates_a_claim() {
        let session_id = Uuid::nil();
        let events = vec![SessionEventRecord {
            id: Uuid::from_u128(11),
            event_type: CanonicalEventType::TurnContext,
            payload: SessionEventPayload {
                message: Some("Task: finish the proof compiler for context_build.".to_string()),
                ..SessionEventPayload::default()
            },
            created_at: "2026-04-15T00:01:00Z".to_string(),
        }];

        let memories = derive_memories_from_session(
            session_id,
            Provider::Codex,
            &end_request_default(session_id),
            &events,
            None,
        );

        assert!(!memories.iter().any(|memory| {
            memory.memory_type == MemoryType::Task
                && memory.provenance_event_ids.contains(&Uuid::from_u128(11))
        }));
    }

    #[test]
    fn agent_message_rejected_without_user_confirmation() {
        // AgentMessage routes through the classifier chain and lands in
        // authority_class=model_derived, verification=inferred — which
        // should_admit_claim rejects for every durable type.
        let session_id = Uuid::nil();
        let events = vec![SessionEventRecord {
            id: Uuid::from_u128(12),
            event_type: CanonicalEventType::AgentMessage,
            payload: SessionEventPayload {
                message: Some("Decision: use tokio::select for the worker scheduler.".to_string()),
                ..SessionEventPayload::default()
            },
            created_at: "2026-04-15T00:02:00Z".to_string(),
        }];

        let memories = derive_memories_from_session(
            session_id,
            Provider::Codex,
            &end_request_default(session_id),
            &events,
            None,
        );

        assert!(!memories.iter().any(|memory| {
            memory.memory_type == MemoryType::Decision
                && memory
                    .metadata
                    .get("belief")
                    .and_then(|value| value.get("admit"))
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
        }));
    }
}
