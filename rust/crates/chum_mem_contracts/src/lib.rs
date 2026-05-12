//! Shared request and response contracts for the Rust migration.
//!
//! The goal is contract parity with the existing TypeScript `@chum-mem/contracts`
//! package while moving validation into explicit Rust code instead of runtime Zod
//! schemas.

use serde::{Deserialize, Serialize};
use thiserror::Error;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use uuid::Uuid;

/// Open client/provider identifier used for ingestion and optional filtering.
///
/// Historically this was a closed enum for Claude/Codex/Gemini. The session
/// layer is provider-neutral: any normalized AI client can write/query session
/// memory as long as tenant and project scope pass.
pub type Provider = String;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActorType {
    User,
    Token,
    System,
}

impl std::str::FromStr for ActorType {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "user" => Ok(Self::User),
            "token" => Ok(Self::Token),
            "system" => Ok(Self::System),
            _ => Err("invalid actor type"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TeamRole {
    Owner,
    Admin,
    Member,
}

impl std::str::FromStr for TeamRole {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "owner" => Ok(Self::Owner),
            "admin" => Ok(Self::Admin),
            "member" => Ok(Self::Member),
            _ => Err("invalid team role"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CanonicalEventType {
    Prompt,
    Response,
    ToolCall,
    ToolResult,
    FileChange,
    Command,
    TestResult,
    Summary,
    Error,
    Annotation,
    // v2.2.1: provider-specific semantic events that carry signal but are
    // hard-rejected at the claim extractor (belief gate). See
    // docs/research/v2.2.1-pckc/DESIGN.md §1.
    Reasoning,
    TurnContext,
    AgentMessage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryType {
    Fact,
    Decision,
    Task,
    Constraint,
    Bug,
    Fix,
    OpenQuestion,
    Summary,
    ImplementationDetail,
    ChangeLog,
    Risk,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthorityClass {
    Repository,
    UserConfirmed,
    ToolVerified,
    TestVerified,
    SessionDerived,
    ModelDerived,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationStatus {
    Verified,
    UserConfirmed,
    Inferred,
    Contradicted,
    Unverified,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProofType {
    Repository,
    SessionEvent,
    ToolResult,
    TestResult,
    UserConfirmation,
    Summary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimRelationType {
    Supersedes,
    Contradicts,
    Confirms,
    DependsOn,
    DerivedFrom,
}

/// v2.2.3: Governance state for durable claims.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum GovernanceState {
    #[default]
    Active,
    Pinned,
    Archived,
    Rejected,
}

impl GovernanceState {
    pub fn as_str(self) -> &'static str {
        match self {
            GovernanceState::Active => "active",
            GovernanceState::Pinned => "pinned",
            GovernanceState::Archived => "archived",
            GovernanceState::Rejected => "rejected",
        }
    }

    pub fn is_current(self) -> bool {
        matches!(self, GovernanceState::Active | GovernanceState::Pinned)
    }
}

impl std::str::FromStr for GovernanceState {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "active" => Ok(Self::Active),
            "pinned" => Ok(Self::Pinned),
            "archived" => Ok(Self::Archived),
            "rejected" => Ok(Self::Rejected),
            _ => Err("invalid governance state"),
        }
    }
}

/// v2.2.3: Request to transition a claim's governance state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GovernClaimRequest {
    pub new_state: GovernanceState,
    pub reason: Option<String>,
}

/// v2.2.3: Response from a governance transition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GovernClaimResponse {
    pub claim_id: Uuid,
    pub previous_state: GovernanceState,
    pub new_state: GovernanceState,
    pub transition_id: Uuid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SearchMode {
    Lexical,
    Semantic,
    #[default]
    Hybrid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DisclosureLevel {
    #[default]
    Overview,
    Related,
    Full,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeQueryKind {
    HubNodes,
    ShortestPath,
    Neighbors,
    Communities,
    Search,
    /// v2.2.2: NeuroPath-inspired goal-directed BFS with semantic sub-goal
    /// advancement scoring. Requires `text` (query with sub-goals) and
    /// optionally `node_id` (start node; derived from search if absent).
    GoalDirected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RetrievalIntent {
    None,
    MemoryOnly,
    RepositoryOnly,
    SessionGraphOnly,
    #[default]
    Hybrid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ContextSourceClass {
    #[default]
    Memory,
    Repository,
    SessionGraph,
    Conflict,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RepoContext {
    pub repo_name: Option<String>,
    pub branch: Option<String>,
    pub commit_sha: Option<String>,
    #[serde(default)]
    pub file_paths: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct LocalContext {
    pub hostname: Option<String>,
    pub os: Option<String>,
    pub client_version: Option<String>,
    pub user_agent: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DiffStat {
    #[serde(default)]
    pub added: u32,
    #[serde(default)]
    pub deleted: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SessionEventPayload {
    pub message: Option<String>,
    pub tool_name: Option<String>,
    pub command: Option<String>,
    pub exit_code: Option<i32>,
    pub file_path: Option<String>,
    pub diff_stat: Option<DiffStat>,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartSessionRequest {
    pub provider: Provider,
    pub project_id: Uuid,
    pub external_session_id: String,
    #[serde(default)]
    pub repo: RepoContext,
    #[serde(default)]
    pub local: LocalContext,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartSessionResponse {
    pub session_id: Uuid,
    pub organization_id: Uuid,
    pub team_id: Uuid,
    pub project_id: Uuid,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppendSessionEventRequest {
    pub session_id: Uuid,
    pub event_id: String,
    pub idempotency_key: String,
    pub provider: Provider,
    pub event_type: CanonicalEventType,
    pub event_time: String,
    pub payload: SessionEventPayload,
    pub raw_payload: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppendSessionEventResponse {
    pub event_id: Uuid,
    pub duplicate: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchAppendSessionEventsRequest {
    pub session_id: Uuid,
    pub events: Vec<AppendSessionEventRequest>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchAppendSessionEventsResponse {
    pub inserted: i32,
    pub duplicates: i32,
}

/// Response for bulk index management endpoints.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BulkIndexResponse {
    pub ok: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EndSessionRequest {
    pub session_id: Uuid,
    pub summary: Option<String>,
    #[serde(default)]
    pub metadata: serde_json::Value,
    /// When true, skip inline memory derivation and enqueue it as a worker job.
    /// Used by bulk import for fast throughput.
    #[serde(default)]
    pub defer: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EndSessionResponse {
    pub session_id: Uuid,
    pub status: String,
    pub queued_jobs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProvenanceHandle {
    pub session_id: Uuid,
    pub session_event_id: Uuid,
    pub excerpt: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProofHandle {
    pub proof_type: ProofType,
    pub source_ref: String,
    pub excerpt: Option<String>,
    pub session_id: Option<Uuid>,
    pub session_event_id: Option<Uuid>,
    pub authority_class: Option<AuthorityClass>,
    pub verification_status: Option<VerificationStatus>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaimRelation {
    pub claim_id: Uuid,
    pub related_claim_id: Uuid,
    pub related_memory_id: Option<Uuid>,
    pub relation_type: ClaimRelationType,
    pub direction: String,
    pub title: Option<String>,
    pub summary: Option<String>,
    pub authority_class: Option<AuthorityClass>,
    pub verification_status: Option<VerificationStatus>,
}

fn default_limit() -> u32 {
    10
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemorySearchRequest {
    pub query: String,
    pub project_id: Option<Uuid>,
    pub session_id: Option<Uuid>,
    pub provider: Option<Provider>,
    pub branch: Option<String>,
    #[serde(default)]
    pub types: Vec<MemoryType>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    #[serde(default)]
    pub mode: SearchMode,
    #[serde(default)]
    pub disclosure_level: DisclosureLevel,
    pub retrieval_intent: Option<RetrievalIntent>,
    pub include_historical: Option<bool>,
    #[serde(default = "default_limit")]
    pub limit: u32,
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryHit {
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
    pub source_class: Option<String>,
    pub ranking_role: Option<String>,
    pub claim_id: Option<Uuid>,
    pub claim_key: Option<String>,
    pub claim_type: Option<MemoryType>,
    pub authority_class: Option<AuthorityClass>,
    pub verification_status: Option<VerificationStatus>,
    pub valid_from: Option<String>,
    pub valid_to: Option<String>,
    pub superseded_by: Option<Uuid>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemorySearchResponse {
    #[serde(default)]
    pub hits: Vec<MemoryHit>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetMemoryResponse {
    pub id: Uuid,
    pub project_id: Uuid,
    #[serde(rename = "type")]
    pub memory_type: MemoryType,
    pub title: String,
    pub content: String,
    pub summary: String,
    #[serde(default)]
    pub metadata: serde_json::Value,
    #[serde(default)]
    pub provenance: Vec<ProvenanceHandle>,
    #[serde(default)]
    pub proof_handles: Vec<ProofHandle>,
    #[serde(default)]
    pub related_memory_ids: Vec<Uuid>,
    #[serde(default)]
    pub claim_relations: Vec<ClaimRelation>,
    pub claim_id: Option<Uuid>,
    pub claim_key: Option<String>,
    pub claim_type: Option<MemoryType>,
    pub authority_class: Option<AuthorityClass>,
    pub verification_status: Option<VerificationStatus>,
    pub valid_from: Option<String>,
    pub valid_to: Option<String>,
    pub superseded_by: Option<Uuid>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextBuildRequest {
    pub provider: Provider,
    pub objective: String,
    pub retrieval_intent: Option<RetrievalIntent>,
    pub include_historical: Option<bool>,
    pub project_id: Option<Uuid>,
    pub branch: Option<String>,
    #[serde(default)]
    pub file_paths: Vec<String>,
    pub max_token_budget: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextItem {
    pub memory_id: Option<Uuid>,
    pub reference_id: Option<String>,
    #[serde(default)]
    pub source_class: ContextSourceClass,
    pub ranking_role: Option<String>,
    #[serde(rename = "type")]
    pub memory_type: MemoryType,
    pub title: String,
    pub summary: String,
    pub tokens: u32,
    #[serde(default)]
    pub provenance: Vec<ProvenanceHandle>,
    #[serde(default)]
    pub proof_handles: Vec<ProofHandle>,
    pub claim_id: Option<Uuid>,
    pub claim_key: Option<String>,
    pub claim_type: Option<MemoryType>,
    pub authority_class: Option<AuthorityClass>,
    pub verification_status: Option<VerificationStatus>,
    pub valid_from: Option<String>,
    pub valid_to: Option<String>,
    pub superseded_by: Option<Uuid>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ContextPack {
    #[serde(default)]
    pub current_truth: Vec<ContextItem>,
    #[serde(default)]
    pub project_facts: Vec<ContextItem>,
    #[serde(default)]
    pub recent_decisions: Vec<ContextItem>,
    #[serde(default)]
    pub active_tasks: Vec<ContextItem>,
    #[serde(default)]
    pub constraints: Vec<ContextItem>,
    #[serde(default)]
    pub known_bugs: Vec<ContextItem>,
    #[serde(default)]
    pub verified_fixes: Vec<ContextItem>,
    #[serde(default)]
    pub open_questions: Vec<ContextItem>,
    #[serde(default)]
    pub implementation_notes: Vec<ContextItem>,
    #[serde(default)]
    pub repository_knowledge: Vec<ContextItem>,
    #[serde(default)]
    pub session_continuity: Vec<ContextItem>,
    #[serde(default)]
    pub conflicts: Vec<ContextItem>,
    #[serde(default)]
    pub proof_handles: Vec<ProofHandle>,
    #[serde(default)]
    pub unknowns: Vec<String>,
    #[serde(default)]
    pub recommended_verification: Vec<String>,
    #[serde(default)]
    pub sources: Vec<ProvenanceHandle>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenUsage {
    pub budget: u32,
    pub used: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextBuildResponse {
    pub context_pack: ContextPack,
    pub token_usage: TokenUsage,
    pub retrieval_intent: RetrievalIntent,
}

fn default_depth() -> Option<u32> {
    Some(1)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeQueryRequest {
    pub project_id: Option<Uuid>,
    pub query: KnowledgeQueryKind,
    pub node_id: Option<String>,
    pub target_node_id: Option<String>,
    pub text: Option<String>,
    #[serde(default = "default_depth")]
    pub depth: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectImportRequest {
    pub root_dir: String,
    pub out_dir: Option<String>,
    pub project_id: Option<Uuid>,
    #[serde(default = "default_true")]
    pub update: bool,
    #[serde(default)]
    pub no_viz: bool,
    #[serde(default = "default_true")]
    pub merge_with_existing: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectImportResponse {
    pub status: String,
    pub project_id: Uuid,
    pub imported_root: String,
    pub merged_with_existing: bool,
    pub generated_at: String,
    pub stats: ProjectImportStats,
    pub artifacts: ProjectImportArtifacts,
    pub graph_summary: ProjectImportGraphSummary,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectImportStats {
    pub processed_files: u32,
    pub reused_files: u32,
    pub removed_files: u32,
    pub total_files: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectImportArtifacts {
    pub graph_json_path: String,
    pub report_path: String,
    pub html_path: Option<String>,
    pub cache_manifest_path: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectImportGraphSummary {
    pub node_count: u32,
    pub edge_count: u32,
    pub community_count: u32,
    pub evidence_distribution: EvidenceDistributionContract,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncFileEntry {
    pub path: String,
    pub hash: String,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub bytes_base64: Option<String>,
    #[serde(default)]
    pub media_type: Option<String>,
    #[serde(default)]
    pub size_bytes: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepositorySyncRequest {
    pub project_id: Option<Uuid>,
    pub files: Vec<SyncFileEntry>,
    #[serde(default)]
    pub removed_paths: Vec<String>,
    #[serde(default)]
    pub manifest: std::collections::HashMap<String, String>,
    #[serde(default = "default_true")]
    pub merge_with_existing: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepositorySyncResponse {
    pub status: String,
    pub project_id: Uuid,
    pub merged_with_existing: bool,
    pub generated_at: String,
    pub stats: RepositorySyncStats,
    pub graph_summary: ProjectImportGraphSummary,
    #[serde(default)]
    pub accepted_paths: Vec<String>,
    #[serde(default)]
    pub missing_paths: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepositorySyncStats {
    pub files_added: u32,
    pub files_removed: u32,
    pub files_unchanged: u32,
    pub total_files: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncRulesResponse {
    pub code_extensions: Vec<String>,
    pub doc_extensions: Vec<String>,
    #[serde(default)]
    pub binary_extensions: Vec<String>,
    pub ignore_dirs: Vec<String>,
    pub ignore_files: Vec<String>,
    pub ignore_patterns: Vec<String>,
    pub max_file_size_bytes: u64,
    #[serde(default)]
    pub max_binary_file_size_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceDistributionContract {
    pub extracted: u32,
    pub inferred: u32,
    pub ambiguous: u32,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryBatchRequest {
    pub ids: Vec<Uuid>,
}

#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum ValidationError {
    #[error("field `{field}` must not be blank")]
    BlankField { field: &'static str },
    #[error("field `{field}` must contain at least {min} characters")]
    TooShort { field: &'static str, min: usize },
    #[error("field `{field}` must contain at most {max} characters")]
    TooLong { field: &'static str, max: usize },
    #[error("field `{field}` is outside the allowed range")]
    OutOfRange { field: &'static str },
    #[error("field `{field}` contains an invalid RFC3339 timestamp")]
    InvalidTimestamp { field: &'static str },
    #[error("field `{field}` requires at least one value")]
    EmptyCollection { field: &'static str },
    #[error("knowledge_query(shortest_path) requires both nodeId and targetNodeId")]
    MissingShortestPathNodes,
    #[error("knowledge_query(neighbors) requires nodeId")]
    MissingNodeId,
    #[error("knowledge_query(search) requires text or nodeId")]
    MissingSearchText,
}

pub trait ValidateInput {
    fn validate(&self) -> Result<(), ValidationError>;
}

impl ValidateInput for StartSessionRequest {
    fn validate(&self) -> Result<(), ValidationError> {
        require_non_blank("provider", &self.provider)?;
        ensure_max_len("provider", &self.provider, 64)?;
        require_non_blank("externalSessionId", &self.external_session_id)?;
        ensure_max_len("externalSessionId", &self.external_session_id, 256)?;
        for file_path in &self.repo.file_paths {
            require_non_blank("repo.filePaths[]", file_path)?;
        }
        Ok(())
    }
}

impl ValidateInput for AppendSessionEventRequest {
    fn validate(&self) -> Result<(), ValidationError> {
        require_non_blank("provider", &self.provider)?;
        ensure_max_len("provider", &self.provider, 64)?;
        require_non_blank("eventId", &self.event_id)?;
        ensure_max_len("eventId", &self.event_id, 256)?;
        require_non_blank("idempotencyKey", &self.idempotency_key)?;
        ensure_min_len("idempotencyKey", &self.idempotency_key, 8)?;
        ensure_max_len("idempotencyKey", &self.idempotency_key, 256)?;
        parse_timestamp("eventTime", &self.event_time)?;
        Ok(())
    }
}

impl ValidateInput for BatchAppendSessionEventsRequest {
    fn validate(&self) -> Result<(), ValidationError> {
        if self.events.is_empty() {
            return Err(ValidationError::EmptyCollection { field: "events" });
        }
        if self.events.len() > 500 {
            return Err(ValidationError::OutOfRange { field: "events" });
        }
        for event in &self.events {
            event.validate()?;
        }
        Ok(())
    }
}

impl ValidateInput for EndSessionRequest {
    fn validate(&self) -> Result<(), ValidationError> {
        if let Some(summary) = &self.summary {
            ensure_max_len("summary", summary, 10_000)?;
        }
        Ok(())
    }
}

impl ValidateInput for MemorySearchRequest {
    fn validate(&self) -> Result<(), ValidationError> {
        require_non_blank("query", &self.query)?;
        ensure_limit("limit", self.limit, 1, 50)?;
        if let Some(from) = &self.from {
            parse_timestamp("from", from)?;
        }
        if let Some(to) = &self.to {
            parse_timestamp("to", to)?;
        }
        Ok(())
    }
}

impl ValidateInput for ContextBuildRequest {
    fn validate(&self) -> Result<(), ValidationError> {
        require_non_blank("objective", &self.objective)?;
        ensure_limit("maxTokenBudget", self.max_token_budget, 1, 64_000)?;
        for file_path in &self.file_paths {
            require_non_blank("filePaths[]", file_path)?;
        }
        Ok(())
    }
}

impl ValidateInput for KnowledgeQueryRequest {
    fn validate(&self) -> Result<(), ValidationError> {
        if let Some(depth) = self.depth {
            ensure_limit("depth", depth, 1, 5)?;
        }

        match self.query {
            KnowledgeQueryKind::ShortestPath => {
                if self.node_id.as_deref().is_none() || self.target_node_id.as_deref().is_none() {
                    return Err(ValidationError::MissingShortestPathNodes);
                }
            }
            KnowledgeQueryKind::Neighbors => {
                if self.node_id.as_deref().is_none() {
                    return Err(ValidationError::MissingNodeId);
                }
            }
            KnowledgeQueryKind::Search => {
                let has_node = self.node_id.as_deref().is_some();
                let has_text = self
                    .text
                    .as_deref()
                    .is_some_and(|value| !value.trim().is_empty());
                if !has_node && !has_text {
                    return Err(ValidationError::MissingSearchText);
                }
            }
            KnowledgeQueryKind::HubNodes | KnowledgeQueryKind::Communities => {}
            KnowledgeQueryKind::GoalDirected => {
                // Requires text to parse sub-goals; node_id is optional
                // (derived from search if absent).
                let has_text = self
                    .text
                    .as_deref()
                    .is_some_and(|value| !value.trim().is_empty());
                if !has_text {
                    return Err(ValidationError::MissingSearchText);
                }
            }
        }

        Ok(())
    }
}

impl ValidateInput for ProjectImportRequest {
    fn validate(&self) -> Result<(), ValidationError> {
        require_non_blank("rootDir", &self.root_dir)
    }
}

impl ValidateInput for MemoryBatchRequest {
    fn validate(&self) -> Result<(), ValidationError> {
        if self.ids.is_empty() {
            return Err(ValidationError::EmptyCollection { field: "ids" });
        }
        ensure_limit("ids", self.ids.len() as u32, 1, 20)
    }
}

fn require_non_blank(field: &'static str, value: &str) -> Result<(), ValidationError> {
    if value.trim().is_empty() {
        return Err(ValidationError::BlankField { field });
    }
    Ok(())
}

fn ensure_min_len(field: &'static str, value: &str, min: usize) -> Result<(), ValidationError> {
    if value.len() < min {
        return Err(ValidationError::TooShort { field, min });
    }
    Ok(())
}

fn ensure_max_len(field: &'static str, value: &str, max: usize) -> Result<(), ValidationError> {
    if value.len() > max {
        return Err(ValidationError::TooLong { field, max });
    }
    Ok(())
}

fn ensure_limit(
    field: &'static str,
    value: u32,
    min: u32,
    max: u32,
) -> Result<(), ValidationError> {
    if value < min || value > max {
        return Err(ValidationError::OutOfRange { field });
    }
    Ok(())
}

fn parse_timestamp(field: &'static str, value: &str) -> Result<(), ValidationError> {
    OffsetDateTime::parse(value, &Rfc3339)
        .map(|_| ())
        .map_err(|_| ValidationError::InvalidTimestamp { field })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_blank_search_query() {
        let request = MemorySearchRequest {
            query: "   ".to_string(),
            project_id: None,
            session_id: None,
            provider: None,
            branch: None,
            types: Vec::new(),
            tags: Vec::new(),
            from: None,
            to: None,
            mode: SearchMode::Hybrid,
            disclosure_level: DisclosureLevel::Overview,
            retrieval_intent: None,
            include_historical: None,
            limit: 10,
            cursor: None,
        };

        assert_eq!(
            request.validate(),
            Err(ValidationError::BlankField { field: "query" })
        );
    }

    #[test]
    fn accepts_valid_append_event_request() {
        let request = AppendSessionEventRequest {
            session_id: Uuid::nil(),
            event_id: "evt-123".to_string(),
            idempotency_key: "abcdefgh".to_string(),
            provider: "cursor".to_string(),
            event_type: CanonicalEventType::Command,
            event_time: "2026-04-10T12:00:00Z".to_string(),
            payload: SessionEventPayload::default(),
            raw_payload: serde_json::json!({}),
            turn_id: None,
        };

        assert!(request.validate().is_ok());
    }

    // ── v2.2.3: Governance state tests ────────────────────────────

    #[test]
    fn governance_state_parse_roundtrip() {
        for (text, expected) in [
            ("active", GovernanceState::Active),
            ("pinned", GovernanceState::Pinned),
            ("archived", GovernanceState::Archived),
            ("rejected", GovernanceState::Rejected),
        ] {
            let parsed: GovernanceState = text.parse().unwrap();
            assert_eq!(parsed, expected);
            assert_eq!(parsed.as_str(), text);
        }
    }

    #[test]
    fn governance_state_invalid_parse() {
        assert!("bogus".parse::<GovernanceState>().is_err());
        assert!("".parse::<GovernanceState>().is_err());
        assert!("Active".parse::<GovernanceState>().is_err()); // case-sensitive
    }

    #[test]
    fn governance_is_current() {
        assert!(GovernanceState::Active.is_current());
        assert!(GovernanceState::Pinned.is_current());
        assert!(!GovernanceState::Archived.is_current());
        assert!(!GovernanceState::Rejected.is_current());
    }

    #[test]
    fn governance_default_is_active() {
        assert_eq!(GovernanceState::default(), GovernanceState::Active);
    }

    #[test]
    fn governance_serde_roundtrip() {
        let request = GovernClaimRequest {
            new_state: GovernanceState::Pinned,
            reason: Some("critical invariant".to_string()),
        };
        let json = serde_json::to_string(&request).unwrap();
        let parsed: GovernClaimRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.new_state, GovernanceState::Pinned);
        assert_eq!(parsed.reason.as_deref(), Some("critical invariant"));
    }

    #[test]
    fn governance_response_serde() {
        let response = GovernClaimResponse {
            claim_id: Uuid::nil(),
            previous_state: GovernanceState::Active,
            new_state: GovernanceState::Archived,
            transition_id: Uuid::nil(),
        };
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"previousState\""));
        assert!(json.contains("\"archived\""));
    }
}
