use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;

use anyhow::Context;
use axum::extract::{DefaultBodyLimit, Path, Query, Request, State};
use axum::http::header::{ACCEPT, CONTENT_TYPE, HeaderName};
use axum::http::{Method, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use chum_mem_app::{
    ErrorBody, HealthResponse, ReadyResponse, ServiceMetadata, build_health_response, init_tracing,
    shutdown_signal,
};
use chum_mem_config::{AppConfig, ServiceKind};
use chum_mem_contracts::{
    AppendSessionEventRequest, AppendSessionEventResponse, AuthorityClass,
    BatchAppendSessionEventsRequest, BatchAppendSessionEventsResponse, BulkIndexResponse,
    ClaimRelation,
    ClaimRelationType, ContextBuildRequest, ContextBuildResponse, ContextItem, ContextSourceClass,
    EndSessionRequest, EndSessionResponse, GetMemoryResponse, GovernClaimRequest,
    GovernClaimResponse, GovernanceState, KnowledgeQueryKind,
    KnowledgeQueryRequest, MemoryBatchRequest, MemorySearchRequest, MemoryType,
    ProjectImportGraphSummary, ProofHandle, ProofType, Provider, RepositorySyncRequest,
    RepositorySyncResponse, RepositorySyncStats, RetrievalIntent, StartSessionRequest,
    StartSessionResponse, SyncRulesResponse, ValidateInput, VerificationStatus,
};
use chum_mem_db::{
    AppendSessionEventParams, ClaimProofInsertParams, ClaimProofRow, ClaimRelationRow,
    ClaimUpsertParams, DashboardGraphEdgeRow, DashboardGraphNodeRow, Database, DbError,
    MemoryInsertParams, MemoryProvenanceRow, MemorySearchRow, RepositoryContext, SessionEventRow,
    append_memory_provenance_batch, append_memory_provenance_preview, apply_repository_context,
    check_readiness, create_session_replay, enqueue_worker_job, ensure_scope_entities,
    bulk_insert_session_events_copy, create_session_events_indexes,
    drop_session_events_indexes, insert_audit_log, insert_memory, insert_session_event,
    insert_session_events_batch,
    load_claim_proofs, load_claim_relations_for_memory_ids, load_dashboard_summary,
    load_memories_batch, load_memory, load_memory_edges_for_ids, load_memory_graph_edges,
    load_memory_graph_nodes, load_memory_provenance, load_memory_search_rows, load_session_events,
    load_session_graph_weights, mark_session_completed, replace_claim_proofs, resolve_session,
    resolve_session_events_for_candidate, upsert_claim, upsert_embedding, upsert_ingested_project,
    upsert_session, upsert_session_edge, upsert_session_episodes_batch,
};
use chum_mem_pipeline::{
    CHROMA_EMBEDDING_DIMENSIONS, ChromaQueryResult, GraphProjection, GraphQueryResponse,
    KnowledgeGraph, MemorySearchEnvelope, RankedMemory, RankingContext, SearchMetrics,
    SessionEventRecord, build_context_pack, build_session_completion_job_plan,
    community_relevance_from_query, compile_minimal_proof_set, derive_memories_from_session,
    derive_session_episodes, embed_text, event_text, generate_knowledge_report,
    memory_community_map, merge_graphs, merge_hybrid_results, progressive_disclosure,
    query_chroma_memories_typed, rank_hybrid_results, run_knowledge_query,
    score_session_relationship, to_node_link_json,
};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{Postgres, Row, Transaction};
use time::OffsetDateTime;
use tokio::sync::RwLock;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing::info;
use uuid::Uuid;

const API_ROUTES: &[&str] = &[
    "GET /health",
    "GET /ready",
    "POST /api/search",
    "GET /api/dashboard/summary",
    "GET /api/dashboard/graph",
    "GET /api/knowledge/export",
    "GET /api/knowledge/report",
    "POST /api/knowledge/query",
    "GET /api/knowledge/communities",
    "POST /api/knowledge/import-project",
    "GET /api/memory/:id",
    "POST /api/memory/batch",
    "POST /api/context/build",
    "POST /v1/projects/resolve",
    "POST /v1/ingest/session/start",
    "POST /v1/ingest/session/event",
    "POST /v1/ingest/session/events",
    "POST /v1/ingest/session/events/bulk",
    "POST /v1/ingest/bulk/drop-indexes",
    "POST /v1/ingest/bulk/create-indexes",
    "POST /v1/ingest/session/end",
    "POST /mcp",
    "GET /mcp",
    "DELETE /mcp",
];

const SEARCH_PROVENANCE_LIMIT_DEFAULT: i64 = 2;
const CONTEXT_BUILD_SEARCH_LIMIT: u32 = 12;
const GLOBAL_PROJECT_SLUG: &str = "global";

/// Cached community data extracted from the session knowledge graph.
/// Scoped by project_id to prevent cross-project contamination.
#[derive(Clone, Default)]
struct CommunityCache {
    project_id: Option<Uuid>,
    community_relevance: HashMap<usize, f64>,
    memory_community: HashMap<Uuid, usize>,
    loaded_at: Option<Instant>,
}

#[derive(Clone)]
struct ApiState {
    config: Arc<AppConfig>,
    db: Database,
    scope: RepositoryContext,
    metadata: ServiceMetadata,
    started_at: OffsetDateTime,
    http_client: Client,
    mcp_sessions: Arc<RwLock<HashSet<String>>>,
    community_cache: Arc<RwLock<CommunityCache>>,
}

#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    body: ErrorBody,
}

impl ApiError {
    fn validation(error: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            body: ErrorBody {
                error: error.into(),
            },
        }
    }

    fn bad_request(error: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            body: ErrorBody {
                error: error.into(),
            },
        }
    }

    fn not_found(error: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            body: ErrorBody {
                error: error.into(),
            },
        }
    }

    fn internal(error: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            body: ErrorBody {
                error: error.into(),
            },
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, Json(self.body)).into_response()
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectScopedQuery {
    project_id: Option<Uuid>,
    layer: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DashboardSummaryResponse {
    total_memories: i64,
    total_sessions: i64,
    total_projects: i64,
    estimated_token_savings: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DashboardGraphResponse {
    nodes: Vec<GraphNode>,
    links: Vec<GraphLink>,
    projection: GraphProjection,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct GraphNode {
    id: String,
    label: String,
    #[serde(rename = "type")]
    node_type: String,
    summary: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct GraphLink {
    source: String,
    target: String,
    relation: String,
    weight: f64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct KnowledgeExportResponse {
    project_id: Option<Uuid>,
    generated_at: String,
    graph: DashboardGraphResponse,
    node_link_json: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct KnowledgeReportResponse {
    project_id: Option<Uuid>,
    generated_at: String,
    report_markdown: String,
    /// v2.2.3: Structured cross-layer summary for unified reports.
    #[serde(skip_serializing_if = "Option::is_none")]
    cross_layer_summary: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct KnowledgeCommunitiesResponse {
    project_id: Option<Uuid>,
    communities: Vec<GraphCommunity>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct GraphCommunity {
    community_id: usize,
    node_count: usize,
    representative_nodes: Vec<String>,
    level: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    community_path: Option<String>,
}

#[derive(Debug)]
struct SnapshotArtifacts {
    report_markdown: Option<String>,
    node_link_json: Option<String>,
    computed_at: Option<String>,
}

async fn health(State(state): State<ApiState>) -> Json<HealthResponse> {
    Json(build_health_response(
        state.metadata,
        state.config.as_ref(),
        state.started_at,
        API_ROUTES,
    ))
}

async fn ready(State(state): State<ApiState>) -> Result<Response, ApiError> {
    let report = check_readiness(state.db.pool(), state.config.as_ref())
        .await
        .map_err(|error| ApiError::internal(error.to_string()))?;
    let status = if report.is_ready() {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    let body: ReadyResponse = report.into_response(state.metadata);
    Ok((status, Json(body)).into_response())
}

async fn search(
    State(state): State<ApiState>,
    Json(input): Json<MemorySearchRequest>,
) -> Result<Response, ApiError> {
    input
        .validate()
        .map_err(|error| ApiError::validation(error.to_string()))?;
    let response = perform_search(&state, input)
        .await
        .map_err(map_domain_error)?;
    Ok((StatusCode::OK, Json(response)).into_response())
}

async fn session_start(
    State(state): State<ApiState>,
    Json(input): Json<StartSessionRequest>,
) -> Result<Response, ApiError> {
    input
        .validate()
        .map_err(|error| ApiError::validation(error.to_string()))?;

    let response = perform_session_start(&state, input)
        .await
        .map_err(map_domain_error)?;
    Ok((StatusCode::OK, Json(response)).into_response())
}

async fn session_event(
    State(state): State<ApiState>,
    Json(input): Json<AppendSessionEventRequest>,
) -> Result<Response, ApiError> {
    input
        .validate()
        .map_err(|error| ApiError::validation(error.to_string()))?;

    let response = perform_session_event(&state, input)
        .await
        .map_err(map_domain_error)?;
    Ok((StatusCode::OK, Json(response)).into_response())
}

async fn session_events_batch(
    State(state): State<ApiState>,
    Json(input): Json<BatchAppendSessionEventsRequest>,
) -> Result<Response, ApiError> {
    input
        .validate()
        .map_err(|error| ApiError::validation(error.to_string()))?;
    let response = perform_session_events_batch(&state, input)
        .await
        .map_err(map_domain_error)?;
    Ok((StatusCode::OK, Json(response)).into_response())
}

/// Bulk event insert using COPY FROM STDIN through an UNLOGGED staging table.
/// Same request shape as the regular batch endpoint but uses the high-throughput
/// COPY path with deferred constraint checking.
async fn session_events_bulk(
    State(state): State<ApiState>,
    Json(input): Json<BatchAppendSessionEventsRequest>,
) -> Result<Response, ApiError> {
    input
        .validate()
        .map_err(|error| ApiError::validation(error.to_string()))?;

    let response = perform_session_events_bulk(&state, input)
        .await
        .map_err(map_domain_error)?;
    Ok((StatusCode::OK, Json(response)).into_response())
}

async fn bulk_drop_indexes(
    State(state): State<ApiState>,
) -> Result<Response, ApiError> {
    drop_session_events_indexes(state.db.pool())
        .await
        .map_err(|e| map_domain_error(DomainError::Internal(e.to_string())))?;
    Ok((StatusCode::OK, Json(BulkIndexResponse { ok: true })).into_response())
}

async fn bulk_create_indexes(
    State(state): State<ApiState>,
) -> Result<Response, ApiError> {
    create_session_events_indexes(state.db.pool())
        .await
        .map_err(|e| map_domain_error(DomainError::Internal(e.to_string())))?;
    Ok((StatusCode::OK, Json(BulkIndexResponse { ok: true })).into_response())
}

async fn session_end(
    State(state): State<ApiState>,
    Json(input): Json<EndSessionRequest>,
) -> Result<Response, ApiError> {
    input
        .validate()
        .map_err(|error| ApiError::validation(error.to_string()))?;
    let response = perform_session_end(&state, input)
        .await
        .map_err(map_domain_error)?;
    Ok((StatusCode::OK, Json(response)).into_response())
}

async fn context_build(
    State(state): State<ApiState>,
    Json(input): Json<ContextBuildRequest>,
) -> Result<Response, ApiError> {
    input
        .validate()
        .map_err(|error| ApiError::validation(error.to_string()))?;
    let response = perform_context_build(&state, input)
        .await
        .map_err(map_domain_error)?;
    Ok((StatusCode::OK, Json(response)).into_response())
}

async fn knowledge_query(
    State(state): State<ApiState>,
    Json(input): Json<KnowledgeQueryRequest>,
) -> Result<Response, ApiError> {
    input
        .validate()
        .map_err(|error| ApiError::validation(error.to_string()))?;
    let response = perform_knowledge_query(&state, input, None)
        .await
        .map_err(map_domain_error)?;
    Ok((StatusCode::OK, Json(response)).into_response())
}

async fn sync_rules() -> Result<Response, ApiError> {
    let rules = chum_mem_pipeline::sync_rules();
    let response = SyncRulesResponse {
        code_extensions: rules.code_extensions,
        doc_extensions: rules.doc_extensions,
        ignore_dirs: rules.ignore_dirs,
        ignore_files: rules.ignore_files,
        ignore_patterns: rules.ignore_patterns,
        max_file_size_bytes: rules.max_file_size_bytes,
    };
    Ok((StatusCode::OK, Json(response)).into_response())
}

async fn repository_sync(
    State(state): State<ApiState>,
    Json(input): Json<RepositorySyncRequest>,
) -> Result<Response, ApiError> {
    let response = perform_repository_sync(&state, input)
        .await
        .map_err(map_domain_error)?;
    Ok((StatusCode::OK, Json(response)).into_response())
}

async fn memory_batch(
    State(state): State<ApiState>,
    Json(input): Json<MemoryBatchRequest>,
) -> Result<Response, ApiError> {
    input
        .validate()
        .map_err(|error| ApiError::validation(error.to_string()))?;
    let memories = perform_memory_batch(&state, input.ids)
        .await
        .map_err(map_domain_error)?;
    Ok((StatusCode::OK, Json(json!({ "memories": memories }))).into_response())
}

async fn memory_get(
    State(state): State<ApiState>,
    Path(memory_id): Path<Uuid>,
) -> Result<Response, ApiError> {
    let response = perform_memory_get(&state, memory_id)
        .await
        .map_err(map_domain_error)?;
    Ok((StatusCode::OK, Json(response)).into_response())
}

/// v2.2.3: Governance transition endpoint.
async fn claim_govern(
    State(state): State<ApiState>,
    Path(claim_id): Path<Uuid>,
    Json(input): Json<GovernClaimRequest>,
) -> Result<Response, ApiError> {
    let response = perform_claim_govern(&state, claim_id, input)
        .await
        .map_err(map_domain_error)?;
    Ok((StatusCode::OK, Json(response)).into_response())
}

async fn dashboard_summary(State(state): State<ApiState>) -> Result<Response, ApiError> {
    let response = perform_dashboard_summary(&state)
        .await
        .map_err(map_domain_error)?;
    Ok((StatusCode::OK, Json(response)).into_response())
}

async fn dashboard_graph(
    State(state): State<ApiState>,
    Query(query): Query<ProjectScopedQuery>,
) -> Result<Response, ApiError> {
    let response = perform_dashboard_graph(&state, query.project_id, query.layer.as_deref())
        .await
        .map_err(map_domain_error)?;
    Ok((StatusCode::OK, Json(response)).into_response())
}

async fn knowledge_export(
    State(state): State<ApiState>,
    Query(query): Query<ProjectScopedQuery>,
) -> Result<Response, ApiError> {
    let response = perform_knowledge_export(&state, query.project_id, query.layer.as_deref())
        .await
        .map_err(map_domain_error)?;
    Ok((StatusCode::OK, Json(response)).into_response())
}

async fn knowledge_report(
    State(state): State<ApiState>,
    Query(query): Query<ProjectScopedQuery>,
) -> Result<Response, ApiError> {
    let response = perform_knowledge_report(&state, query.project_id, query.layer.as_deref())
        .await
        .map_err(map_domain_error)?;

    // v2.2.3: For unified reports, return JSON with structured cross-layer
    // summary so the benchmark (and clients) can inspect fields directly.
    if query.layer.as_deref() == Some("unified") {
        return Ok((
            StatusCode::OK,
            Json(json!({
                "report": {
                    "crossLayerSummary": response.cross_layer_summary,
                    "repository": !response.report_markdown.is_empty(),
                    "session": !response.report_markdown.is_empty(),
                    "markdown": response.report_markdown,
                },
            })),
        )
            .into_response());
    }

    Ok((
        StatusCode::OK,
        [(CONTENT_TYPE, "text/markdown; charset=utf-8")],
        response.report_markdown,
    )
        .into_response())
}

async fn knowledge_communities(
    State(state): State<ApiState>,
    Query(query): Query<ProjectScopedQuery>,
) -> Result<Response, ApiError> {
    let response = perform_knowledge_communities(&state, query.project_id, query.layer.as_deref())
        .await
        .map_err(map_domain_error)?;
    Ok((StatusCode::OK, Json(response)).into_response())
}

// ── MCP JSON-RPC Protocol ──

const MCP_PROTOCOL_VERSION: &str = "2025-03-26";

fn mcp_tool_definitions() -> Vec<Value> {
    vec![
        json!({"name":"session_start","description":"Start or resume a provider session for ingestion","inputSchema":{"type":"object","properties":{"provider":{"type":"string","enum":["claude","codex","gemini"]},"projectId":{"type":"string","format":"uuid"},"externalSessionId":{"type":"string"},"repo":{"type":"object","properties":{"repoUrl":{"type":"string"},"repoName":{"type":"string"},"branch":{"type":"string"},"commitSha":{"type":"string"},"filePaths":{"type":"array","items":{"type":"string"}}}},"local":{"type":"object","properties":{"hostname":{"type":"string"},"os":{"type":"string"},"clientVersion":{"type":"string"},"userAgent":{"type":"string"}}},"metadata":{"type":"object"}},"required":["provider","projectId","externalSessionId"]}}),
        json!({"name":"session_event_append","description":"Append a normalized provider event to a session","inputSchema":{"type":"object","properties":{"sessionId":{"type":"string","format":"uuid"},"eventId":{"type":"string"},"idempotencyKey":{"type":"string"},"provider":{"type":"string","enum":["claude","codex","gemini"]},"eventType":{"type":"string","enum":["prompt","response","tool_call","tool_result","file_change","command","test_result","summary","error","annotation","reasoning","turn_context","agent_message"]},"eventTime":{"type":"string","format":"date-time"},"payload":{"type":"object"},"rawPayload":{"type":"object"},"turnId":{"type":"string","description":"Optional turn-graph identifier clustering events from one model step."}},"required":["sessionId","eventId","idempotencyKey","provider","eventType","eventTime","payload","rawPayload"]}}),
        json!({"name":"session_end","description":"End a session and derive searchable memories with provenance","inputSchema":{"type":"object","properties":{"sessionId":{"type":"string","format":"uuid"},"summary":{"type":"string"},"metadata":{"type":"object"}},"required":["sessionId"]}}),
        json!({"name":"repository_sync","description":"Incremental repository sync — accepts pre-read file contents and removed paths, parses in-memory, merges into existing graph snapshot. Preferred over project_import; the plugin hook invokes this automatically.","inputSchema":{"type":"object","properties":{"projectId":{"type":"string","format":"uuid"},"files":{"type":"array","items":{"type":"object","properties":{"path":{"type":"string"},"hash":{"type":"string"},"content":{"type":"string"}},"required":["path","hash","content"]}},"removedPaths":{"type":"array","items":{"type":"string"}},"manifest":{"type":"object","additionalProperties":{"type":"string"}},"mergeWithExisting":{"type":"boolean"}},"required":["files"]}}),
        json!({"name":"health_check","description":"Verify that PostgreSQL and optional Chroma dependencies are reachable","inputSchema":{"type":"object","properties":{}}}),
        json!({"name":"mem_search","description":"Natural language memory retrieval with progressive disclosure","inputSchema":{"type":"object","properties":{"query":{"type":"string"},"projectId":{"type":"string","format":"uuid"},"sessionId":{"type":"string","format":"uuid"},"provider":{"type":"string","enum":["claude","codex","gemini"]},"repoUrl":{"type":"string"},"branch":{"type":"string"},"types":{"type":"array","items":{"type":"string"}},"tags":{"type":"array","items":{"type":"string"}},"from":{"type":"string","format":"date-time"},"to":{"type":"string","format":"date-time"},"mode":{"type":"string","enum":["lexical","semantic","hybrid"]},"disclosureLevel":{"type":"string","enum":["overview","related","full"]},"includeHistorical":{"type":"boolean"},"limit":{"type":"integer","minimum":1,"maximum":50},"cursor":{"type":"string"}},"required":["query"]}}),
        json!({"name":"context_build","description":"Build a compact context pack from hybrid retrieval results","inputSchema":{"type":"object","properties":{"provider":{"type":"string","enum":["claude","codex","gemini"]},"objective":{"type":"string"},"retrievalIntent":{"type":"string","enum":["none","memory_only","repository_only","session_graph_only","hybrid"]},"projectId":{"type":"string","format":"uuid"},"repoUrl":{"type":"string"},"branch":{"type":"string"},"filePaths":{"type":"array","items":{"type":"string"}},"includeHistorical":{"type":"boolean"},"maxTokenBudget":{"type":"integer","minimum":1,"maximum":64000}},"required":["provider","objective","maxTokenBudget"]}}),
        json!({"name":"context_compile_v2","description":"Compile the smallest proof set whose claims cover the objective's sub-goals. v2.2.1 replacement for context_build: hard-ceiling token budget, surfaces uncovered sub-goals as proof_gap markers in unknowns instead of silently truncating. See docs/research/v2.2.1-pckc/DESIGN.md §4.","inputSchema":{"type":"object","properties":{"provider":{"type":"string","enum":["claude","codex","gemini"]},"objective":{"type":"string"},"retrievalIntent":{"type":"string","enum":["none","memory_only","repository_only","session_graph_only","hybrid"]},"projectId":{"type":"string","format":"uuid"},"repoUrl":{"type":"string"},"branch":{"type":"string"},"filePaths":{"type":"array","items":{"type":"string"}},"includeHistorical":{"type":"boolean"},"maxTokenBudget":{"type":"integer","minimum":1,"maximum":64000}},"required":["provider","objective","maxTokenBudget"]}}),
        json!({"name":"graph_snapshot","description":"Return a knowledge graph snapshot of memory relationships","inputSchema":{"type":"object","properties":{}}}),
        json!({"name":"knowledge_graph_export","description":"Export the latest knowledge graph as machine-readable JSON","inputSchema":{"type":"object","properties":{"projectId":{"type":"string","format":"uuid"},"layer":{"type":"string","enum":["repository","session"],"description":"Graph layer: repository (code structure) or session (interaction history). Omit for latest of any type."}}}}),
        json!({"name":"knowledge_report","description":"Generate a human-readable knowledge report from the latest graph snapshot","inputSchema":{"type":"object","properties":{"projectId":{"type":"string","format":"uuid"},"layer":{"type":"string","enum":["repository","session"],"description":"Graph layer: repository (code structure) or session (interaction history). Omit for latest of any type."}}}}),
        json!({"name":"knowledge_query","description":"Query hub nodes, shortest paths, neighbors, communities, search the graph, or run NeuroPath goal-directed path pruning","inputSchema":{"type":"object","properties":{"projectId":{"type":"string","format":"uuid"},"query":{"type":"string","enum":["hub_nodes","shortest_path","neighbors","communities","search","goal_directed"]},"nodeId":{"type":"string"},"targetNodeId":{"type":"string"},"text":{"type":"string"},"depth":{"type":"integer","minimum":1,"maximum":5},"layer":{"type":"string","enum":["repository","session"],"description":"Graph layer: repository (code structure) or session (interaction history). Omit for latest of any type."}},"required":["query"]}}),
        json!({"name":"knowledge_communities","description":"List detected graph communities and cohesion scores","inputSchema":{"type":"object","properties":{"projectId":{"type":"string","format":"uuid"},"layer":{"type":"string","enum":["repository","session"],"description":"Graph layer to query."}}}}),
        json!({"name":"memory_get_batch","description":"Fetch multiple memory records by ID after mem_search filtering","inputSchema":{"type":"object","properties":{"ids":{"type":"array","items":{"type":"string","format":"uuid"},"minItems":1,"maxItems":20}},"required":["ids"]}}),
        json!({"name":"memory_get","description":"Fetch a single memory and its related links","inputSchema":{"type":"object","properties":{"id":{"type":"string","format":"uuid"}},"required":["id"]}}),
        json!({"name":"claim_govern","description":"Transition a claim's governance state (pin, archive, reject, or reactivate)","inputSchema":{"type":"object","properties":{"claimId":{"type":"string","format":"uuid"},"newState":{"type":"string","enum":["active","pinned","archived","rejected"]},"reason":{"type":"string"}},"required":["claimId","newState"]}}),
    ]
}

fn jsonrpc_ok(id: &Value, result: Value) -> Value {
    json!({"jsonrpc":"2.0","id":id,"result":result})
}

fn jsonrpc_error(id: &Value, code: i32, message: &str) -> Value {
    json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}})
}

async fn handle_mcp_call(state: &ApiState, method: &str, params: &Value) -> Result<Value, String> {
    let args = if let Some(a) = params.get("arguments") {
        a
    } else {
        params
    };
    let tool_name = params
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or(method);

    match tool_name {
        "health_check" => {
            let response = build_health_response(
                state.metadata,
                state.config.as_ref(),
                state.started_at,
                API_ROUTES,
            );
            Ok(
                json!({"content":[{"type":"text","text":serde_json::to_string(&response).unwrap_or_default()}]}),
            )
        }
        "session_start" => {
            let input: StartSessionRequest =
                serde_json::from_value(args.clone()).map_err(|e| e.to_string())?;
            let result = perform_session_start(state, input)
                .await
                .map_err(|e| format!("{e:?}"))?;
            Ok(
                json!({"content":[{"type":"text","text":serde_json::to_string(&result).unwrap_or_default()}],"structuredContent":result}),
            )
        }
        "session_event_append" => {
            let input: AppendSessionEventRequest =
                serde_json::from_value(args.clone()).map_err(|e| e.to_string())?;
            let result = perform_session_event(state, input)
                .await
                .map_err(|e| format!("{e:?}"))?;
            Ok(
                json!({"content":[{"type":"text","text":serde_json::to_string(&result).unwrap_or_default()}],"structuredContent":result}),
            )
        }
        "session_end" => {
            let input: EndSessionRequest =
                serde_json::from_value(args.clone()).map_err(|e| e.to_string())?;
            let result = perform_session_end(state, input)
                .await
                .map_err(|e| format!("{e:?}"))?;
            Ok(
                json!({"content":[{"type":"text","text":serde_json::to_string(&result).unwrap_or_default()}],"structuredContent":result}),
            )
        }
        "mem_search" => {
            let input: MemorySearchRequest =
                serde_json::from_value(args.clone()).map_err(|e| e.to_string())?;
            let result = perform_search(state, input)
                .await
                .map_err(|e| format!("{e:?}"))?;
            Ok(
                json!({"content":[{"type":"text","text":serde_json::to_string(&result).unwrap_or_default()}],"structuredContent":result}),
            )
        }
        "context_build" => {
            let input: ContextBuildRequest =
                serde_json::from_value(args.clone()).map_err(|e| e.to_string())?;
            let result = perform_context_build(state, input)
                .await
                .map_err(|e| format!("{e:?}"))?;
            Ok(
                json!({"content":[{"type":"text","text":serde_json::to_string(&result).unwrap_or_default()}],"structuredContent":result}),
            )
        }
        "context_compile_v2" => {
            let input: ContextBuildRequest =
                serde_json::from_value(args.clone()).map_err(|e| e.to_string())?;
            let result = perform_context_compile_v2(state, input)
                .await
                .map_err(|e| format!("{e:?}"))?;
            Ok(
                json!({"content":[{"type":"text","text":serde_json::to_string(&result).unwrap_or_default()}],"structuredContent":result}),
            )
        }
        "graph_snapshot" => {
            let result = perform_dashboard_graph(state, None, None)
                .await
                .map_err(|e| format!("{e:?}"))?;
            Ok(
                json!({"content":[{"type":"text","text":serde_json::to_string(&result).unwrap_or_default()}],"structuredContent":result}),
            )
        }
        "knowledge_graph_export" => {
            let project_id = args
                .get("projectId")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse::<Uuid>().ok());
            let layer = args.get("layer").and_then(|v| v.as_str()).map(String::from);
            let result = perform_knowledge_export(state, project_id, layer.as_deref())
                .await
                .map_err(|e| format!("{e:?}"))?;
            Ok(
                json!({"content":[{"type":"text","text":result.node_link_json}],"structuredContent":result}),
            )
        }
        "knowledge_report" => {
            let project_id = args
                .get("projectId")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse::<Uuid>().ok());
            let layer = args.get("layer").and_then(|v| v.as_str()).map(String::from);
            let result = perform_knowledge_report(state, project_id, layer.as_deref())
                .await
                .map_err(|e| format!("{e:?}"))?;
            Ok(
                json!({"content":[{"type":"text","text":result.report_markdown}],"structuredContent":result}),
            )
        }
        "knowledge_query" => {
            let input: KnowledgeQueryRequest =
                serde_json::from_value(args.clone()).map_err(|e| e.to_string())?;
            let layer = args.get("layer").and_then(|v| v.as_str()).map(String::from);
            let result = perform_knowledge_query(state, input, layer.as_deref())
                .await
                .map_err(|e| format!("{e:?}"))?;
            Ok(
                json!({"content":[{"type":"text","text":serde_json::to_string(&result).unwrap_or_default()}],"structuredContent":result}),
            )
        }
        "knowledge_communities" => {
            let project_id = args
                .get("projectId")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse::<Uuid>().ok());
            let layer = args.get("layer").and_then(|v| v.as_str()).map(String::from);
            let result = perform_knowledge_communities(state, project_id, layer.as_deref())
                .await
                .map_err(|e| format!("{e:?}"))?;
            Ok(
                json!({"content":[{"type":"text","text":serde_json::to_string(&result).unwrap_or_default()}],"structuredContent":result}),
            )
        }
        "memory_get" => {
            let id_str = args
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or("missing id")?;
            let memory_id: Uuid = id_str.parse().map_err(|_| "invalid uuid")?;
            let result = perform_memory_get(state, memory_id)
                .await
                .map_err(|e| format!("{e:?}"))?;
            Ok(
                json!({"content":[{"type":"text","text":serde_json::to_string(&result).unwrap_or_default()}],"structuredContent":result}),
            )
        }
        "memory_get_batch" => {
            let input: MemoryBatchRequest =
                serde_json::from_value(args.clone()).map_err(|e| e.to_string())?;
            let result = perform_memory_batch(state, input.ids)
                .await
                .map_err(|e| format!("{e:?}"))?;
            Ok(
                json!({"content":[{"type":"text","text":serde_json::to_string(&result).unwrap_or_default()}],"structuredContent":{"memories":result}}),
            )
        }
        "repository_sync" => {
            let input: RepositorySyncRequest =
                serde_json::from_value(args.clone()).map_err(|e| e.to_string())?;
            let result = perform_repository_sync(state, input)
                .await
                .map_err(|e| format!("{e:?}"))?;
            Ok(json!({"content":[{"type":"text","text":"SUCCESSFUL"}],"structuredContent":result}))
        }
        "claim_govern" => {
            let claim_id: Uuid = args
                .get("claimId")
                .and_then(|v| v.as_str())
                .ok_or("claimId is required")?
                .parse()
                .map_err(|_| "invalid claimId UUID")?;
            let input: GovernClaimRequest =
                serde_json::from_value(json!({"newState": args.get("newState"), "reason": args.get("reason")}))
                    .map_err(|e| e.to_string())?;
            let result = perform_claim_govern(state, claim_id, input)
                .await
                .map_err(|e| format!("{e:?}"))?;
            Ok(json!({"content":[{"type":"text","text":format!("Claim {} transitioned from {} to {}", result.claim_id, serde_json::to_value(&result.previous_state).unwrap_or_default(), serde_json::to_value(&result.new_state).unwrap_or_default())}],"structuredContent":result}))
        }
        _ => Err(format!("unknown tool: {tool_name}")),
    }
}

async fn mcp_post(State(state): State<ApiState>, request: Request) -> Result<Response, ApiError> {
    let session_id_header = request
        .headers()
        .get("mcp-session-id")
        .and_then(|v| v.to_str().ok())
        .map(String::from);

    let body_bytes = axum::body::to_bytes(request.into_body(), 8 * 1024 * 1024)
        .await
        .map_err(|e| ApiError::bad_request(e.to_string()))?;
    let body: Value = serde_json::from_slice(&body_bytes)
        .map_err(|e| ApiError::bad_request(format!("invalid JSON: {e}")))?;

    let method = body.get("method").and_then(|v| v.as_str()).unwrap_or("");
    let id = body.get("id").cloned().unwrap_or(Value::Null);
    let params = body.get("params").cloned().unwrap_or(json!({}));

    match method {
        "initialize" => {
            // Validate existing session or create new one
            let new_session_id = Uuid::new_v4().to_string();
            state
                .mcp_sessions
                .write()
                .await
                .insert(new_session_id.clone());

            let result = json!({
                "protocolVersion": MCP_PROTOCOL_VERSION,
                "capabilities": {
                    "tools": { "listChanged": false }
                },
                "serverInfo": {
                    "name": "chum-mem",
                    "version": env!("CARGO_PKG_VERSION")
                }
            });

            let response_body = serde_json::to_string(&jsonrpc_ok(&id, result))
                .map_err(|e| ApiError::internal(e.to_string()))?;

            Ok(Response::builder()
                .status(200)
                .header("Content-Type", "application/json")
                .header("Mcp-Session-Id", &new_session_id)
                .body(axum::body::Body::from(response_body))
                .unwrap())
        }

        "notifications/initialized" => {
            // Acknowledgement notification — no response needed per spec
            Ok(Response::builder()
                .status(204)
                .body(axum::body::Body::empty())
                .unwrap())
        }

        "tools/list" => {
            // Verify session
            if let Some(ref sid) = session_id_header {
                if !state.mcp_sessions.read().await.contains(sid) {
                    return Ok(Response::builder()
                        .status(400)
                        .header("Content-Type", "application/json")
                        .body(axum::body::Body::from(r#"{"error":"Invalid session ID"}"#))
                        .unwrap());
                }
            }

            let result = json!({ "tools": mcp_tool_definitions() });
            let response_body = serde_json::to_string(&jsonrpc_ok(&id, result))
                .map_err(|e| ApiError::internal(e.to_string()))?;

            let mut builder = Response::builder()
                .status(200)
                .header("Content-Type", "application/json");
            if let Some(ref sid) = session_id_header {
                builder = builder.header("Mcp-Session-Id", sid.as_str());
            }
            Ok(builder.body(axum::body::Body::from(response_body)).unwrap())
        }

        "tools/call" => {
            // Verify session
            if let Some(ref sid) = session_id_header {
                if !state.mcp_sessions.read().await.contains(sid) {
                    return Ok(Response::builder()
                        .status(400)
                        .header("Content-Type", "application/json")
                        .body(axum::body::Body::from(r#"{"error":"Invalid session ID"}"#))
                        .unwrap());
                }
            }

            let result = match handle_mcp_call(&state, method, &params).await {
                Ok(content) => jsonrpc_ok(&id, content),
                Err(err) => jsonrpc_error(&id, -32000, &err),
            };

            let response_body =
                serde_json::to_string(&result).map_err(|e| ApiError::internal(e.to_string()))?;

            let mut builder = Response::builder()
                .status(200)
                .header("Content-Type", "application/json");
            if let Some(ref sid) = session_id_header {
                builder = builder.header("Mcp-Session-Id", sid.as_str());
            }
            Ok(builder.body(axum::body::Body::from(response_body)).unwrap())
        }

        "ping" => {
            let response_body = serde_json::to_string(&jsonrpc_ok(&id, json!({})))
                .map_err(|e| ApiError::internal(e.to_string()))?;
            Ok(Response::builder()
                .status(200)
                .header("Content-Type", "application/json")
                .body(axum::body::Body::from(response_body))
                .unwrap())
        }

        _ => {
            let response_body = serde_json::to_string(&jsonrpc_error(
                &id,
                -32601,
                &format!("Method not found: {method}"),
            ))
            .map_err(|e| ApiError::internal(e.to_string()))?;
            Ok(Response::builder()
                .status(200)
                .header("Content-Type", "application/json")
                .body(axum::body::Body::from(response_body))
                .unwrap())
        }
    }
}

async fn mcp_get(State(state): State<ApiState>, request: Request) -> Result<Response, ApiError> {
    let session_id = request
        .headers()
        .get("mcp-session-id")
        .and_then(|v| v.to_str().ok());

    match session_id {
        Some(sid) if state.mcp_sessions.read().await.contains(sid) => {
            // SSE endpoint — keep-alive with no events (stateless server)
            Ok(Response::builder()
                .status(200)
                .header("Content-Type", "text/event-stream")
                .header("Cache-Control", "no-cache")
                .header("Mcp-Session-Id", sid)
                .body(axum::body::Body::from("event: endpoint\ndata: {}\n\n"))
                .unwrap())
        }
        _ => Ok(Response::builder()
            .status(400)
            .header("Content-Type", "application/json")
            .body(axum::body::Body::from(
                r#"{"error":"Invalid or missing session ID"}"#,
            ))
            .unwrap()),
    }
}

async fn mcp_delete(State(state): State<ApiState>, request: Request) -> Result<Response, ApiError> {
    let session_id = request
        .headers()
        .get("mcp-session-id")
        .and_then(|v| v.to_str().ok())
        .map(String::from);

    match session_id {
        Some(sid) if state.mcp_sessions.write().await.remove(&sid) => Ok(Response::builder()
            .status(204)
            .body(axum::body::Body::empty())
            .unwrap()),
        _ => Ok(Response::builder()
            .status(400)
            .header("Content-Type", "application/json")
            .body(axum::body::Body::from(
                r#"{"error":"Invalid or missing session ID"}"#,
            ))
            .unwrap()),
    }
}

fn router(state: ApiState) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([Method::GET, Method::POST, Method::DELETE])
        .allow_headers(Any)
        .expose_headers([
            ACCEPT,
            HeaderName::from_static("mcp-session-id"),
            HeaderName::from_static("content-type"),
        ]);

    Router::new()
        .route("/health", get(health))
        .route("/ready", get(ready))
        .route("/api/search", post(search))
        .route("/api/dashboard/summary", get(dashboard_summary))
        .route("/api/dashboard/graph", get(dashboard_graph))
        .route("/api/knowledge/export", get(knowledge_export))
        .route("/api/knowledge/report", get(knowledge_report))
        .route("/api/knowledge/query", post(knowledge_query))
        .route("/api/knowledge/communities", get(knowledge_communities))
        .route("/api/knowledge/sync-rules", get(sync_rules))
        .route("/api/knowledge/repository-sync", post(repository_sync))
        .route("/api/memory/{id}", get(memory_get))
        .route("/api/memory/batch", post(memory_batch))
        .route("/api/context/build", post(context_build))
        .route("/api/claims/{id}/govern", post(claim_govern))
        .route("/v1/projects/resolve", post(project_resolve))
        .route("/v1/ingest/session/start", post(session_start))
        .route("/v1/ingest/session/event", post(session_event))
        .route("/v1/ingest/session/events", post(session_events_batch))
        .route("/v1/ingest/session/events/bulk", post(session_events_bulk))
        .route("/v1/ingest/bulk/drop-indexes", post(bulk_drop_indexes))
        .route("/v1/ingest/bulk/create-indexes", post(bulk_create_indexes))
        .route("/v1/ingest/session/end", post(session_end))
        .route("/mcp", post(mcp_post).get(mcp_get).delete(mcp_delete))
        .layer(DefaultBodyLimit::max(32 * 1024 * 1024))
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing("chum_mem_api");

    let config = Arc::new(AppConfig::from_env().context("loading API configuration")?);
    let db = Database::connect(config.as_ref())
        .await
        .context("connecting API database pool")?;
    db.migrate_if_enabled(config.as_ref())
        .await
        .context("running API startup migrations")?;

    let metadata = ServiceMetadata {
        name: "chum-mem-api",
        version: env!("CARGO_PKG_VERSION"),
        role: "api",
    };
    let state = ApiState {
        config: Arc::clone(&config),
        db,
        scope: RepositoryContext::from_config(config.as_ref()),
        metadata,
        started_at: OffsetDateTime::now_utc(),
        http_client: Client::builder()
            .build()
            .context("building shared HTTP client")?,
        mcp_sessions: Arc::new(RwLock::new(HashSet::new())),
        community_cache: Arc::new(RwLock::new(CommunityCache::default())),
    };

    let address = config
        .bind_address(ServiceKind::Api)
        .context("building API bind address")?;
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .with_context(|| format!("binding API listener on {address}"))?;

    info!(address = %address, "starting Rust API");

    axum::serve(listener, router(state))
        .with_graceful_shutdown(shutdown_signal("chum-mem-api"))
        .await
        .context("running API server")?;

    Ok(())
}

// ── Project resolve endpoint ────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectResolveRequest {
    repo_url: Option<String>,
    name: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProjectResolveResponse {
    project_id: Uuid,
    name: String,
    created: bool,
}

async fn project_resolve(
    State(state): State<ApiState>,
    Json(input): Json<ProjectResolveRequest>,
) -> Result<Json<ProjectResolveResponse>, ApiError> {
    let response = perform_project_resolve(&state, input)
        .await
        .map_err(map_domain_error)?;
    Ok(Json(response))
}

async fn perform_project_resolve(
    state: &ApiState,
    input: ProjectResolveRequest,
) -> Result<ProjectResolveResponse, DomainError> {
    let name = input.name.unwrap_or_else(|| {
        input
            .repo_url
            .as_deref()
            .and_then(|u| u.rsplit('/').next())
            .map(|s| s.trim_end_matches(".git").to_string())
            .unwrap_or_else(|| "unnamed-project".to_string())
    });

    let mut tx = begin_tx(state, &state.scope).await?;
    ensure_scope_entities(&mut tx, &state.scope).await?;

    // Try to find existing project by repo_url first, then by name.
    // Exclude the global fallback project (slug = 'global') so it never
    // hijacks per-project resolution.
    let existing = if let Some(ref url) = input.repo_url {
        let by_url = sqlx::query(
            r#"
            select id, name from public.projects
            where organization_id = $1 and team_id = $2 and repo_url = $3
              and slug != 'global'
            limit 1
            "#,
        )
        .bind(state.scope.organization_id)
        .bind(state.scope.team_id)
        .bind(url)
        .fetch_optional(&mut *tx)
        .await
        .map_err(DbError::from)?;
        if by_url.is_some() {
            by_url
        } else {
            sqlx::query(
                r#"
                select id, name from public.projects
                where organization_id = $1 and team_id = $2 and name = $3
                  and slug != 'global'
                limit 1
                "#,
            )
            .bind(state.scope.organization_id)
            .bind(state.scope.team_id)
            .bind(&name)
            .fetch_optional(&mut *tx)
            .await
            .map_err(DbError::from)?
        }
    } else {
        sqlx::query(
            r#"
            select id, name from public.projects
            where organization_id = $1 and team_id = $2 and name = $3
              and slug != 'global'
            limit 1
            "#,
        )
        .bind(state.scope.organization_id)
        .bind(state.scope.team_id)
        .bind(&name)
        .fetch_optional(&mut *tx)
        .await
        .map_err(DbError::from)?
    };

    if let Some(row) = existing {
        let project_id: Uuid = row.try_get("id").map_err(DbError::from)?;
        let project_name: String = row.try_get("name").map_err(DbError::from)?;
        // Backfill repo_url if the existing project doesn't have one
        if let Some(ref url) = input.repo_url {
            sqlx::query(
                "UPDATE public.projects SET repo_url = $1 WHERE id = $2 AND repo_url IS NULL",
            )
            .bind(url)
            .bind(project_id)
            .execute(&mut *tx)
            .await
            .map_err(DbError::from)?;
        }
        commit_tx(tx).await?;
        return Ok(ProjectResolveResponse {
            project_id,
            name: project_name,
            created: false,
        });
    }

    let project_id = Uuid::new_v4();
    let slug = format!("project-{}", &project_id.simple().to_string()[..12]);
    sqlx::query(
        r#"
        insert into public.projects (id, organization_id, team_id, name, slug, repo_url)
        values ($1, $2, $3, $4, $5, $6)
        "#,
    )
    .bind(project_id)
    .bind(state.scope.organization_id)
    .bind(state.scope.team_id)
    .bind(&name)
    .bind(&slug)
    .bind(input.repo_url.as_deref())
    .execute(&mut *tx)
    .await
    .map_err(DbError::from)?;

    commit_tx(tx).await?;
    Ok(ProjectResolveResponse {
        project_id,
        name,
        created: true,
    })
}

async fn perform_session_start(
    state: &ApiState,
    input: StartSessionRequest,
) -> Result<StartSessionResponse, DomainError> {
    if let Some(scoped_project) = state.scope.project_id
        && scoped_project != input.project_id
    {
        return Err(DomainError::BadRequest(format!(
            "Project {} is out of scope for this server configuration",
            input.project_id
        )));
    }

    let mut tx = begin_tx(state, &state.scope).await?;
    ensure_scope_entities(&mut tx, &state.scope).await?;
    upsert_ingested_project(
        &mut tx,
        &state.scope,
        input.project_id,
        input.repo.repo_url.as_ref().map(|value| value.as_str()),
        input.repo.branch.as_deref(),
    )
    .await?;
    let session = upsert_session(&mut tx, &state.scope, &input).await?;
    insert_audit_log(
        &mut tx,
        &state.scope,
        "session.started",
        "session",
        session.id,
        json!({
            "externalSessionId": input.external_session_id,
            "provider": provider_str(input.provider),
        }),
    )
    .await?;
    commit_tx(tx).await?;

    Ok(StartSessionResponse {
        session_id: session.id,
        organization_id: session.organization_id,
        team_id: session.team_id,
        project_id: session.project_id,
        status: session.status,
    })
}

async fn perform_session_event(
    state: &ApiState,
    input: AppendSessionEventRequest,
) -> Result<AppendSessionEventResponse, DomainError> {
    let mut tx = begin_tx(state, &state.scope).await?;
    let session = resolve_session(&mut tx, &state.scope, input.session_id).await?;
    if session.status != "active" {
        return Err(DomainError::BadRequest(format!(
            "Cannot append events to non-active session {}",
            session.id
        )));
    }

    let inserted = insert_session_event(
        &mut tx,
        &state.scope,
        &AppendSessionEventParams {
            session_id: session.id,
            project_id: session.project_id,
            provider: input.provider,
            event_type: canonical_event_type_str(input.event_type).to_string(),
            event_time: input.event_time.clone(),
            event_id: input.event_id.clone(),
            idempotency_key: input.idempotency_key.clone(),
            payload: sanitize_json_value(
                serde_json::to_value(input.payload).expect("payload serializes"),
            ),
            raw_payload: sanitize_json_value(input.raw_payload),
            turn_id: input.turn_id.clone(),
        },
    )
    .await?;

    insert_audit_log(
        &mut tx,
        &state.scope,
        "session.event_ingested",
        "session_event",
        inserted.event_id,
        json!({
            "duplicate": inserted.duplicate,
            "provider": provider_str(input.provider),
            "eventType": canonical_event_type_str(input.event_type),
        }),
    )
    .await?;
    commit_tx(tx).await?;

    Ok(AppendSessionEventResponse {
        event_id: inserted.event_id,
        duplicate: inserted.duplicate,
    })
}

async fn perform_session_events_batch(
    state: &ApiState,
    input: BatchAppendSessionEventsRequest,
) -> Result<BatchAppendSessionEventsResponse, DomainError> {
    let mut tx = begin_tx(state, &state.scope).await?;
    let session = resolve_session(&mut tx, &state.scope, input.session_id).await?;
    if session.status != "active" {
        return Err(DomainError::BadRequest(format!(
            "Cannot append events to non-active session {}",
            session.id
        )));
    }

    // One multi-row INSERT per batch instead of one round trip per event.
    // Collapses the primary ingestion hot path from O(N) round trips to O(1).
    let batch_params: Vec<AppendSessionEventParams> = input
        .events
        .into_iter()
        .map(|event| AppendSessionEventParams {
            session_id: session.id,
            project_id: session.project_id,
            provider: event.provider,
            event_type: canonical_event_type_str(event.event_type).to_string(),
            event_time: event.event_time,
            event_id: event.event_id,
            idempotency_key: event.idempotency_key,
            payload: sanitize_json_value(
                serde_json::to_value(event.payload).expect("payload serializes"),
            ),
            raw_payload: sanitize_json_value(event.raw_payload),
            turn_id: event.turn_id,
        })
        .collect();

    let inserted_rows = insert_session_events_batch(&mut tx, &state.scope, &batch_params).await?;

    let mut inserted_count = 0_i32;
    let mut duplicate_count = 0_i32;
    for row in &inserted_rows {
        if row.duplicate {
            duplicate_count += 1;
        } else {
            inserted_count += 1;
        }
    }
    commit_tx(tx).await?;

    Ok(BatchAppendSessionEventsResponse {
        inserted: inserted_count,
        duplicates: duplicate_count,
    })
}

/// High-throughput bulk insert using COPY FROM STDIN through an UNLOGGED
/// staging table with deferred constraint checking.
async fn perform_session_events_bulk(
    state: &ApiState,
    input: BatchAppendSessionEventsRequest,
) -> Result<BatchAppendSessionEventsResponse, DomainError> {
    // Validate session exists and is active via a short-lived transaction.
    let mut tx = begin_tx(state, &state.scope).await?;
    let session = resolve_session(&mut tx, &state.scope, input.session_id).await?;
    if session.status != "active" {
        return Err(DomainError::BadRequest(format!(
            "Cannot append events to non-active session {}",
            session.id
        )));
    }
    commit_tx(tx).await?;

    let batch_params: Vec<AppendSessionEventParams> = input
        .events
        .into_iter()
        .map(|event| AppendSessionEventParams {
            session_id: session.id,
            project_id: session.project_id,
            provider: event.provider,
            event_type: canonical_event_type_str(event.event_type).to_string(),
            event_time: event.event_time,
            event_id: event.event_id,
            idempotency_key: event.idempotency_key,
            payload: sanitize_json_value(
                serde_json::to_value(event.payload).expect("payload serializes"),
            ),
            raw_payload: sanitize_json_value(event.raw_payload),
            turn_id: event.turn_id,
        })
        .collect();

    let (inserted, duplicates) =
        bulk_insert_session_events_copy(state.db.pool(), &state.scope, batch_params)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

    Ok(BatchAppendSessionEventsResponse {
        inserted: inserted as i32,
        duplicates: duplicates as i32,
    })
}

async fn perform_session_end(
    state: &ApiState,
    input: EndSessionRequest,
) -> Result<EndSessionResponse, DomainError> {
    let mut tx = begin_tx(state, &state.scope).await?;
    let session = resolve_session(&mut tx, &state.scope, input.session_id).await?;
    let session_update = mark_session_completed(
        &mut tx,
        session.id,
        &json!({
            "sessionSummary": input.summary,
            "metadata": input.metadata,
        }),
    )
    .await?;

    let defer = input.defer.unwrap_or(false);

    let (derived_info, queued_jobs) = if defer {
        // Deferred mode: skip inline derivation, enqueue a worker job.
        // Used by bulk import for fast throughput.
        let queued = enqueue_worker_job(
            &mut tx,
            &state.scope,
            session.project_id,
            Some(session.id),
            None,
            "derive-session-memories",
            &format!("derive:{}", session.id),
            50,
            3,
            None,
            &json!({
                "sessionId": session.id,
                "summary": input.summary,
                "metadata": input.metadata,
            }),
        )
        .await?;
        (json!({ "deferred": true }), vec![queued.job_type])
    } else {
        // Inline mode: derive memories synchronously (normal runtime path).
        let derived = derive_and_persist_session_memories(
            &mut tx,
            &state.scope,
            session.project_id,
            &session.provider,
            session.id,
            session.repo_url.as_deref(),
            session.branch.as_deref(),
            &input,
        )
        .await?;

        let job_plan = build_session_completion_job_plan(
            session.id,
            derived.unresolved_risk,
            state.config.chroma_enabled(),
            true,
        );
        let mut jobs = Vec::new();
        for job in job_plan {
            let queued = enqueue_worker_job(
                &mut tx,
                &state.scope,
                session.project_id,
                Some(session.id),
                None,
                &job.job_type,
                &job.dedupe_key,
                job.priority,
                3,
                None,
                &job.payload,
            )
            .await?;
            if job.job_type == "replay-failed-session" {
                create_session_replay(
                    &mut tx,
                    &state.scope,
                    session.project_id,
                    session.id,
                    queued.id,
                    "session ended with unresolved debugging risk",
                    &json!({ "sessionId": session.id }),
                )
                .await?;
            }
            jobs.push(queued.job_type);
        }

        (json!({
            "derivedMemories": derived.derived_memories,
            "derivedEpisodes": derived.derived_episodes,
            "derivedSessionEdges": derived.derived_session_edges,
            "unresolvedRisk": derived.unresolved_risk,
        }), jobs)
    };

    insert_audit_log(
        &mut tx,
        &state.scope,
        "session.ended",
        "session",
        session.id,
        json!({
            "status": session_update.status,
            "queuedJobs": queued_jobs,
            "derived": derived_info,
        }),
    )
    .await?;
    commit_tx(tx).await?;

    Ok(EndSessionResponse {
        session_id: session.id,
        status: "completed".to_string(),
        queued_jobs,
    })
}

async fn perform_search(
    state: &ApiState,
    input: MemorySearchRequest,
) -> Result<MemorySearchEnvelope, DomainError> {
    let started = Instant::now();
    let mut tx = begin_tx(state, &state.scope).await?;
    let lexical_rows = load_memory_search_rows(
        &mut tx,
        &state.scope,
        &input.query,
        input.project_id,
        input.session_id,
        input.provider.map(provider_str),
        input.repo_url.as_ref().map(|value| value.as_str()),
        input.branch.as_deref(),
        input.from.as_deref(),
        input.to.as_deref(),
        &input
            .types
            .iter()
            .map(|memory_type| memory_type_str(*memory_type).to_string())
            .collect::<Vec<_>>(),
        input.include_historical.unwrap_or(false),
        input.limit as i64,
    )
    .await?;
    let lexical_hits = lexical_rows
        .iter()
        .map(|row| map_ranked_memory(row, true))
        .collect::<Vec<_>>();
    let semantic_hits = if input.mode == chum_mem_contracts::SearchMode::Lexical {
        Vec::new()
    } else {
        semantic_search(&mut tx, &state.scope, &input).await?
    };
    let mut ranking_context = RankingContext::default();
    ranking_context.session_id = input.session_id;
    ranking_context.repo_url = input.repo_url.as_ref().map(|value| value.to_string());
    ranking_context.branch = input.branch.clone();
    ranking_context.retrieval_intent = input.retrieval_intent.unwrap_or_default();
    ranking_context.query_text = Some(input.query.clone());
    // v2.2.3: Continuation retrieval mode
    ranking_context.is_continuation = is_continuation_query(&input.query);
    // v2.2.2: Pass requested types for type-fit scoring
    ranking_context.requested_types = input
        .types
        .iter()
        .map(|t| memory_type_str(*t).to_string())
        .collect();
    // v2.2.2: Community-aware retrieval — best-effort. Use cached community
    // maps instead of loading the full session snapshot on every query.
    // Cache TTL 5 minutes, scoped by project_id.
    {
        const CACHE_TTL_SECS: u64 = 300;
        let needs_refresh = {
            let cache = state.community_cache.read().await;
            let project_changed = cache.project_id != input.project_id;
            cache.loaded_at.map_or(true, |t| t.elapsed().as_secs() > CACHE_TTL_SECS) || project_changed
        };
        if needs_refresh {
            if let Ok(scope) = scoped_context(&state.scope, input.project_id)
                && let Ok(Some(graph)) =
                    load_latest_knowledge_graph_by_type(&mut tx, &scope, Some("session")).await
            {
                let relevance = community_relevance_from_query(&input.query, &graph);
                let mem_comm = memory_community_map(&graph);
                let mut cache = state.community_cache.write().await;
                cache.project_id = input.project_id;
                cache.community_relevance = relevance;
                cache.memory_community = mem_comm;
                cache.loaded_at = Some(Instant::now());
            }
        }
        let cache = state.community_cache.read().await;
        if !cache.community_relevance.is_empty() {
            ranking_context.community_relevance = cache.community_relevance.clone();
            ranking_context.memory_community = cache.memory_community.clone();
        }
    }
    if let Some(session_id) = input.session_id {
        let preliminary = merge_hybrid_results(&lexical_hits, &semantic_hits, &ranking_context);
        let candidate_session_ids = preliminary
            .iter()
            .flat_map(|hit| hit.session_ids.iter().copied())
            .filter(|candidate| *candidate != session_id)
            .collect::<HashSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let weights =
            load_session_graph_weights(&mut tx, &state.scope, session_id, &candidate_session_ids)
                .await?;
        ranking_context.session_graph_weights = weights.into_iter().collect();
    }

    // v2.2.2: Always query Chroma as a primary source (not fallback-only).
    // Chroma uses real ML embeddings (all-MiniLM-L6-v2) for semantic search,
    // complementing the pgvector hash-based ANN and PostgreSQL lexical search.
    let mut chroma_semantic: Vec<chum_mem_pipeline::SemanticQueryResult> = Vec::new();
    let mut chroma_ranked_hits: Vec<RankedMemory> = Vec::new();
    if input.mode != chum_mem_contracts::SearchMode::Lexical {
        if let Some(chroma_url) = state.config.chroma_url.as_ref().map(|value| value.as_str()) {
            let typed_filter: Vec<String> = input
                .types
                .iter()
                .map(|t| memory_type_str(*t).to_string())
                .collect();
            if let Ok(chroma) = query_chroma_memories_typed(
                &state.http_client,
                chroma_url,
                &state.config.chroma_collection,
                &input.query,
                &typed_filter,
                input.limit as usize,
            )
            .await
            {
                let existing_ids: HashSet<Uuid> = lexical_hits.iter().map(|h| h.id)
                    .chain(semantic_hits.iter().map(|h| h.id))
                    .collect();
                let chroma_only_ids: Vec<Uuid> = chroma
                    .iter()
                    .map(|hit| hit.id)
                    .filter(|id| !existing_ids.contains(id))
                    .collect();
                if !chroma_only_ids.is_empty() {
                    if let Ok(chroma_memories) =
                        load_memories_batch(&mut tx, &state.scope, &chroma_only_ids).await
                    {
                        chroma_ranked_hits = chroma_memories
                            .iter()
                            .map(map_memory_detail_to_ranked_memory)
                            .collect();
                    }
                }
                chroma_semantic = chroma
                    .iter()
                    .map(chroma_to_semantic_query_result)
                    .collect();
            }
        }
    }

    // Merge all three sources: lexical + pgvector semantic + Chroma ML semantic
    let total_semantic_count = semantic_hits.len() + chroma_semantic.len();
    let all_lexical = [lexical_hits, chroma_ranked_hits].concat();
    let all_semantic = [semantic_hits, chroma_semantic].concat();
    let merged = merge_hybrid_results(&all_lexical, &all_semantic, &ranking_context);

    let mut ranked = rank_hybrid_results(&merged, &ranking_context);

    // Type filtering: soft preference via type_fit_boost in ranking, hard filter
    // only when enough matching results exist to fill the limit.
    if !input.types.is_empty() {
        let typed: Vec<_> = ranked
            .iter()
            .filter(|hit| input.types.contains(&hit.memory_type))
            .cloned()
            .collect();
        if !typed.is_empty() {
            ranked = typed;
        }
    }

    let final_ids = ranked
        .iter()
        .take(input.limit as usize)
        .map(|hit| hit.id)
        .collect::<Vec<_>>();
    let provenance =
        load_memory_provenance(&mut tx, &final_ids, SEARCH_PROVENANCE_LIMIT_DEFAULT).await?;
    let claim_proofs = load_claim_proofs(&mut tx, &final_ids).await?;
    let provenance_map = map_provenance_rows(&provenance);
    let claim_proof_map = map_claim_proof_rows(&claim_proofs);
    ranked = ranked
        .into_iter()
        .map(|mut hit| {
            hit.provenance = provenance_map.get(&hit.id).cloned().unwrap_or_default();
            hit.proof_handles = if let Some(proofs) = claim_proof_map.get(&hit.id) {
                proofs.clone()
            } else if hit.proof_handles.is_empty() {
                build_proof_handles_from_ranked_memory(&hit)
            } else {
                hit.proof_handles.clone()
            };
            hit
        })
        .collect();
    commit_tx(tx).await?;

    // Global project fallback: if project-specific query returned no results
    // and we queried a specific project, retry against the "global" project.
    if ranked.is_empty() && input.project_id.is_some() {
        let mut fallback_tx = begin_tx(state, &state.scope).await?;
        if let Some(global_pid) =
            resolve_global_project_id(&mut fallback_tx, &state.scope).await
        {
            if input.project_id != Some(global_pid) {
                let mut fallback_input = input.clone();
                fallback_input.project_id = Some(global_pid);
                commit_tx(fallback_tx).await?;
                return Box::pin(perform_search(state, fallback_input)).await;
            }
        }
        commit_tx(fallback_tx).await?;
    }

    Ok(MemorySearchEnvelope {
        disclosure: progressive_disclosure(&ranked, input.disclosure_level),
        metrics: SearchMetrics {
            lexical_count: lexical_rows.len(),
            semantic_count: total_semantic_count,
            latency_ms: started.elapsed().as_millis(),
        },
        hits: ranked,
    })
}

async fn perform_context_build(
    state: &ApiState,
    input: ContextBuildRequest,
) -> Result<ContextBuildResponse, DomainError> {
    let retrieval_intent = input
        .retrieval_intent
        .unwrap_or_else(|| infer_retrieval_intent(&input));
    if retrieval_intent == RetrievalIntent::None {
        return Ok(build_context_pack(
            &[],
            input.max_token_budget,
            RetrievalIntent::None,
        ));
    }

    let context = scoped_context(&state.scope, input.project_id)?;
    let mut tx = begin_tx(state, &context).await?;
    let repository_graph = if matches!(
        retrieval_intent,
        RetrievalIntent::RepositoryOnly | RetrievalIntent::Hybrid
    ) {
        load_latest_knowledge_graph_by_type(&mut tx, &context, Some("repository")).await?
    } else {
        None
    };
    let session_graph = if matches!(
        retrieval_intent,
        RetrievalIntent::SessionGraphOnly | RetrievalIntent::Hybrid
    ) {
        load_latest_knowledge_graph_by_type(&mut tx, &context, Some("session")).await?
    } else {
        None
    };
    commit_tx(tx).await?;

    let memory_hits = if matches!(
        retrieval_intent,
        RetrievalIntent::MemoryOnly | RetrievalIntent::Hybrid
    ) {
        Some(perform_context_memory_searches(state, &input, retrieval_intent).await?)
    } else {
        None
    };

    let mut items = Vec::new();
    let mut conflict_items = Vec::new();

    if let Some(graph) = repository_graph.as_ref() {
        items.extend(build_repository_context_items(
            graph,
            &input.objective,
            &input.file_paths,
        ));
    }
    if let Some(hits) = memory_hits.as_ref() {
        let (memory_items, conflicts) = build_memory_context_items(hits);
        items.extend(memory_items);
        conflict_items.extend(conflicts);
    }
    if let Some(graph) = session_graph.as_ref() {
        items.extend(build_session_graph_context_items(graph, &input.objective));
    }
    items.extend(conflict_items);

    Ok(build_context_pack(
        &items,
        input.max_token_budget,
        retrieval_intent,
    ))
}

/// v2.2.1 compiler variant of `perform_context_build`.
///
/// Shares the retrieval path (hybrid memory search + repository + session
/// graph) but routes the final assembly through `compile_minimal_proof_set`
/// instead of the packer. Surfaces proof gaps in `unknowns` instead of
/// silently truncating. See `docs/research/v2.2.1-pckc/DESIGN.md` §4.
async fn perform_context_compile_v2(
    state: &ApiState,
    input: ContextBuildRequest,
) -> Result<ContextBuildResponse, DomainError> {
    let retrieval_intent = input
        .retrieval_intent
        .unwrap_or_else(|| infer_retrieval_intent(&input));
    if retrieval_intent == RetrievalIntent::None {
        return Ok(compile_minimal_proof_set(
            &input.objective,
            &[],
            input.max_token_budget,
            RetrievalIntent::None,
        ));
    }

    let context = scoped_context(&state.scope, input.project_id)?;
    let mut tx = begin_tx(state, &context).await?;
    let repository_graph = if matches!(
        retrieval_intent,
        RetrievalIntent::RepositoryOnly | RetrievalIntent::Hybrid
    ) {
        load_latest_knowledge_graph_by_type(&mut tx, &context, Some("repository")).await?
    } else {
        None
    };
    let session_graph = if matches!(
        retrieval_intent,
        RetrievalIntent::SessionGraphOnly | RetrievalIntent::Hybrid
    ) {
        load_latest_knowledge_graph_by_type(&mut tx, &context, Some("session")).await?
    } else {
        None
    };
    commit_tx(tx).await?;

    let memory_hits = if matches!(
        retrieval_intent,
        RetrievalIntent::MemoryOnly | RetrievalIntent::Hybrid
    ) {
        Some(perform_context_memory_searches(state, &input, retrieval_intent).await?)
    } else {
        None
    };

    let mut items = Vec::new();
    let mut conflict_items = Vec::new();

    if let Some(graph) = repository_graph.as_ref() {
        items.extend(build_repository_context_items(
            graph,
            &input.objective,
            &input.file_paths,
        ));
    }
    if let Some(hits) = memory_hits.as_ref() {
        let (memory_items, conflicts) = build_memory_context_items(hits);
        items.extend(memory_items);
        conflict_items.extend(conflicts);
    }
    if let Some(graph) = session_graph.as_ref() {
        items.extend(build_session_graph_context_items(graph, &input.objective));
    }
    items.extend(conflict_items);

    Ok(compile_minimal_proof_set(
        &input.objective,
        &items,
        input.max_token_budget,
        retrieval_intent,
    ))
}

async fn perform_context_memory_searches(
    state: &ApiState,
    input: &ContextBuildRequest,
    retrieval_intent: RetrievalIntent,
) -> Result<Vec<RankedMemory>, DomainError> {
    let objective = input.objective.clone();
    let type_scopes = context_memory_type_scopes(&objective);
    let mut merged = HashMap::<Uuid, RankedMemory>::new();

    let mut requests = Vec::with_capacity(type_scopes.len() + 1);
    requests.push(MemorySearchRequest {
        query: objective.clone(),
        project_id: input.project_id,
        session_id: None,
        provider: None,
        repo_url: input.repo_url.clone(),
        branch: input.branch.clone(),
        types: Vec::new(),
        tags: Vec::new(),
        from: None,
        to: None,
        mode: chum_mem_contracts::SearchMode::Hybrid,
        disclosure_level: chum_mem_contracts::DisclosureLevel::Overview,
        retrieval_intent: Some(retrieval_intent),
        include_historical: input.include_historical,
        limit: CONTEXT_BUILD_SEARCH_LIMIT,
        cursor: None,
    });

    for (scoped_types, scope_limit) in type_scopes {
        let scoped_query = context_memory_query_for_scope(&objective, &scoped_types);
        requests.push(MemorySearchRequest {
            query: scoped_query,
            project_id: input.project_id,
            session_id: None,
            provider: None,
            repo_url: input.repo_url.clone(),
            branch: input.branch.clone(),
            types: scoped_types,
            tags: Vec::new(),
            from: None,
            to: None,
            mode: chum_mem_contracts::SearchMode::Hybrid,
            disclosure_level: chum_mem_contracts::DisclosureLevel::Overview,
            retrieval_intent: Some(retrieval_intent),
            include_historical: input.include_historical,
            limit: scope_limit,
            cursor: None,
        });
    }

    for request in requests {
        for hit in perform_search(state, request).await?.hits {
            let keep = match merged.get(&hit.id) {
                Some(existing) => {
                    context_memory_hit_priority(&hit, &objective)
                        > context_memory_hit_priority(existing, &objective)
                }
                None => true,
            };
            if keep {
                merged.insert(hit.id, hit);
            }
        }
    }

    let mut hits = merged.into_values().collect::<Vec<_>>();
    let has_atomic_claims = hits.iter().any(is_atomic_context_claim);
    let wants_implementation_detail = wants_implementation_detail(&objective);
    if has_atomic_claims {
        hits.retain(|hit| {
            !matches!(hit.memory_type, MemoryType::Summary)
                && (wants_implementation_detail
                    || !matches!(hit.memory_type, MemoryType::ImplementationDetail))
        });
    }
    hits.sort_by(|left, right| {
        context_memory_hit_priority(right, &objective)
            .cmp(&context_memory_hit_priority(left, &objective))
            .then_with(|| {
                right
                    .score
                    .partial_cmp(&left.score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    });
    hits.truncate(CONTEXT_BUILD_SEARCH_LIMIT as usize);
    Ok(hits)
}

/// v2.2.3: Governance transition — pin / archive / reject / reactivate claims.
/// Accepts either a claim ID or a memory ID (1:1 via unique constraint).
async fn perform_claim_govern(
    state: &ApiState,
    id: Uuid,
    input: GovernClaimRequest,
) -> Result<GovernClaimResponse, DomainError> {
    let mut tx = begin_tx(state, &state.scope).await?;

    let row = sqlx::query_as::<_, (Uuid, String)>(
        "SELECT id, governance_state FROM public.claims WHERE id = $1 OR memory_id = $1",
    )
    .bind(id)
    .fetch_optional(tx.as_mut())
    .await
    .map_err(|e| DomainError::Internal(e.to_string()))?
    .ok_or_else(|| DomainError::NotFound(format!("Claim {id} not found")))?;

    let claim_id = row.0;
    let previous_state: GovernanceState = row.1.parse().unwrap_or_default();
    let new_state = input.new_state;

    sqlx::query(
        "UPDATE public.claims SET governance_state = $1, updated_at = now() WHERE id = $2",
    )
    .bind(new_state.as_str())
    .bind(claim_id)
    .execute(tx.as_mut())
    .await
    .map_err(|e| DomainError::Internal(e.to_string()))?;

    let transition_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO public.claim_governance_history \
         (id, organization_id, team_id, project_id, claim_id, previous_state, new_state, reason, actor_type) \
         SELECT $1, organization_id, team_id, project_id, $2, $3, $4, $5, $6 \
         FROM public.claims WHERE id = $2",
    )
    .bind(transition_id)
    .bind(claim_id)
    .bind(previous_state.as_str())
    .bind(new_state.as_str())
    .bind(input.reason.as_deref())
    .bind(match state.scope.actor_type {
        chum_mem_contracts::ActorType::User => "user",
        chum_mem_contracts::ActorType::Token => "token",
        chum_mem_contracts::ActorType::System => "system",
    })
    .execute(tx.as_mut())
    .await
    .map_err(|e| DomainError::Internal(e.to_string()))?;

    commit_tx(tx).await?;

    Ok(GovernClaimResponse {
        claim_id,
        previous_state,
        new_state,
        transition_id,
    })
}

async fn perform_memory_get(
    state: &ApiState,
    memory_id: Uuid,
) -> Result<GetMemoryResponse, DomainError> {
    let mut tx = begin_tx(state, &state.scope).await?;
    let memory = load_memory(&mut tx, &state.scope, memory_id)
        .await?
        .ok_or_else(|| DomainError::NotFound(format!("Memory {memory_id} not found")))?;
    let provenance =
        load_memory_provenance(&mut tx, &[memory_id], SEARCH_PROVENANCE_LIMIT_DEFAULT).await?;
    let edges = load_memory_edges_for_ids(&mut tx, &[memory_id]).await?;
    let claim_proofs = load_claim_proofs(&mut tx, &[memory_id]).await?;
    let claim_relations = load_claim_relations_for_memory_ids(&mut tx, &[memory_id]).await?;
    commit_tx(tx).await?;
    let mut provenance_map = map_provenance_rows(&provenance);
    let claim_proof_map = map_claim_proof_rows(&claim_proofs);
    let claim_relation_map = map_claim_relation_rows(&claim_relations);
    let provenance_handles = provenance_map.remove(&memory_id).unwrap_or_default();
    let memory_metadata = memory.metadata.clone();

    Ok(GetMemoryResponse {
        id: memory.id,
        project_id: memory.project_id,
        memory_type: parse_memory_type(&memory.memory_type),
        title: memory.title,
        content: memory.content,
        summary: memory.summary,
        metadata: memory_metadata.clone(),
        provenance: provenance_handles.clone(),
        proof_handles: claim_proof_map
            .get(&memory_id)
            .cloned()
            .unwrap_or_else(|| build_proof_handles(&memory_metadata, &provenance_handles)),
        related_memory_ids: edges
            .into_iter()
            .filter_map(|(left, right)| {
                if left == memory_id {
                    Some(right)
                } else if right == memory_id {
                    Some(left)
                } else {
                    None
                }
            })
            .collect(),
        claim_relations: claim_relation_map
            .get(&memory_id)
            .cloned()
            .unwrap_or_default(),
        claim_id: memory.claim_id,
        claim_key: memory
            .claim_key
            .clone()
            .or_else(|| metadata_string(&memory_metadata, "claimKey")),
        claim_type: memory.claim_type.as_deref().map(parse_memory_type),
        authority_class: memory
            .claim_authority_class
            .as_deref()
            .and_then(parse_authority_class)
            .or_else(|| metadata_authority_class(&memory_metadata)),
        verification_status: memory
            .claim_verification_status
            .as_deref()
            .and_then(parse_verification_status)
            .or_else(|| metadata_verification_status(&memory_metadata)),
        valid_from: memory.claim_valid_from.map(format_time),
        valid_to: memory.claim_valid_to.map(format_time),
        superseded_by: memory.claim_superseded_by,
    })
}

async fn perform_memory_batch(
    state: &ApiState,
    ids: Vec<Uuid>,
) -> Result<Vec<GetMemoryResponse>, DomainError> {
    let mut tx = begin_tx(state, &state.scope).await?;
    let rows = load_memories_batch(&mut tx, &state.scope, &ids).await?;
    let found_ids = rows.iter().map(|row| row.id).collect::<Vec<_>>();
    let provenance = load_memory_provenance(&mut tx, &found_ids, 8).await?;
    let related_edges = load_memory_edges_for_ids(&mut tx, &found_ids).await?;
    let claim_proofs = load_claim_proofs(&mut tx, &found_ids).await?;
    let claim_relations = load_claim_relations_for_memory_ids(&mut tx, &found_ids).await?;
    commit_tx(tx).await?;

    let provenance_map = map_provenance_rows(&provenance);
    let related_map = build_related_map(&related_edges);
    let claim_proof_map = map_claim_proof_rows(&claim_proofs);
    let claim_relation_map = map_claim_relation_rows(&claim_relations);
    Ok(rows
        .into_iter()
        .map(|row| GetMemoryResponse {
            id: row.id,
            project_id: row.project_id,
            memory_type: parse_memory_type(&row.memory_type),
            title: row.title,
            content: row.content,
            summary: row.summary,
            metadata: row.metadata.clone(),
            provenance: provenance_map.get(&row.id).cloned().unwrap_or_default(),
            proof_handles: claim_proof_map.get(&row.id).cloned().unwrap_or_else(|| {
                build_proof_handles(
                    &row.metadata,
                    &provenance_map.get(&row.id).cloned().unwrap_or_default(),
                )
            }),
            related_memory_ids: related_map.get(&row.id).cloned().unwrap_or_default(),
            claim_relations: claim_relation_map.get(&row.id).cloned().unwrap_or_default(),
            claim_id: row.claim_id,
            claim_key: row
                .claim_key
                .clone()
                .or_else(|| metadata_string(&row.metadata, "claimKey")),
            claim_type: row.claim_type.as_deref().map(parse_memory_type),
            authority_class: row
                .claim_authority_class
                .as_deref()
                .and_then(parse_authority_class)
                .or_else(|| metadata_authority_class(&row.metadata)),
            verification_status: row
                .claim_verification_status
                .as_deref()
                .and_then(parse_verification_status)
                .or_else(|| metadata_verification_status(&row.metadata)),
            valid_from: row.claim_valid_from.map(format_time),
            valid_to: row.claim_valid_to.map(format_time),
            superseded_by: row.claim_superseded_by,
        })
        .collect())
}

async fn perform_dashboard_summary(
    state: &ApiState,
) -> Result<DashboardSummaryResponse, DomainError> {
    let mut tx = begin_tx(state, &state.scope).await?;
    let (total_memories, total_sessions, total_projects, estimated_token_savings) =
        load_dashboard_summary(&mut tx, &state.scope).await?;
    commit_tx(tx).await?;
    Ok(DashboardSummaryResponse {
        total_memories,
        total_sessions,
        total_projects,
        estimated_token_savings,
    })
}

async fn perform_dashboard_graph(
    state: &ApiState,
    project_id: Option<Uuid>,
    layer: Option<&str>,
) -> Result<DashboardGraphResponse, DomainError> {
    let context = scoped_context(&state.scope, project_id)?;
    let mut tx = begin_tx(state, &context).await?;
    // When the caller specifies a layer, return only that layer's snapshot.
    // When no layer is specified (e.g. the MCP graph_snapshot tool), merge
    // both layers so callers without layer awareness see the full graph.
    let has_project = project_id.is_some();
    let (repo_snapshot, session_snapshot) = match layer {
        Some("repository") => (
            if has_project {
                load_latest_knowledge_graph_by_type(&mut tx, &context, Some("repository")).await?
            } else {
                load_merged_snapshots_by_type(&mut tx, &context, "repository").await?
            },
            None,
        ),
        Some("session") => (
            None,
            if has_project {
                load_latest_knowledge_graph_by_type(&mut tx, &context, Some("session")).await?
            } else {
                load_merged_snapshots_by_type(&mut tx, &context, "session").await?
            },
        ),
        _ => (
            if has_project {
                load_latest_knowledge_graph_by_type(&mut tx, &context, Some("repository")).await?
            } else {
                load_merged_snapshots_by_type(&mut tx, &context, "repository").await?
            },
            if has_project {
                load_latest_knowledge_graph_by_type(&mut tx, &context, Some("session")).await?
            } else {
                load_merged_snapshots_by_type(&mut tx, &context, "session").await?
            },
        ),
    };
    let response = match (repo_snapshot, session_snapshot) {
        (Some(repo), Some(session)) => {
            let merged = merge_graphs(&repo, &session);
            let projection = GraphProjection {
                total_nodes: merged.nodes.len(),
                total_edges: merged.edges.len(),
                returned_nodes: merged.nodes.len(),
                returned_edges: merged.edges.len(),
            };
            map_dashboard_graph(merged, projection)
        }
        (Some(graph), None) | (None, Some(graph)) => {
            let projection = GraphProjection {
                total_nodes: graph.nodes.len(),
                total_edges: graph.edges.len(),
                returned_nodes: graph.nodes.len(),
                returned_edges: graph.edges.len(),
            };
            map_dashboard_graph(graph, projection)
        }
        (None, None) => {
            let nodes = load_memory_graph_nodes(&mut tx, &context, i64::MAX).await?;
            let links = load_memory_graph_edges(&mut tx, &context, i64::MAX).await?;
            map_dashboard_graph_fallback(nodes, links)
        }
    };
    commit_tx(tx).await?;
    Ok(response)
}

async fn perform_knowledge_export(
    state: &ApiState,
    project_id: Option<Uuid>,
    layer: Option<&str>,
) -> Result<KnowledgeExportResponse, DomainError> {
    let context = scoped_context(&state.scope, project_id)?;
    let mut tx = begin_tx(state, &context).await?;
    let graph = load_latest_knowledge_graph_by_type(&mut tx, &context, layer)
        .await?
        .ok_or_else(|| DomainError::NotFound("No knowledge graph snapshot found".to_string()))?;
    let artifacts = load_latest_snapshot_artifacts_by_type(&mut tx, &context, layer).await?;
    commit_tx(tx).await?;
    let projection = GraphProjection {
        total_nodes: graph.nodes.len(),
        total_edges: graph.edges.len(),
        returned_nodes: graph.nodes.len(),
        returned_edges: graph.edges.len(),
    };
    let node_link_json = artifacts
        .and_then(|artifact| artifact.node_link_json)
        .unwrap_or_else(|| to_node_link_json(&graph));
    Ok(KnowledgeExportResponse {
        project_id,
        generated_at: graph.generated_at.clone(),
        node_link_json,
        graph: map_dashboard_graph(graph, projection),
    })
}

async fn perform_knowledge_report(
    state: &ApiState,
    project_id: Option<Uuid>,
    layer: Option<&str>,
) -> Result<KnowledgeReportResponse, DomainError> {
    let context = scoped_context(&state.scope, project_id)?;
    let mut tx = begin_tx(state, &context).await?;

    // v2.2.2: Support unified layer that merges repository + session reports
    if layer == Some("unified") {
        let repo_graph =
            load_latest_knowledge_graph_by_type(&mut tx, &context, Some("repository")).await?;
        let session_graph =
            load_latest_knowledge_graph_by_type(&mut tx, &context, Some("session")).await?;
        commit_tx(tx).await?;

        let repo_report = repo_graph
            .as_ref()
            .map(|g| generate_knowledge_report(g))
            .unwrap_or_default();
        let session_report = session_graph
            .as_ref()
            .map(|g| generate_knowledge_report(g))
            .unwrap_or_default();

        // Build unified cross-layer summary
        let cross_layer = build_unified_cross_layer_summary(
            repo_graph.as_ref(),
            session_graph.as_ref(),
        );

        let unified_markdown = format!(
            "# Unified Knowledge Report\n\n\
             ## Repository Layer\n\n{}\n\n\
             ## Session Layer\n\n{}\n\n\
             ## Cross-Layer Summary\n\n{}",
            repo_report, session_report, cross_layer
        );
        let generated_at = repo_graph
            .as_ref()
            .or(session_graph.as_ref())
            .map(|g| g.generated_at.clone())
            .unwrap_or_default();
        return Ok(KnowledgeReportResponse {
            project_id,
            generated_at,
            report_markdown: unified_markdown,
            cross_layer_summary: Some(cross_layer),
        });
    }

    // Fast path: return pre-computed artifact report (avoids deserializing the
    // full JSONB graph snapshot which is ~200-400ms for large graphs).
    let artifacts = load_latest_snapshot_artifacts_by_type(&mut tx, &context, layer).await?;
    if let Some(ref arts) = artifacts {
        if let Some(ref md) = arts.report_markdown {
            if !md.is_empty() {
                commit_tx(tx).await?;
                return Ok(KnowledgeReportResponse {
                    project_id,
                    generated_at: arts.computed_at.clone().unwrap_or_default(),
                    report_markdown: md.clone(),
                    cross_layer_summary: None,
                });
            }
        }
    }

    // Slow path: load full graph snapshot and regenerate report.
    let graph = load_latest_knowledge_graph_by_type(&mut tx, &context, layer)
        .await?
        .ok_or_else(|| DomainError::NotFound("No knowledge graph snapshot found".to_string()))?;
    commit_tx(tx).await?;
    Ok(KnowledgeReportResponse {
        project_id,
        generated_at: graph.generated_at.clone(),
        report_markdown: generate_knowledge_report(&graph),
        cross_layer_summary: None,
    })
}

/// v2.2.2: Build a cross-layer summary from repository + session graphs.
fn build_unified_cross_layer_summary(
    repo_graph: Option<&KnowledgeGraph>,
    session_graph: Option<&KnowledgeGraph>,
) -> String {
    let mut lines = Vec::new();

    // Most-modified files (file nodes with most edges in session graph)
    if let Some(sg) = session_graph {
        let mut file_edge_count: std::collections::HashMap<&str, usize> =
            std::collections::HashMap::new();
        for edge in &sg.edges {
            if edge.relation == "modifies" || edge.relation == "touched_by" {
                if let Some(file_path) = edge.target.strip_prefix("file:").or_else(|| edge.source.strip_prefix("file:")) {
                    *file_edge_count.entry(file_path).or_default() += 1;
                }
            }
        }
        let mut sorted: Vec<_> = file_edge_count.into_iter().collect();
        sorted.sort_by(|a, b| b.1.cmp(&a.1));
        if !sorted.is_empty() {
            lines.push("### Most Modified Files".to_string());
            for (path, count) in sorted.iter().take(10) {
                lines.push(format!("- `{}` ({} sessions)", path, count));
            }
            lines.push(String::new());
        }

        // Active decisions, open tasks, known bugs
        let decisions: Vec<_> = sg.nodes.iter().filter(|n| n.node_type == "decision").collect();
        let tasks: Vec<_> = sg.nodes.iter().filter(|n| n.node_type == "task").collect();
        let bugs: Vec<_> = sg.nodes.iter().filter(|n| n.node_type == "bug").collect();

        if !decisions.is_empty() {
            lines.push(format!("### Active Decisions ({})", decisions.len()));
            for d in decisions.iter().take(5) {
                lines.push(format!("- {}", d.label));
            }
            lines.push(String::new());
        }
        if !tasks.is_empty() {
            lines.push(format!("### Open Tasks ({})", tasks.len()));
            for t in tasks.iter().take(5) {
                lines.push(format!("- {}", t.label));
            }
            lines.push(String::new());
        }
        if !bugs.is_empty() {
            lines.push(format!("### Known Bugs ({})", bugs.len()));
            for b in bugs.iter().take(5) {
                lines.push(format!("- {}", b.label));
            }
            lines.push(String::new());
        }
    }

    // God nodes (domain hubs) from repository graph
    if let Some(rg) = repo_graph {
        let mut degree: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
        for edge in &rg.edges {
            *degree.entry(&edge.source).or_default() += 1;
            *degree.entry(&edge.target).or_default() += 1;
        }
        let mut hub_nodes: Vec<_> = rg
            .nodes
            .iter()
            .filter(|n| n.node_type != "session" && n.node_type != "module")
            .map(|n| (n, *degree.get(n.id.as_str()).unwrap_or(&0)))
            .collect();
        hub_nodes.sort_by(|a, b| b.1.cmp(&a.1));
        if !hub_nodes.is_empty() {
            lines.push("### Architectural Hubs".to_string());
            for (node, deg) in hub_nodes.iter().take(10) {
                lines.push(format!("- `{}` (degree {})", node.label, deg));
            }
            lines.push(String::new());
        }
    }

    if lines.is_empty() {
        "No cross-layer data available.".to_string()
    } else {
        lines.join("\n")
    }
}

async fn perform_knowledge_communities(
    state: &ApiState,
    project_id: Option<Uuid>,
    layer: Option<&str>,
) -> Result<KnowledgeCommunitiesResponse, DomainError> {
    let context = scoped_context(&state.scope, project_id)?;
    let mut tx = begin_tx(state, &context).await?;
    let graph = load_latest_knowledge_graph_by_type(&mut tx, &context, layer)
        .await?
        .ok_or_else(|| DomainError::NotFound("No knowledge graph snapshot found".to_string()))?;
    commit_tx(tx).await?;
    Ok(KnowledgeCommunitiesResponse {
        project_id,
        communities: graph
            .communities
            .into_iter()
            .map(|community| GraphCommunity {
                community_id: community.community_id,
                node_count: community.node_count,
                representative_nodes: community.representative_nodes,
                level: community.level,
                community_path: community.community_path,
            })
            .collect(),
    })
}

async fn perform_knowledge_query(
    state: &ApiState,
    input: KnowledgeQueryRequest,
    layer: Option<&str>,
) -> Result<GraphQueryResponse, DomainError> {
    let context = scoped_context(&state.scope, input.project_id)?;
    let mut tx = begin_tx(state, &context).await?;
    let graph = load_latest_knowledge_graph_by_type(&mut tx, &context, layer).await?;
    commit_tx(tx).await?;

    let graph = graph
        .ok_or_else(|| DomainError::NotFound("No knowledge graph snapshot found".to_string()))?;
    Ok(run_knowledge_query(
        &graph,
        knowledge_query_kind_str(input.query),
        input.node_id.as_deref(),
        input.target_node_id.as_deref(),
        input.text.as_deref(),
        input.depth.unwrap_or(1) as usize,
    ))
}

async fn perform_repository_sync(
    state: &ApiState,
    input: RepositorySyncRequest,
) -> Result<RepositorySyncResponse, DomainError> {
    let project_id = input.project_id.or(state.scope.project_id).ok_or_else(|| {
        DomainError::BadRequest(
            "projectId is required when CHUM_MEM_PROJECT_ID is not configured".to_string(),
        )
    })?;
    let context = scoped_context(&state.scope, Some(project_id))?;
    let max_nodes = state.config.knowledge_graph_max_cluster_nodes as usize;
    let max_edges = state.config.knowledge_graph_max_cluster_edges as usize;
    let merge_with_existing = input.merge_with_existing;
    let files_added = input.files.len() as u32;
    let files_removed = input.removed_paths.len() as u32;

    // Parse the incoming file contents in a blocking task (tree-sitter is CPU-bound)
    let file_pairs: Vec<(String, String)> = input
        .files
        .iter()
        .map(|f| (f.path.clone(), f.content.clone()))
        .collect();
    let removed_paths = input.removed_paths.clone();

    let (new_nodes, new_edges) = if !file_pairs.is_empty() {
        tokio::task::spawn_blocking(move || chum_mem_pipeline::parse_file_batch(&file_pairs))
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?
    } else {
        (Vec::new(), Vec::new())
    };

    let mut tx = begin_tx(state, &context).await?;
    sqlx::query("select pg_advisory_xact_lock($1)")
        .bind(project_advisory_lock_key(project_id))
        .execute(&mut *tx)
        .await
        .map_err(DbError::from)?;

    let existing = if merge_with_existing {
        load_latest_knowledge_graph_by_type(&mut tx, &context, Some("repository")).await?
    } else {
        None
    };

    let graph = if let Some(mut existing_graph) = existing.clone() {
        // Remove nodes/edges belonging to removed or re-synced files
        let stale_prefixes: HashSet<String> = removed_paths
            .iter()
            .chain(input.files.iter().map(|f| &f.path))
            .map(|p| format!("file:{p}"))
            .collect();
        let stale_paths: HashSet<&str> = removed_paths
            .iter()
            .chain(input.files.iter().map(|f| &f.path))
            .map(|s| s.as_str())
            .collect();

        existing_graph.nodes.retain(|n| {
            !stale_prefixes.contains(&n.id)
                && !n
                    .id
                    .starts_with("symbol:")
                    .then(|| {
                        // symbol:path/to/file.ts:SymbolName — check if path segment matches
                        n.metadata
                            .get("sourceFile")
                            .and_then(|v| v.as_str())
                            .map_or(false, |sf| stale_paths.contains(sf))
                    })
                    .unwrap_or(false)
        });
        existing_graph.edges.retain(|e| {
            let src_file = e.source.strip_prefix("file:").unwrap_or(&e.source);
            let tgt_file = e.target.strip_prefix("file:").unwrap_or(&e.target);
            !stale_paths.contains(src_file)
                && !stale_paths.contains(tgt_file)
                && !e
                    .source_file
                    .as_ref()
                    .map_or(false, |sf| stale_paths.contains(sf.as_str()))
        });

        // Merge in new nodes/edges
        existing_graph.nodes.extend(new_nodes);
        existing_graph.edges.extend(new_edges);

        chum_mem_pipeline::assign_communities_with_budget(&existing_graph, max_nodes, max_edges)
    } else {
        // No existing graph — build fresh from the sync payload
        chum_mem_pipeline::assign_communities_with_budget(
            &KnowledgeGraph {
                version: "1.0.0".to_string(),
                generated_at: OffsetDateTime::now_utc()
                    .format(&time::format_description::well_known::Rfc3339)
                    .unwrap_or_default(),
                project_id,
                nodes: new_nodes,
                edges: new_edges,
                communities: Vec::new(),
                statistics: chum_mem_pipeline::GraphStatistics {
                    node_count: 0,
                    edge_count: 0,
                    community_count: 0,
                    evidence_distribution: Default::default(),
                    avg_degree: 0.0,
                    density: 0.0,
                    isolated_nodes: 0,
                },
            },
            max_nodes,
            max_edges,
        )
    };

    let graph_file_paths: HashSet<String> = graph
        .nodes
        .iter()
        .filter(|n| n.node_type == "file" || n.node_type == "document")
        .filter_map(|n| n.id.strip_prefix("file:").map(|s| s.to_string()))
        .collect();
    let total_files = graph_file_paths.len() as u32;

    let accepted_paths: Vec<String> = input
        .files
        .iter()
        .map(|f| f.path.clone())
        .filter(|p| graph_file_paths.contains(p))
        .collect();

    let missing_paths: Vec<String> = input
        .manifest
        .keys()
        .filter(|p| !graph_file_paths.contains(p.as_str()))
        .cloned()
        .collect();

    persist_knowledge_snapshot_typed(&mut tx, &context, project_id, &graph, "repository").await?;
    commit_tx(tx).await?;

    Ok(RepositorySyncResponse {
        status: "SUCCESSFUL".to_string(),
        project_id,
        merged_with_existing: merge_with_existing && existing.is_some(),
        generated_at: graph.generated_at.clone(),
        stats: RepositorySyncStats {
            files_added,
            files_removed,
            files_unchanged: total_files.saturating_sub(files_added),
            total_files,
        },
        graph_summary: ProjectImportGraphSummary {
            node_count: graph.statistics.node_count as u32,
            edge_count: graph.statistics.edge_count as u32,
            community_count: graph.statistics.community_count as u32,
            evidence_distribution: chum_mem_contracts::EvidenceDistributionContract {
                extracted: graph.statistics.evidence_distribution.extracted as u32,
                inferred: graph.statistics.evidence_distribution.inferred as u32,
                ambiguous: graph.statistics.evidence_distribution.ambiguous as u32,
            },
        },
        accepted_paths,
        missing_paths,
    })
}

async fn derive_and_persist_session_memories(
    tx: &mut Transaction<'_, Postgres>,
    context: &RepositoryContext,
    project_id: Uuid,
    provider: &str,
    session_id: Uuid,
    repo_url: Option<&str>,
    branch: Option<&str>,
    end_request: &EndSessionRequest,
) -> Result<DerivedPersistenceResult, DomainError> {
    let event_rows = load_session_events(tx, session_id).await?;
    let records = event_rows
        .iter()
        .map(map_session_event_record)
        .collect::<Vec<_>>();
    let episodes =
        derive_session_episodes(session_id, parse_provider(provider), end_request, &records);
    let batch_rows: Vec<chum_mem_db::EpisodeBatchRow> = episodes
        .iter()
        .map(|ep| chum_mem_db::EpisodeBatchRow {
            episode_ordinal: ep.episode_ordinal,
            episode_type: ep.episode_type.clone(),
            title: ep.title.clone(),
            summary: ep.summary.clone(),
            started_at: ep.started_at.clone(),
            ended_at: ep.ended_at.clone(),
            metadata: ep.metadata.clone(),
        })
        .collect();
    let persisted_episodes =
        upsert_session_episodes_batch(tx, context, project_id, session_id, &batch_rows).await?;
    let episode_ids: HashMap<i32, Uuid> = persisted_episodes
        .into_iter()
        .map(|ep| (ep.episode_ordinal, ep.id))
        .collect();

    let drafts = derive_memories_from_session(
        session_id,
        parse_provider(provider),
        end_request,
        &records,
        Some(&episodes),
    );
    let unresolved_risk = drafts.iter().any(|draft| {
        draft
            .metadata
            .get("derivation")
            .and_then(Value::as_str)
            .is_some_and(|value| value == "session_reflection_v1")
            && draft
                .metadata
                .get("unresolvedRisk")
                .and_then(Value::as_bool)
                .unwrap_or(false)
    });

    // v2.2.1 ingestion-choke fix:
    //   - Previously this path acquired one advisory lock per (claim_key, claim_subject)
    //     pair — hundreds of xact-scoped locks per session_end — and ran
    //     reconcile_claim_memory_state() inline per draft. That blew the
    //     max_locks_per_transaction budget and deadlocked under concurrent imports.
    //   - Now we take exactly ONE advisory lock per session (bounded, O(1) per xact)
    //     and defer reconciliation to an async worker job enqueued at the end of
    //     this function. See infra/migrations/0013_reconcile_claim_state_job.sql.
    sqlx::query("select pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(format!(
            "chum-mem:session-end:{}:{}",
            project_id, session_id
        ))
        .execute(&mut **tx)
        .await
        .map_err(DbError::from)?;

    // Accumulate provenance rows across every admitted draft and flush them in a
    // single multi-row INSERT at the end of the loop (was one INSERT per
    // provenance event; now one INSERT per session_end).
    let mut batched_provenance: Vec<(Uuid, Uuid, Option<String>)> = Vec::new();
    // IDs of freshly-created claims that the async reconciliation worker will
    // process after commit.
    let mut new_claim_ids: Vec<Uuid> = Vec::new();

    let mut derived_memories = 0_i32;
    for draft in drafts {
        if !draft_is_belief_admitted(&draft.metadata) {
            continue;
        }
        let episode_ordinal = draft
            .metadata
            .get("episodeOrdinal")
            .and_then(Value::as_i64)
            .map(|value| value as i32);
        let derivation_key = format!(
            "{}:{}:{}:{}",
            session_id,
            draft
                .metadata
                .get("derivation")
                .and_then(Value::as_str)
                .unwrap_or("derived"),
            episode_ordinal
                .map(|value| value.to_string())
                .unwrap_or_else(|| "session".to_string()),
            memory_type_str(draft.memory_type)
        );
        let existing = sqlx::query_scalar::<_, Uuid>(
            r#"
            select id
            from public.memories
            where session_id = $1
              and metadata->>'derivationKey' = $2
            limit 1
            "#,
        )
        .bind(session_id)
        .bind(&derivation_key)
        .fetch_optional(&mut **tx)
        .await
        .map_err(DbError::from)?;
        if existing.is_some() {
            continue;
        }

        let mut metadata = match draft.metadata {
            Value::Object(existing) => existing,
            _ => serde_json::Map::new(),
        };
        metadata.insert(
            "derivationKey".to_string(),
            Value::String(derivation_key.clone()),
        );
        let metadata_value = Value::Object(metadata);

        let memory_id = insert_memory(
            tx,
            context,
            project_id,
            context.actor_id,
            &MemoryInsertParams {
                session_id,
                episode_id: episode_ordinal.and_then(|value| episode_ids.get(&value).copied()),
                memory_type: memory_type_str(draft.memory_type).to_string(),
                title: draft.title.clone(),
                content: draft.content.clone(),
                summary: draft.summary.clone(),
                importance_score: draft.importance_score,
                confidence_score: draft.confidence_score,
                metadata: metadata_value.clone(),
            },
        )
        .await?;
        let embedding = embed_text(&format!(
            "{}\n{}\n{}",
            draft.title, draft.summary, draft.content
        ));
        upsert_embedding(
            tx,
            context,
            project_id,
            memory_id,
            "local-hash-1536-v1",
            &to_pgvector_literal(&embedding),
        )
        .await?;
        // Collect provenance rows instead of inserting one at a time. Flushed
        // once after the draft loop.
        for provenance_event_id in &draft.provenance_event_ids {
            let excerpt = records
                .iter()
                .find(|event| event.id == *provenance_event_id)
                .map(event_text)
                .map(|value| truncate(&value, 500));
            batched_provenance.push((memory_id, *provenance_event_id, excerpt));
        }
        let preview = draft
            .provenance_event_ids
            .first()
            .and_then(|event_id| records.iter().find(|event| &event.id == event_id))
            .map(event_text)
            .map(|value| truncate(&value, 500));
        append_memory_provenance_preview(
            tx,
            context,
            project_id,
            memory_id,
            Some(session_id),
            draft.provenance_event_ids.first().copied(),
            preview.as_deref(),
        )
        .await?;
        let claim = upsert_claim(
            tx,
            context,
            project_id,
            &build_claim_upsert_params(memory_id, session_id, &metadata_value, draft.memory_type),
        )
        .await?;
        replace_claim_proofs(
            tx,
            context,
            project_id,
            claim.id,
            &build_claim_proof_insert_params(
                claim.id,
                memory_id,
                &metadata_value,
                &records,
                &draft.provenance_event_ids,
            ),
        )
        .await?;
        // v2.2.1: reconciliation (supersedes / contradicts / confirms) is now
        // handled by the `reconcile-claim-state` worker job enqueued below,
        // NOT inline. This keeps session_end off the critical lock-budget path.
        new_claim_ids.push(claim.id);
        derived_memories += 1;
    }

    // One multi-row INSERT for every admitted draft's provenance rows.
    if !batched_provenance.is_empty() {
        append_memory_provenance_batch(tx, context, project_id, &batched_provenance).await?;
    }

    // Enqueue the async reconciliation job exactly once per session_end.
    // Per-project dedupe_key lets multiple sessions in flight coalesce if the
    // worker is behind; the worker picks up the latest payload and chunks it.
    if !new_claim_ids.is_empty() {
        enqueue_worker_job(
            tx,
            context,
            project_id,
            Some(session_id),
            None,
            "reconcile-claim-state",
            &format!("reconcile-claims:{project_id}"),
            40,
            5,
            None,
            &json!({
                "projectId": project_id,
                "sessionId": session_id,
                "newClaimIds": new_claim_ids,
            }),
        )
        .await?;
    }

    let derived_session_edges = derive_and_persist_session_edges(
        tx, context, project_id, session_id, repo_url, branch, &records,
    )
    .await?;

    Ok(DerivedPersistenceResult {
        derived_memories,
        derived_episodes: episodes.len() as i32,
        derived_session_edges,
        unresolved_risk,
    })
}

fn draft_is_belief_admitted(metadata: &Value) -> bool {
    metadata
        .get("belief")
        .and_then(|value| value.get("admit"))
        .and_then(Value::as_bool)
        .unwrap_or(true)
}

// v2.2.1 ingestion-choke fix: `reconcile_claim_memory_state`,
// `sort_claim_supersession_candidates`, `claim_reconciliation_advisory_keys`,
// `current_supersedes_prior`, `claim_strength`, and `verification_rank` all
// moved out of the writer path. The reconciliation policy now lives in
// `chum_mem_pipeline::reconcile` and `chum_mem_db::reconcile`, and is driven
// by the `reconcile-claim-state` worker job (see
// `rust/apps/worker/src/main.rs::reconcile_claim_state_job`).
async fn derive_and_persist_session_edges(
    tx: &mut Transaction<'_, Postgres>,
    context: &RepositoryContext,
    project_id: Uuid,
    session_id: Uuid,
    repo_url: Option<&str>,
    branch: Option<&str>,
    current_records: &[SessionEventRecord],
) -> Result<i32, DomainError> {
    let current_signals = chum_mem_pipeline::extract_session_signals(current_records);
    let candidates =
        chum_mem_db::load_candidate_completed_sessions(tx, context, project_id, session_id).await?;
    let mut inserted_edges = 0_i32;

    for candidate in candidates {
        let candidate_rows = resolve_session_events_for_candidate(tx, candidate.id).await?;
        let candidate_records = candidate_rows
            .iter()
            .map(map_session_event_record)
            .collect::<Vec<_>>();
        let candidate_signals = chum_mem_pipeline::extract_session_signals(&candidate_records);
        if let Some(relationship) = score_session_relationship(
            repo_url,
            branch,
            &current_signals,
            candidate.repo_url.as_deref(),
            candidate.branch.as_deref(),
            &candidate_signals,
        ) {
            let (left, right) = if session_id < candidate.id {
                (session_id, candidate.id)
            } else {
                (candidate.id, session_id)
            };
            upsert_session_edge(
                tx,
                context,
                project_id,
                left,
                right,
                &relationship.edge_type,
                relationship.weight,
                &json!({ "reasons": relationship.reasons }),
            )
            .await?;
            inserted_edges += 1;
        }
    }

    Ok(inserted_edges)
}

async fn semantic_search(
    tx: &mut Transaction<'_, Postgres>,
    context: &RepositoryContext,
    input: &MemorySearchRequest,
) -> Result<Vec<chum_mem_pipeline::SemanticQueryResult>, DomainError> {
    let query_vector = to_pgvector_literal(&embed_text(&input.query));
    let rows = sqlx::query_as::<_, MemorySearchRow>(
        r#"
        with ann_shortlist as (
          select
            e.memory_id,
            greatest(0, 1 - (e.embedding <=> $4::vector))::float8 as semantic_score
          from public.embeddings e
          where e.organization_id = $1
            and e.team_id = $2
            and ($3::uuid is null or e.project_id = $3)
            and ($5::uuid is null or e.project_id = $5)
          order by e.embedding <=> $4::vector asc
          limit $6
        ),
        deduped as (
          select memory_id, max(semantic_score)::float8 as semantic_score
          from ann_shortlist
          group by memory_id
        )
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
          s.repo_url,
          s.branch,
          null::float8 as lexical_score,
          d.semantic_score,
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
        from deduped d
        join public.memories m on m.id = d.memory_id
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
          and ($7::uuid is null or m.session_id = $7)
          and ($8::text is null or s.provider = $8::public.provider_kind)
          and ($9::text is null or s.repo_url = $9)
          and ($10::text is null or s.branch = $10)
          and ($11::timestamptz is null or m.created_at >= $11::timestamptz)
          and ($12::timestamptz is null or m.created_at <= $12::timestamptz)
          and (cardinality($13::text[]) = 0 or m.type::text = any($13))
          and (
            $15::boolean
            or c.id is null
            or (
              c.admitted = true
              and c.superseded_by is null
              and c.valid_to is null
              and c.verification_status <> 'contradicted'
              and coalesce(c.governance_state, 'active') not in ('archived', 'rejected')
            )
          )
        order by d.semantic_score desc, m.created_at desc
        limit $14
        "#,
    )
    .bind(context.organization_id)
    .bind(context.team_id)
    .bind(context.project_id)
    .bind(&query_vector)
    .bind(input.project_id)
    .bind((input.limit * 3) as i64)
    .bind(input.session_id)
    .bind(input.provider.map(provider_str))
    .bind(input.repo_url.as_ref().map(|value| value.as_str()))
    .bind(input.branch.as_deref())
    .bind(input.from.as_deref())
    .bind(input.to.as_deref())
    .bind(
        input
            .types
            .iter()
            .map(|memory_type| memory_type_str(*memory_type).to_string())
            .collect::<Vec<_>>(),
    )
    .bind(input.limit as i64)
    .bind(input.include_historical.unwrap_or(false))
    .fetch_all(&mut **tx)
    .await
    .map_err(DbError::from)?;

    Ok(rows
        .into_iter()
        .map(|row| chum_mem_pipeline::SemanticQueryResult {
            id: row.id,
            distance: (1.0_f64 - row.semantic_score.unwrap_or(0.0_f64)).max(0.0_f64),
            document: Some(format!("{}\n{}", row.title, row.summary)),
            metadata: Some(json!({
                "projectId": row.project_id,
                "type": row.memory_type,
                "title": row.title,
                "summary": row.summary,
                "createdAt": format_time(row.created_at),
                "sessionIds": row.session_id.into_iter().collect::<Vec<_>>(),
                "sessionId": row.session_id,
                "repoUrl": row.repo_url,
                "branch": row.branch,
                "importanceScore": row.importance_score,
                "confidenceScore": row.confidence_score,
                "supersededAt": row.superseded_at.map(format_time),
                "claimId": row.claim_id,
                "claimType": row.claim_type,
                "claimKey": row.claim_key,
                "authorityClass": row.claim_authority_class,
                "verificationStatus": row.claim_verification_status,
                "validFrom": row.claim_valid_from.map(format_time),
                "validTo": row.claim_valid_to.map(format_time),
                "supersededBy": row.claim_superseded_by,
                "activeConflictCount": row.active_conflict_count,
                "governanceState": row.claim_governance_state,
            })),
        })
        .collect())
}

fn begin_tx<'a>(
    state: &'a ApiState,
    context: &'a RepositoryContext,
) -> impl std::future::Future<Output = Result<Transaction<'a, Postgres>, DomainError>> + 'a {
    async move {
        let mut tx = state.db.pool().begin().await.map_err(DbError::from)?;
        apply_repository_context(&mut *tx, context)
            .await
            .map_err(DbError::from)?;
        Ok(tx)
    }
}

async fn commit_tx(tx: Transaction<'_, Postgres>) -> Result<(), DomainError> {
    tx.commit()
        .await
        .map_err(DbError::from)
        .map_err(DomainError::Db)
}

fn map_ranked_memory(row: &MemorySearchRow, lexical: bool) -> RankedMemory {
    RankedMemory {
        id: row.id,
        project_id: row.project_id,
        memory_type: parse_memory_type(&row.memory_type),
        title: row.title.clone(),
        summary: row.summary.clone(),
        score: if lexical {
            row.lexical_score.unwrap_or(0.0)
        } else {
            row.semantic_score.unwrap_or(0.0)
        },
        created_at: format_time(row.created_at),
        session_ids: row.session_id.into_iter().collect(),
        provenance: Vec::new(),
        proof_handles: Vec::new(),
        lexical_score: if lexical { row.lexical_score } else { None },
        semantic_score: if lexical { None } else { row.semantic_score },
        exact_session_match: None,
        session_relevance_score: None,
        graph_proximity_score: None,
        recency_score: None,
        importance_score: Some(row.importance_score),
        confidence_score: Some(row.confidence_score),
        freshness_penalty: None,
        superseded_penalty: None,
        community_score: None,
        repo_url: row.repo_url.clone(),
        branch: row.branch.clone(),
        superseded_at: row.superseded_at.map(format_time),
        related_memory_ids: Vec::new(),
        source_class: metadata_string(&row.metadata, "sourceClass"),
        ranking_role: metadata_string(&row.metadata, "rankingRole"),
        claim_id: row.claim_id,
        claim_key: row
            .claim_key
            .clone()
            .or_else(|| metadata_string(&row.metadata, "claimKey")),
        claim_type: row.claim_type.as_deref().map(parse_memory_type),
        authority_class: row
            .claim_authority_class
            .as_deref()
            .and_then(parse_authority_class)
            .or_else(|| metadata_authority_class(&row.metadata)),
        verification_status: row
            .claim_verification_status
            .as_deref()
            .and_then(parse_verification_status)
            .or_else(|| metadata_verification_status(&row.metadata)),
        proof_type: metadata_proof_type(&row.metadata),
        valid_from: row.claim_valid_from.map(format_time),
        valid_to: row.claim_valid_to.map(format_time),
        superseded_by: row.claim_superseded_by,
        active_conflict_count: row.active_conflict_count,
        governance_state: row.claim_governance_state.clone(),
    }
}

fn map_memory_detail_to_ranked_memory(row: &chum_mem_db::MemoryDetailRow) -> RankedMemory {
    RankedMemory {
        id: row.id,
        project_id: row.project_id,
        memory_type: parse_memory_type(&row.memory_type),
        title: row.title.clone(),
        summary: row.summary.clone(),
        score: 0.0,
        created_at: format_time(row.created_at),
        session_ids: Vec::new(),
        provenance: Vec::new(),
        proof_handles: Vec::new(),
        lexical_score: None,
        semantic_score: None,
        exact_session_match: None,
        session_relevance_score: None,
        graph_proximity_score: None,
        recency_score: None,
        importance_score: None,
        confidence_score: None,
        freshness_penalty: None,
        superseded_penalty: None,
        community_score: None,
        repo_url: None,
        branch: None,
        superseded_at: None,
        related_memory_ids: Vec::new(),
        source_class: None,
        ranking_role: None,
        claim_id: row.claim_id,
        claim_key: row
            .claim_key
            .clone()
            .or_else(|| metadata_string(&row.metadata, "claimKey")),
        claim_type: row.claim_type.as_deref().map(parse_memory_type),
        authority_class: row
            .claim_authority_class
            .as_deref()
            .and_then(parse_authority_class)
            .or_else(|| metadata_authority_class(&row.metadata)),
        verification_status: row
            .claim_verification_status
            .as_deref()
            .and_then(parse_verification_status)
            .or_else(|| metadata_verification_status(&row.metadata)),
        proof_type: metadata_proof_type(&row.metadata),
        valid_from: row.claim_valid_from.map(format_time),
        valid_to: row.claim_valid_to.map(format_time),
        superseded_by: row.claim_superseded_by,
        active_conflict_count: row.active_conflict_count,
        governance_state: row.claim_governance_state.clone(),
    }
}

fn infer_retrieval_intent(input: &ContextBuildRequest) -> RetrievalIntent {
    let objective = input.objective.to_lowercase();
    let has_file_paths = !input.file_paths.is_empty();
    let memory_signals = [
        "continue",
        "continuation",
        "previous",
        "prior",
        "last session",
        "what did we decide",
        "open task",
        "open loop",
        "recent decision",
    ]
    .iter()
    .any(|signal| objective.contains(signal));
    let session_graph_signals = [
        "session history",
        "debugging history",
        "what happened in prior sessions",
        "past sessions",
        "session graph",
    ]
    .iter()
    .any(|signal| objective.contains(signal));
    let repository_signals = has_file_paths
        || [
            "codebase",
            "repository",
            "repo",
            "file",
            "symbol",
            "module",
            "import",
            "architecture",
            "api",
            "doc",
            "section",
            "heading",
            "class",
            "function",
            "struct",
        ]
        .iter()
        .any(|signal| objective.contains(signal));
    let transform_only = !memory_signals
        && !session_graph_signals
        && !repository_signals
        && [
            "rewrite",
            "rephrase",
            "summarize",
            "translate",
            "format",
            "fix grammar",
            "clean up wording",
        ]
        .iter()
        .any(|signal| objective.contains(signal));

    if transform_only {
        RetrievalIntent::None
    } else if session_graph_signals && !repository_signals && !memory_signals {
        RetrievalIntent::SessionGraphOnly
    } else if repository_signals && memory_signals {
        RetrievalIntent::Hybrid
    } else if repository_signals {
        RetrievalIntent::RepositoryOnly
    } else if memory_signals {
        RetrievalIntent::MemoryOnly
    } else {
        RetrievalIntent::Hybrid
    }
}

/// v2.2.3: Detect whether a query expresses continuation/resume intent.
fn is_continuation_query(query: &str) -> bool {
    let lower = query.to_lowercase();
    [
        "continue",
        "continuation",
        "resume",
        "pick up where",
        "where we left off",
        "what were we",
        "what was i",
        "prior work",
        "previous session",
        "last session",
        "open task",
        "open loop",
        "unfinished",
        "follow up",
        "what's next",
        "what is next",
        "next step",
    ]
    .iter()
    .any(|signal| lower.contains(signal))
}

/// v2.2.3: Section-aware type scopes.
///
/// Always includes baseline queries for all core sections so that
/// `context_build` / `context_compile_v2` can fill every typed section
/// even when the objective text doesn't contain section-specific keywords.
/// Keyword-matched sections get a higher per-scope limit (emphasis scopes).
fn context_memory_type_scopes(objective: &str) -> Vec<(Vec<MemoryType>, u32)> {
    let objective = objective.to_lowercase();

    // Baseline: every core section gets a low-limit query (2 hits each).
    // This ensures projectFacts, recentDecisions, knownBugs, openQuestions
    // etc. are populated even for generic objectives.
    let mut scopes: Vec<(Vec<MemoryType>, u32)> = vec![
        (vec![MemoryType::Decision], 2),
        (vec![MemoryType::Task], 2),
        (vec![MemoryType::Fact], 2),
        (vec![MemoryType::Constraint], 2),
        (vec![MemoryType::Bug, MemoryType::Fix], 2),
        (vec![MemoryType::OpenQuestion], 2),
    ];

    // Emphasis: keyword-matched sections get additional hits.
    if ["decision", "decide", "policy", "latest", "recent"]
        .iter()
        .any(|signal| objective.contains(signal))
    {
        scopes.push((vec![MemoryType::Decision], 4));
    }
    if ["constraint", "rule", "must", "guardrail"]
        .iter()
        .any(|signal| objective.contains(signal))
    {
        scopes.push((vec![MemoryType::Constraint], 4));
    }
    if ["open question", "unknown", "unresolved", "question"]
        .iter()
        .any(|signal| objective.contains(signal))
    {
        scopes.push((vec![MemoryType::OpenQuestion], 4));
    }
    if ["bug", "issue", "failure", "drift", "broken"]
        .iter()
        .any(|signal| objective.contains(signal))
    {
        scopes.push((vec![MemoryType::Bug, MemoryType::Fix], 4));
    }
    if [
        "task",
        "todo",
        "next step",
        "continue",
        "unfinished",
        "follow up",
    ]
    .iter()
    .any(|signal| objective.contains(signal))
    {
        scopes.push((vec![MemoryType::Task], 4));
    }
    if ["verified", "fact", "truth", "result", "evidence"]
        .iter()
        .any(|signal| objective.contains(signal))
    {
        scopes.push((vec![MemoryType::Fact, MemoryType::Fix], 4));
    }

    // v2.2.3: Continuation emphasis — when the objective expresses
    // resume/continue intent, boost the claim types that matter most
    // for picking up where a prior session left off.
    if is_continuation_query(&objective) {
        scopes.push((vec![MemoryType::Task], 4));
        scopes.push((vec![MemoryType::Decision], 4));
        scopes.push((vec![MemoryType::OpenQuestion], 3));
        scopes.push((vec![MemoryType::Constraint], 3));
        scopes.push((vec![MemoryType::Fix], 3));
    }

    scopes
}

fn context_memory_query_for_scope(objective: &str, scoped_types: &[MemoryType]) -> String {
    let hint = match scoped_types {
        [MemoryType::Decision] => "latest verified decision policy",
        [MemoryType::Constraint] => "latest verified constraint rule guardrail",
        [MemoryType::OpenQuestion] => "open question unresolved unknown",
        [MemoryType::Bug, MemoryType::Fix] => "verified fix bug state failure correction",
        [MemoryType::Task] => "active task unfinished follow up",
        [MemoryType::Fact, MemoryType::Fix] => "verified fact evidence result",
        [MemoryType::Fact] => "verified project fact",
        _ => "verified memory",
    };
    format!("{hint} {objective}")
}

fn context_memory_hit_priority(hit: &RankedMemory, objective: &str) -> i32 {
    let verification_weight = match hit.verification_status {
        Some(VerificationStatus::Verified) => 40,
        Some(VerificationStatus::UserConfirmed) => 34,
        Some(VerificationStatus::Inferred) => 10,
        Some(VerificationStatus::Unverified) => -6,
        Some(VerificationStatus::Contradicted) => -20,
        None => 0,
    };
    let authority_weight = match hit.authority_class {
        Some(AuthorityClass::Repository) => 30,
        Some(AuthorityClass::TestVerified) => 28,
        Some(AuthorityClass::ToolVerified) => 24,
        Some(AuthorityClass::UserConfirmed) => 18,
        Some(AuthorityClass::SessionDerived) => 5,
        Some(AuthorityClass::ModelDerived) => -12,
        None => 0,
    };
    let type_weight = match hit.memory_type {
        MemoryType::Constraint => 24,
        MemoryType::Decision => 22,
        MemoryType::Fix | MemoryType::Fact => 20,
        MemoryType::Task | MemoryType::OpenQuestion => 18,
        MemoryType::Bug => 16,
        MemoryType::ImplementationDetail => {
            if wants_implementation_detail(objective) {
                12
            } else {
                -4
            }
        }
        MemoryType::Summary | MemoryType::Risk | MemoryType::ChangeLog => -18,
    };
    let score_weight = (hit.score * 100.0).round() as i32;
    verification_weight + authority_weight + type_weight + score_weight
}

fn is_atomic_context_claim(hit: &RankedMemory) -> bool {
    matches!(
        hit.memory_type,
        MemoryType::Decision
            | MemoryType::Task
            | MemoryType::Constraint
            | MemoryType::Bug
            | MemoryType::Fix
            | MemoryType::Fact
            | MemoryType::OpenQuestion
    )
}

fn wants_implementation_detail(objective: &str) -> bool {
    let objective = objective.to_lowercase();
    [
        "implementation",
        "how",
        "code change",
        "patch",
        "trace",
        "step by step",
    ]
    .iter()
    .any(|signal| objective.contains(signal))
}

fn build_memory_context_items(hits: &[RankedMemory]) -> (Vec<ContextItem>, Vec<ContextItem>) {
    let mut memory_items = Vec::new();
    let mut conflict_items = Vec::new();

    for hit in hits.iter().take(CONTEXT_BUILD_SEARCH_LIMIT as usize) {
        let source_class = classify_memory_context_source(hit);
        let item = ContextItem {
            memory_id: Some(hit.id),
            reference_id: None,
            source_class,
            ranking_role: hit.ranking_role.clone(),
            memory_type: hit.memory_type,
            title: hit.title.clone(),
            summary: hit.summary.clone(),
            tokens: estimate_tokens(&format!("{}\n{}", hit.title, hit.summary)),
            provenance: hit.provenance.clone(),
            proof_handles: if hit.proof_handles.is_empty() {
                build_proof_handles_from_ranked_memory(hit)
            } else {
                hit.proof_handles.clone()
            },
            claim_id: hit.claim_id,
            claim_key: hit.claim_key.clone(),
            claim_type: hit.claim_type,
            authority_class: hit.authority_class,
            verification_status: hit.verification_status,
            valid_from: hit.valid_from.clone(),
            valid_to: hit.valid_to.clone(),
            superseded_by: hit.superseded_by,
        };
        if source_class == ContextSourceClass::Conflict
            || hit.verification_status == Some(VerificationStatus::Contradicted)
            || hit.active_conflict_count > 0
        {
            conflict_items.push(item);
        } else {
            memory_items.push(item);
        }
    }

    (memory_items, conflict_items)
}

fn classify_memory_context_source(hit: &RankedMemory) -> ContextSourceClass {
    if hit.superseded_at.is_some()
        || hit.verification_status == Some(VerificationStatus::Contradicted)
        || hit.active_conflict_count > 0
    {
        return ContextSourceClass::Conflict;
    }
    match hit.source_class.as_deref() {
        Some("session_summary") | Some("reflection") => ContextSourceClass::SessionGraph,
        _ => ContextSourceClass::Memory,
    }
}

fn build_repository_context_items(
    graph: &KnowledgeGraph,
    objective: &str,
    file_paths: &[String],
) -> Vec<ContextItem> {
    let mut nodes = run_knowledge_query(graph, "search", None, None, Some(objective), 1).nodes;
    for file_path in file_paths {
        let exact = format!("file:{file_path}");
        if let Some(node) = graph.nodes.iter().find(|node| node.id == exact) {
            nodes.push(node.clone());
            continue;
        }
        nodes.extend(run_knowledge_query(graph, "search", None, None, Some(file_path), 1).nodes);
    }
    graph_nodes_to_context_items(nodes, ContextSourceClass::Repository)
}

fn build_session_graph_context_items(graph: &KnowledgeGraph, objective: &str) -> Vec<ContextItem> {
    let response = run_knowledge_query(graph, "search", None, None, Some(objective), 1);
    graph_nodes_to_context_items(response.nodes, ContextSourceClass::SessionGraph)
}

fn graph_nodes_to_context_items(
    nodes: Vec<chum_mem_pipeline::KnowledgeNode>,
    source_class: ContextSourceClass,
) -> Vec<ContextItem> {
    let mut seen = HashSet::new();
    let mut items = Vec::new();

    for node in nodes {
        if !seen.insert(node.id.clone()) {
            continue;
        }
        let summary = summarize_knowledge_node(&node);
        if summary.is_empty() {
            continue;
        }
        items.push(ContextItem {
            memory_id: None,
            reference_id: Some(node.id.clone()),
            source_class,
            ranking_role: Some(node.node_type.clone()),
            memory_type: knowledge_node_memory_type(&node),
            title: node.label.clone(),
            summary: summary.clone(),
            tokens: estimate_tokens(&format!("{}\n{}", node.label, summary)),
            provenance: Vec::new(),
            proof_handles: vec![ProofHandle {
                proof_type: ProofType::Repository,
                source_ref: node.id.clone(),
                excerpt: Some(summary.clone()),
                session_id: None,
                session_event_id: None,
                authority_class: Some(AuthorityClass::Repository),
                verification_status: Some(VerificationStatus::Verified),
            }],
            claim_id: Some(Uuid::new_v4()),
            claim_key: Some(node.id.clone()),
            claim_type: Some(knowledge_node_memory_type(&node)),
            authority_class: Some(match source_class {
                ContextSourceClass::Repository => AuthorityClass::Repository,
                ContextSourceClass::SessionGraph => AuthorityClass::SessionDerived,
                ContextSourceClass::Conflict => AuthorityClass::SessionDerived,
                ContextSourceClass::Memory => AuthorityClass::SessionDerived,
            }),
            verification_status: Some(match source_class {
                ContextSourceClass::Repository => VerificationStatus::Verified,
                ContextSourceClass::Conflict => VerificationStatus::Contradicted,
                _ => VerificationStatus::Inferred,
            }),
            valid_from: None,
            valid_to: None,
            superseded_by: None,
        });
    }

    items
}

fn knowledge_node_memory_type(node: &chum_mem_pipeline::KnowledgeNode) -> MemoryType {
    match node.node_type.as_str() {
        "decision" => MemoryType::Decision,
        "task" => MemoryType::Task,
        "rationale" | "section" | "document" => MemoryType::Fact,
        "symbol" | "module" | "file" => MemoryType::ImplementationDetail,
        "error" => MemoryType::Bug,
        _ => MemoryType::Summary,
    }
}

fn summarize_knowledge_node(node: &chum_mem_pipeline::KnowledgeNode) -> String {
    let metadata = &node.metadata;
    let source_file = metadata
        .get("sourceFile")
        .or_else(|| metadata.get("fullPath"))
        .and_then(Value::as_str);
    let source_location = metadata.get("sourceLocation").and_then(Value::as_str);
    let symbol_kind = metadata.get("symbolKind").and_then(Value::as_str);
    let heading = metadata.get("heading").and_then(Value::as_str);
    let rationale_tag = metadata.get("tag").and_then(Value::as_str);
    let rationale_body = metadata.get("body").and_then(Value::as_str);
    let import_source = metadata.get("importSource").and_then(Value::as_str);
    let mut parts = Vec::new();
    parts.push(format!("{} node", node.node_type));
    if let Some(source_file) = source_file {
        parts.push(format!("source {source_file}"));
    }
    if let Some(source_location) = source_location {
        parts.push(format!("at {source_location}"));
    }
    if let Some(symbol_kind) = symbol_kind {
        parts.push(format!("kind {symbol_kind}"));
    }
    if let Some(heading) = heading {
        parts.push(format!("heading {heading}"));
    }
    if let Some(import_source) = import_source {
        parts.push(format!("import {import_source}"));
    }
    if let Some(rationale_tag) = rationale_tag {
        parts.push(format!("tag {rationale_tag}"));
    }
    if let Some(rationale_body) = rationale_body {
        parts.push(truncate_text(rationale_body, 180));
    } else if let Some(description) = metadata
        .get("description")
        .and_then(Value::as_str)
        .or_else(|| metadata.get("excerpt").and_then(Value::as_str))
    {
        parts.push(truncate_text(description, 180));
    }
    parts.join("; ")
}

fn truncate_text(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_string();
    }
    let truncated = value
        .chars()
        .take(max_chars.saturating_sub(1))
        .collect::<String>();
    format!("{truncated}…")
}

fn chroma_to_semantic_query_result(
    hit: &ChromaQueryResult,
) -> chum_mem_pipeline::SemanticQueryResult {
    chum_mem_pipeline::SemanticQueryResult {
        id: hit.id,
        distance: hit.distance,
        document: hit.document.clone(),
        metadata: hit.metadata.clone(),
    }
}

fn map_session_event_record(row: &SessionEventRow) -> SessionEventRecord {
    SessionEventRecord {
        id: row.id,
        event_type: parse_canonical_event_type(&row.event_type),
        payload: serde_json::from_value(row.payload.clone()).unwrap_or_default(),
        created_at: format_time(row.created_at),
    }
}

fn map_provenance_rows(
    rows: &[MemoryProvenanceRow],
) -> HashMap<Uuid, Vec<chum_mem_contracts::ProvenanceHandle>> {
    let mut map = HashMap::new();
    for row in rows {
        map.entry(row.memory_id).or_insert_with(Vec::new).push(
            chum_mem_contracts::ProvenanceHandle {
                session_id: row.session_id,
                session_event_id: row.session_event_id,
                excerpt: row.excerpt.clone(),
            },
        );
    }
    map
}

fn map_claim_proof_rows(rows: &[ClaimProofRow]) -> HashMap<Uuid, Vec<ProofHandle>> {
    let mut map = HashMap::<Uuid, Vec<ProofHandle>>::new();
    for row in rows {
        map.entry(row.memory_id).or_default().push(ProofHandle {
            proof_type: parse_proof_type(&row.proof_type).unwrap_or(ProofType::SessionEvent),
            source_ref: row.source_ref.clone(),
            excerpt: row.excerpt.clone(),
            session_id: row.session_id,
            session_event_id: row.session_event_id,
            authority_class: row
                .authority_class
                .as_deref()
                .and_then(parse_authority_class),
            verification_status: row
                .verification_status
                .as_deref()
                .and_then(parse_verification_status),
        });
    }
    map
}

fn map_claim_relation_rows(rows: &[ClaimRelationRow]) -> HashMap<Uuid, Vec<ClaimRelation>> {
    let mut map = HashMap::<Uuid, Vec<ClaimRelation>>::new();
    for row in rows {
        let Some(relation_type) = parse_claim_relation_type(&row.edge_type) else {
            continue;
        };
        map.entry(row.memory_id).or_default().push(ClaimRelation {
            claim_id: row.claim_id,
            related_claim_id: row.related_claim_id,
            related_memory_id: row.related_memory_id,
            relation_type,
            direction: row.direction.clone(),
            title: row.title.clone(),
            summary: row.summary.clone(),
            authority_class: row
                .authority_class
                .as_deref()
                .and_then(parse_authority_class),
            verification_status: row
                .verification_status
                .as_deref()
                .and_then(parse_verification_status),
        });
    }
    map
}

fn build_claim_upsert_params(
    memory_id: Uuid,
    session_id: Uuid,
    metadata: &Value,
    memory_type: MemoryType,
) -> ClaimUpsertParams {
    let claim_key = metadata_string(metadata, "claimKey")
        .unwrap_or_else(|| format!("{}:{memory_id}", memory_type_str(memory_type)));
    ClaimUpsertParams {
        memory_id,
        session_id: Some(session_id),
        claim_key: claim_key.clone(),
        claim_type: memory_type_str(memory_type).to_string(),
        subject: claim_subject(metadata, &claim_key),
        predicate: metadata_string(metadata, "rankingRole")
            .unwrap_or_else(|| memory_type_str(memory_type).to_string()),
        object: metadata_string(metadata, "claimObject").unwrap_or_else(|| claim_key.clone()),
        claim_polarity: metadata_string(metadata, "claimPolarity")
            .unwrap_or_else(|| "positive".to_string()),
        authority_class: metadata_string(metadata, "authorityClass")
            .unwrap_or_else(|| "session_derived".to_string()),
        verification_status: metadata_string(metadata, "verificationStatus")
            .unwrap_or_else(|| "unverified".to_string()),
        admitted: draft_is_belief_admitted(metadata),
        valid_from: None,
        valid_to: None,
        superseded_by: None,
    }
}

fn build_claim_proof_insert_params(
    claim_id: Uuid,
    memory_id: Uuid,
    metadata: &Value,
    records: &[SessionEventRecord],
    provenance_event_ids: &[Uuid],
) -> Vec<ClaimProofInsertParams> {
    let proof_type =
        metadata_string(metadata, "proofType").unwrap_or_else(|| "session_event".to_string());
    let authority_class = metadata_string(metadata, "authorityClass");
    let verification_status = metadata_string(metadata, "verificationStatus");
    if provenance_event_ids.is_empty() {
        return vec![ClaimProofInsertParams {
            claim_id,
            memory_id,
            session_id: None,
            session_event_id: None,
            proof_type,
            source_ref: metadata_string(metadata, "claimKey")
                .unwrap_or_else(|| "memory".to_string()),
            excerpt: None,
            authority_class,
            verification_status,
            proof_time: None,
        }];
    }

    provenance_event_ids
        .iter()
        .map(|event_id| {
            let event = records.iter().find(|candidate| candidate.id == *event_id);
            ClaimProofInsertParams {
                claim_id,
                memory_id,
                session_id: None,
                session_event_id: Some(*event_id),
                proof_type: proof_type.clone(),
                source_ref: format!("session_event:{event_id}"),
                excerpt: event.map(event_text).map(|text| truncate_text(&text, 500)),
                authority_class: authority_class.clone(),
                verification_status: verification_status.clone(),
                proof_time: event.map(|value| value.created_at.clone()),
            }
        })
        .collect()
}

fn claim_subject(metadata: &Value, claim_key: &str) -> String {
    if let Some(subject) = claim_key
        .split(':')
        .nth(1)
        .filter(|value| !value.is_empty())
    {
        return subject.to_string();
    }
    metadata_string(metadata, "sessionId").unwrap_or_else(|| "global".to_string())
}

fn build_related_map(edges: &[(Uuid, Uuid)]) -> HashMap<Uuid, Vec<Uuid>> {
    let mut map = HashMap::<Uuid, Vec<Uuid>>::new();
    for (left, right) in edges {
        map.entry(*left).or_default().push(*right);
        map.entry(*right).or_default().push(*left);
    }
    for values in map.values_mut() {
        values.sort_unstable();
        values.dedup();
    }
    map
}

fn map_dashboard_graph(
    graph: KnowledgeGraph,
    projection: GraphProjection,
) -> DashboardGraphResponse {
    DashboardGraphResponse {
        projection,
        nodes: graph
            .nodes
            .into_iter()
            .map(|node| GraphNode {
                id: node.id,
                label: node.label,
                node_type: node.node_type,
                summary: node
                    .metadata
                    .get("summary")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
            })
            .collect(),
        links: graph
            .edges
            .into_iter()
            .map(|edge| GraphLink {
                source: edge.source,
                target: edge.target,
                relation: edge.relation,
                weight: edge.weight,
            })
            .collect(),
    }
}

fn map_dashboard_graph_fallback(
    nodes: Vec<DashboardGraphNodeRow>,
    edges: Vec<DashboardGraphEdgeRow>,
) -> DashboardGraphResponse {
    DashboardGraphResponse {
        projection: GraphProjection {
            total_nodes: nodes.len(),
            total_edges: edges.len(),
            returned_nodes: nodes.len(),
            returned_edges: edges.len(),
        },
        nodes: nodes
            .into_iter()
            .map(|node| GraphNode {
                id: format!("memory:{}", node.id),
                label: node.title,
                node_type: node.memory_type,
                summary: node.summary,
            })
            .collect(),
        links: edges
            .into_iter()
            .map(|edge| GraphLink {
                source: format!("memory:{}", edge.source),
                target: format!("memory:{}", edge.target),
                relation: edge.edge_type,
                weight: edge.weight.unwrap_or(1.0),
            })
            .collect(),
    }
}

async fn load_latest_knowledge_graph_by_type(
    tx: &mut Transaction<'_, Postgres>,
    context: &RepositoryContext,
    snapshot_type: Option<&str>,
) -> Result<Option<KnowledgeGraph>, DomainError> {
    let row = sqlx::query(
        r#"
        select snapshot
        from public.knowledge_snapshots
        where organization_id = $1
          and team_id = $2
          and ($3::uuid is null or project_id = $3)
          and ($4::text is null or snapshot_type = $4)
        order by created_at desc
        limit 1
        "#,
    )
    .bind(context.organization_id)
    .bind(context.team_id)
    .bind(context.project_id)
    .bind(snapshot_type)
    .fetch_optional(&mut **tx)
    .await
    .map_err(DbError::from)?;
    let Some(row) = row else {
        return Ok(None);
    };
    let snapshot = row.try_get::<Value, _>("snapshot").map_err(DbError::from)?;
    serde_json::from_value(snapshot)
        .map(Some)
        .map_err(|error| DomainError::Internal(error.to_string()))
}

async fn load_merged_snapshots_by_type(
    tx: &mut Transaction<'_, Postgres>,
    context: &RepositoryContext,
    snapshot_type: &str,
) -> Result<Option<KnowledgeGraph>, DomainError> {
    let rows = sqlx::query(
        r#"
        select distinct on (project_id) snapshot
        from public.knowledge_snapshots
        where organization_id = $1
          and team_id = $2
          and snapshot_type = $3
        order by project_id, created_at desc
        "#,
    )
    .bind(context.organization_id)
    .bind(context.team_id)
    .bind(snapshot_type)
    .fetch_all(&mut **tx)
    .await
    .map_err(DbError::from)?;

    if rows.is_empty() {
        return Ok(None);
    }

    let mut merged: Option<KnowledgeGraph> = None;
    for row in rows {
        let snapshot = row.try_get::<Value, _>("snapshot").map_err(DbError::from)?;
        let graph: KnowledgeGraph = serde_json::from_value(snapshot)
            .map_err(|error| DomainError::Internal(error.to_string()))?;
        merged = Some(match merged {
            Some(base) => merge_graphs(&base, &graph),
            None => graph,
        });
    }
    Ok(merged)
}

async fn load_latest_snapshot_artifacts_by_type(
    tx: &mut Transaction<'_, Postgres>,
    context: &RepositoryContext,
    snapshot_type: Option<&str>,
) -> Result<Option<SnapshotArtifacts>, DomainError> {
    let row = sqlx::query(
        r#"
        select a.report_markdown, a.node_link_json,
               to_char(a.computed_at, 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') as computed_at
        from public.knowledge_snapshot_heads h
        join public.knowledge_snapshot_artifacts a on a.snapshot_id = h.snapshot_id
        where h.organization_id = $1
          and h.team_id = $2
          and ($3::uuid is null or h.project_id = $3)
          and ($4::text is null or h.snapshot_type = $4)
        order by h.updated_at desc
        limit 1
        "#,
    )
    .bind(context.organization_id)
    .bind(context.team_id)
    .bind(context.project_id)
    .bind(snapshot_type)
    .fetch_optional(&mut **tx)
    .await
    .map_err(DbError::from)?;
    row.map(|row| {
        Ok(SnapshotArtifacts {
            report_markdown: row.try_get("report_markdown").map_err(DbError::from)?,
            node_link_json: row.try_get("node_link_json").map_err(DbError::from)?,
            computed_at: row.try_get("computed_at").map_err(DbError::from)?,
        })
    })
    .transpose()
}

async fn persist_knowledge_snapshot_typed(
    tx: &mut Transaction<'_, Postgres>,
    context: &RepositoryContext,
    project_id: Uuid,
    graph: &KnowledgeGraph,
    snapshot_type: &str,
) -> Result<(), DomainError> {
    let snapshot_id = Uuid::new_v4();
    let snapshot =
        serde_json::to_value(graph).map_err(|error| DomainError::Internal(error.to_string()))?;

    sqlx::query(
        r#"
        insert into public.knowledge_snapshots (
          id, organization_id, team_id, project_id, snapshot, node_count, edge_count, community_count,
          snapshot_type
        ) values ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        "#,
    )
    .bind(snapshot_id)
    .bind(context.organization_id)
    .bind(context.team_id)
    .bind(project_id)
    .bind(snapshot)
    .bind(graph.statistics.node_count as i32)
    .bind(graph.statistics.edge_count as i32)
    .bind(graph.statistics.community_count as i32)
    .bind(snapshot_type)
    .execute(&mut **tx)
    .await
    .map_err(DbError::from)?;

    sqlx::query(
        r#"
        insert into public.knowledge_snapshot_heads (
          organization_id, team_id, project_id, snapshot_id, snapshot_type
        ) values ($1, $2, $3, $4, $5)
        on conflict (organization_id, team_id, project_id, snapshot_type) do update set
          snapshot_id = excluded.snapshot_id,
          updated_at = now()
        "#,
    )
    .bind(context.organization_id)
    .bind(context.team_id)
    .bind(project_id)
    .bind(snapshot_id)
    .bind(snapshot_type)
    .execute(&mut **tx)
    .await
    .map_err(DbError::from)?;

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
    .bind(context.organization_id)
    .bind(context.team_id)
    .bind(project_id)
    .bind(generate_knowledge_report(graph))
    .bind(to_node_link_json(graph))
    .bind(graph.statistics.node_count as i32)
    .bind(graph.statistics.edge_count as i32)
    .bind(graph.statistics.community_count as i32)
    .bind(snapshot_type)
    .execute(&mut **tx)
    .await
    .map_err(DbError::from)?;

    sqlx::query(
        r#"
        delete from public.knowledge_communities
        where organization_id = $1 and team_id = $2 and project_id = $3
        "#,
    )
    .bind(context.organization_id)
    .bind(context.team_id)
    .bind(project_id)
    .execute(&mut **tx)
    .await
    .map_err(DbError::from)?;

    for community in &graph.communities {
        sqlx::query(
            r#"
            insert into public.knowledge_communities (
              organization_id, team_id, project_id, community_id, label, cohesion_score,
              node_count, representative_nodes, bridge_nodes, level, community_path
            ) values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            "#,
        )
        .bind(context.organization_id)
        .bind(context.team_id)
        .bind(project_id)
        .bind(community.community_id as i32)
        .bind(&community.label)
        .bind(community.cohesion_score)
        .bind(community.node_count as i32)
        .bind(json!(community.representative_nodes))
        .bind(json!(community.bridge_nodes))
        .bind(community.level as i32)
        .bind(community.community_path.as_deref())
        .execute(&mut **tx)
        .await
        .map_err(DbError::from)?;
    }

    Ok(())
}

/// Matches the Node.js hash: `str.split('').reduce((h, c) => ((h << 5) - h + c.charCodeAt(0)) | 0, 0)`
/// which produces a 32-bit signed integer.
fn project_advisory_lock_key(project_id: Uuid) -> i64 {
    let s = project_id.to_string();
    let hash = s.chars().fold(0_i32, |h, c| {
        ((h << 5).wrapping_sub(h)).wrapping_add(c as i32)
    });
    hash as i64
}

fn knowledge_query_kind_str(kind: KnowledgeQueryKind) -> &'static str {
    match kind {
        KnowledgeQueryKind::HubNodes => "hub_nodes",
        KnowledgeQueryKind::ShortestPath => "shortest_path",
        KnowledgeQueryKind::Neighbors => "neighbors",
        KnowledgeQueryKind::Communities => "communities",
        KnowledgeQueryKind::Search => "search",
        KnowledgeQueryKind::GoalDirected => "goal_directed",
    }
}

async fn resolve_global_project_id(
    tx: &mut Transaction<'_, Postgres>,
    scope: &RepositoryContext,
) -> Option<Uuid> {
    sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM public.projects WHERE organization_id = $1 AND team_id = $2 AND slug = $3 LIMIT 1",
    )
    .bind(scope.organization_id)
    .bind(scope.team_id)
    .bind(GLOBAL_PROJECT_SLUG)
    .fetch_optional(&mut **tx)
    .await
    .ok()
    .flatten()
}

fn scoped_context(
    base: &RepositoryContext,
    project_id: Option<Uuid>,
) -> Result<RepositoryContext, DomainError> {
    if let Some(scoped_project) = base.project_id
        && let Some(requested) = project_id
        && scoped_project != requested
    {
        return Err(DomainError::BadRequest(format!(
            "Project {requested} is out of scope for this server configuration"
        )));
    }
    Ok(RepositoryContext {
        project_id: project_id.or(base.project_id),
        ..base.clone()
    })
}

fn provider_str(provider: Provider) -> &'static str {
    match provider {
        Provider::Claude => "claude",
        Provider::Codex => "codex",
        Provider::Gemini => "gemini",
    }
}

fn parse_provider(provider: &str) -> Provider {
    match provider {
        "claude" => Provider::Claude,
        "gemini" => Provider::Gemini,
        _ => Provider::Codex,
    }
}

fn canonical_event_type_str(event_type: chum_mem_contracts::CanonicalEventType) -> &'static str {
    match event_type {
        chum_mem_contracts::CanonicalEventType::Prompt => "prompt",
        chum_mem_contracts::CanonicalEventType::Response => "response",
        chum_mem_contracts::CanonicalEventType::ToolCall => "tool_call",
        chum_mem_contracts::CanonicalEventType::ToolResult => "tool_result",
        chum_mem_contracts::CanonicalEventType::FileChange => "file_change",
        chum_mem_contracts::CanonicalEventType::Command => "command",
        chum_mem_contracts::CanonicalEventType::TestResult => "test_result",
        chum_mem_contracts::CanonicalEventType::Summary => "summary",
        chum_mem_contracts::CanonicalEventType::Error => "error",
        chum_mem_contracts::CanonicalEventType::Annotation => "annotation",
        chum_mem_contracts::CanonicalEventType::Reasoning => "reasoning",
        chum_mem_contracts::CanonicalEventType::TurnContext => "turn_context",
        chum_mem_contracts::CanonicalEventType::AgentMessage => "agent_message",
    }
}

fn parse_canonical_event_type(value: &str) -> chum_mem_contracts::CanonicalEventType {
    match value {
        "prompt" => chum_mem_contracts::CanonicalEventType::Prompt,
        "response" => chum_mem_contracts::CanonicalEventType::Response,
        "tool_call" => chum_mem_contracts::CanonicalEventType::ToolCall,
        "tool_result" => chum_mem_contracts::CanonicalEventType::ToolResult,
        "file_change" => chum_mem_contracts::CanonicalEventType::FileChange,
        "command" => chum_mem_contracts::CanonicalEventType::Command,
        "test_result" => chum_mem_contracts::CanonicalEventType::TestResult,
        "summary" => chum_mem_contracts::CanonicalEventType::Summary,
        "error" => chum_mem_contracts::CanonicalEventType::Error,
        "reasoning" => chum_mem_contracts::CanonicalEventType::Reasoning,
        "turn_context" => chum_mem_contracts::CanonicalEventType::TurnContext,
        "agent_message" => chum_mem_contracts::CanonicalEventType::AgentMessage,
        _ => chum_mem_contracts::CanonicalEventType::Annotation,
    }
}

fn memory_type_str(memory_type: chum_mem_contracts::MemoryType) -> &'static str {
    match memory_type {
        chum_mem_contracts::MemoryType::Fact => "fact",
        chum_mem_contracts::MemoryType::Decision => "decision",
        chum_mem_contracts::MemoryType::Task => "task",
        chum_mem_contracts::MemoryType::Constraint => "constraint",
        chum_mem_contracts::MemoryType::Bug => "bug",
        chum_mem_contracts::MemoryType::Fix => "fix",
        chum_mem_contracts::MemoryType::OpenQuestion => "open_question",
        chum_mem_contracts::MemoryType::Summary => "summary",
        chum_mem_contracts::MemoryType::ImplementationDetail => "implementation_detail",
        chum_mem_contracts::MemoryType::ChangeLog => "change_log",
        chum_mem_contracts::MemoryType::Risk => "risk",
    }
}

fn parse_memory_type(value: &str) -> chum_mem_contracts::MemoryType {
    match value {
        "fact" => chum_mem_contracts::MemoryType::Fact,
        "decision" => chum_mem_contracts::MemoryType::Decision,
        "task" => chum_mem_contracts::MemoryType::Task,
        "constraint" => chum_mem_contracts::MemoryType::Constraint,
        "bug" => chum_mem_contracts::MemoryType::Bug,
        "fix" => chum_mem_contracts::MemoryType::Fix,
        "open_question" => chum_mem_contracts::MemoryType::OpenQuestion,
        "implementation_detail" => chum_mem_contracts::MemoryType::ImplementationDetail,
        "change_log" => chum_mem_contracts::MemoryType::ChangeLog,
        "risk" => chum_mem_contracts::MemoryType::Risk,
        _ => chum_mem_contracts::MemoryType::Summary,
    }
}

fn metadata_string(metadata: &Value, key: &str) -> Option<String> {
    metadata
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
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

fn parse_authority_class(value: &str) -> Option<AuthorityClass> {
    match value {
        "repository" => Some(AuthorityClass::Repository),
        "user_confirmed" => Some(AuthorityClass::UserConfirmed),
        "tool_verified" => Some(AuthorityClass::ToolVerified),
        "test_verified" => Some(AuthorityClass::TestVerified),
        "session_derived" => Some(AuthorityClass::SessionDerived),
        "model_derived" => Some(AuthorityClass::ModelDerived),
        _ => None,
    }
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

fn parse_verification_status(value: &str) -> Option<VerificationStatus> {
    match value {
        "verified" => Some(VerificationStatus::Verified),
        "user_confirmed" => Some(VerificationStatus::UserConfirmed),
        "inferred" => Some(VerificationStatus::Inferred),
        "contradicted" => Some(VerificationStatus::Contradicted),
        "unverified" => Some(VerificationStatus::Unverified),
        _ => None,
    }
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

fn parse_proof_type(value: &str) -> Option<ProofType> {
    match value {
        "repository" => Some(ProofType::Repository),
        "session_event" => Some(ProofType::SessionEvent),
        "tool_result" => Some(ProofType::ToolResult),
        "test_result" => Some(ProofType::TestResult),
        "user_confirmation" => Some(ProofType::UserConfirmation),
        "summary" => Some(ProofType::Summary),
        _ => None,
    }
}

fn parse_claim_relation_type(value: &str) -> Option<ClaimRelationType> {
    match value {
        "supersedes" => Some(ClaimRelationType::Supersedes),
        "contradicts" => Some(ClaimRelationType::Contradicts),
        "confirms" => Some(ClaimRelationType::Confirms),
        "depends_on" => Some(ClaimRelationType::DependsOn),
        "derived_from" => Some(ClaimRelationType::DerivedFrom),
        _ => None,
    }
}

fn build_proof_handles(
    metadata: &Value,
    provenance: &[chum_mem_contracts::ProvenanceHandle],
) -> Vec<ProofHandle> {
    let proof_type = metadata_proof_type(metadata).unwrap_or(ProofType::SessionEvent);
    let authority_class = metadata_authority_class(metadata);
    let verification_status = metadata_verification_status(metadata);
    if provenance.is_empty() {
        return vec![ProofHandle {
            proof_type,
            source_ref: metadata_string(metadata, "claimKey")
                .or_else(|| metadata_string(metadata, "derivation"))
                .unwrap_or_else(|| "memory".to_string()),
            excerpt: None,
            session_id: None,
            session_event_id: None,
            authority_class,
            verification_status,
        }];
    }
    provenance
        .iter()
        .map(|handle| ProofHandle {
            proof_type,
            source_ref: format!("session_event:{}", handle.session_event_id),
            excerpt: handle.excerpt.clone(),
            session_id: Some(handle.session_id),
            session_event_id: Some(handle.session_event_id),
            authority_class,
            verification_status,
        })
        .collect()
}

fn build_proof_handles_from_ranked_memory(hit: &RankedMemory) -> Vec<ProofHandle> {
    let metadata = json!({
        "claimKey": hit.claim_key,
        "proofType": hit.proof_type.map(|value| serde_json::to_value(value).ok()).flatten(),
        "authorityClass": hit.authority_class.map(|value| serde_json::to_value(value).ok()).flatten(),
        "verificationStatus": hit.verification_status.map(|value| serde_json::to_value(value).ok()).flatten(),
    });
    build_proof_handles(&metadata, &hit.provenance)
}

fn sanitize_json_value(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::String(text) => serde_json::Value::String(text.replace('\0', "")),
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.into_iter().map(sanitize_json_value).collect())
        }
        serde_json::Value::Object(map) => serde_json::Value::Object(
            map.into_iter()
                .map(|(key, value)| (key, sanitize_json_value(value)))
                .collect(),
        ),
        other => other,
    }
}

fn truncate(value: &str, max: usize) -> String {
    if value.chars().count() <= max {
        return value.to_string();
    }
    if max <= 3 {
        return value.chars().take(max).collect();
    }
    let mut truncated: String = value.chars().take(max - 3).collect();
    truncated.push_str("...");
    truncated
}

fn estimate_tokens(text: &str) -> u32 {
    ((text.len() as u32) / 4).max(1)
}

fn to_pgvector_literal(vector: &[f64]) -> String {
    assert_eq!(vector.len(), CHROMA_EMBEDDING_DIMENSIONS);
    format!(
        "[{}]",
        vector
            .iter()
            .map(|value| value.to_string())
            .collect::<Vec<_>>()
            .join(",")
    )
}

fn format_time(value: OffsetDateTime) -> String {
    value
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

fn map_domain_error(error: DomainError) -> ApiError {
    match error {
        DomainError::BadRequest(message) => ApiError::bad_request(message),
        DomainError::NotFound(message) => ApiError::not_found(message),
        DomainError::Db(DbError::NotFound(_)) => ApiError::not_found("Resource not found"),
        DomainError::Db(error) => ApiError::internal(error.to_string()),
        DomainError::Internal(message) => ApiError::internal(message),
    }
}

#[derive(Debug)]
enum DomainError {
    BadRequest(String),
    NotFound(String),
    Db(DbError),
    Internal(String),
}

impl From<DbError> for DomainError {
    fn from(value: DbError) -> Self {
        Self::Db(value)
    }
}

#[derive(Debug)]
struct DerivedPersistenceResult {
    derived_memories: i32,
    derived_episodes: i32,
    derived_session_edges: i32,
    unresolved_risk: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;
    use axum::http::HeaderValue;
    use axum::http::Request;
    use chum_mem_contracts::{
        AppendSessionEventRequest, CanonicalEventType, ClaimRelationType, ContextBuildRequest,
        DisclosureLevel, EndSessionRequest, MemorySearchRequest, Provider, RetrievalIntent,
        SearchMode, SessionEventPayload, StartSessionRequest,
    };
    use serde_json::json;
    use std::env;
    use std::sync::Arc;
    use tower::ServiceExt;
    use uuid::Uuid;

    fn test_state() -> ApiState {
        let values = std::collections::HashMap::from([
            (
                "DATABASE_URL".to_string(),
                "postgres://chum_mem:chum_mem@postgres:65432/chum_mem".to_string(),
            ),
            (
                "CHUM_MEM_ORGANIZATION_ID".to_string(),
                "00000000-0000-0000-0000-000000000001".to_string(),
            ),
            (
                "CHUM_MEM_TEAM_ID".to_string(),
                "00000000-0000-0000-0000-000000000002".to_string(),
            ),
        ]);
        let config = Arc::new(AppConfig::from_map(&values).expect("test config should parse"));
        let db = Database::connect_lazy(config.as_ref()).expect("lazy pool should parse");

        ApiState {
            config: Arc::clone(&config),
            db,
            scope: RepositoryContext::from_config(config.as_ref()),
            metadata: ServiceMetadata {
                name: "chum-mem-api",
                version: "test",
                role: "api",
            },
            started_at: OffsetDateTime::now_utc(),
            http_client: Client::new(),
            mcp_sessions: Arc::new(RwLock::new(HashSet::new())),
            community_cache: Arc::new(RwLock::new(CommunityCache::default())),
        }
    }

    #[tokio::test]
    async fn health_endpoint_reports_ok() {
        let response = router(test_state())
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .expect("health request should succeed");

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn ready_endpoint_reports_unavailable_without_database() {
        let response = router(test_state())
            .oneshot(
                Request::builder()
                    .uri("/ready")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .expect("ready request should succeed");

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn search_endpoint_rejects_blank_query() {
        let response = router(test_state())
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/search")
                    .header("content-type", HeaderValue::from_static("application/json"))
                    .body(axum::body::Body::from(
                        serde_json::json!({
                            "query": "   ",
                            "limit": 5,
                            "mode": "hybrid",
                            "disclosureLevel": "overview"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .expect("search request should succeed");

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body should read");
        let body = String::from_utf8(body.to_vec()).expect("body should be utf8");
        assert!(body.contains("must not be blank"));
    }

    async fn integration_state(project_id: Uuid) -> Option<ApiState> {
        let database_url = env::var("CHUM_MEM_INTEGRATION_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://chum_mem:chum_mem@127.0.0.1:65432/chum_mem".to_string());
        let values = std::collections::HashMap::from([
            ("DATABASE_URL".to_string(), database_url),
            (
                "CHUM_MEM_ORGANIZATION_ID".to_string(),
                "00000000-0000-0000-0000-000000000001".to_string(),
            ),
            (
                "CHUM_MEM_TEAM_ID".to_string(),
                "00000000-0000-0000-0000-000000000002".to_string(),
            ),
            ("CHUM_MEM_PROJECT_ID".to_string(), project_id.to_string()),
            (
                "CHUM_MEM_USER_ID".to_string(),
                "00000000-0000-0000-0000-000000000004".to_string(),
            ),
            ("CHUM_MEM_ACTOR_TYPE".to_string(), "system".to_string()),
            ("CHUM_MEM_TEAM_ROLE".to_string(), "admin".to_string()),
        ]);
        let config = Arc::new(match AppConfig::from_map(&values) {
            Ok(config) => config,
            Err(error) => {
                eprintln!("skipping integration test: config parse failed: {error}");
                return None;
            }
        });
        let db = match Database::connect(config.as_ref()).await {
            Ok(db) => db,
            Err(error) => {
                eprintln!("skipping integration test: database unavailable: {error}");
                return None;
            }
        };
        if let Err(error) = db.migrate_if_enabled(config.as_ref()).await {
            eprintln!("skipping integration test: migration failed: {error}");
            return None;
        }

        Some(ApiState {
            config: Arc::clone(&config),
            db,
            scope: RepositoryContext::from_config(config.as_ref()),
            metadata: ServiceMetadata {
                name: "chum-mem-api",
                version: "test",
                role: "api",
            },
            started_at: OffsetDateTime::now_utc(),
            http_client: Client::new(),
            mcp_sessions: Arc::new(RwLock::new(HashSet::new())),
            community_cache: Arc::new(RwLock::new(CommunityCache::default())),
        })
    }

    async fn ingest_session(
        state: &ApiState,
        external_session_id: &str,
        file_path: &str,
        events: &[(&str, CanonicalEventType)],
    ) -> Uuid {
        let started = perform_session_start(
            state,
            StartSessionRequest {
                provider: Provider::Codex,
                project_id: state
                    .scope
                    .project_id
                    .expect("integration state should be project scoped"),
                external_session_id: external_session_id.to_string(),
                repo: Default::default(),
                local: Default::default(),
                metadata: json!({}),
            },
        )
        .await
        .expect("session should start");

        for (index, (message, event_type)) in events.iter().enumerate() {
            perform_session_event(
                state,
                AppendSessionEventRequest {
                    session_id: started.session_id,
                    event_id: format!("{external_session_id}-event-{index}"),
                    idempotency_key: format!("{external_session_id}-key-{index}"),
                    provider: Provider::Codex,
                    event_type: *event_type,
                    event_time: format!("2026-04-14T00:00:{:02}Z", index),
                    payload: SessionEventPayload {
                        message: Some((*message).to_string()),
                        tool_name: None,
                        command: None,
                        exit_code: None,
                        file_path: Some(file_path.to_string()),
                        diff_stat: None,
                        metadata: json!({}),
                    },
                    raw_payload: json!({ "message": message, "filePath": file_path }),
                    turn_id: None,
                },
            )
            .await
            .expect("session event should append");
        }

        perform_session_end(
            state,
            EndSessionRequest {
                session_id: started.session_id,
                summary: None,
                metadata: json!({}),
                defer: None,
            },
        )
        .await
        .expect("session should end");

        // v2.2.1 ingestion-choke fix: reconciliation is now async. Drive any
        // pending `reconcile-claim-state` jobs inline so the integration test
        // assertions (supersedes / contradicts / active_conflict_count) still
        // observe a converged state. This mirrors what the worker does.
        run_pending_reconciliation(state).await;

        started.session_id
    }

    async fn run_pending_reconciliation(state: &ApiState) {
        use chum_mem_db::reconcile::reconcile_claim_state_for_claims;
        let pool = state.db.pool();
        loop {
            let mut tx = pool.begin().await.expect("tx should begin");
            apply_repository_context(&mut *tx, &state.scope)
                .await
                .expect("apply repo context");
            let row = sqlx::query(
                r#"
                update public.worker_jobs
                set status = 'completed', completed_at = now(), updated_at = now()
                where id = (
                  select id from public.worker_jobs
                  where job_type = 'reconcile-claim-state'
                    and status = 'pending'
                    and organization_id = $1
                    and team_id = $2
                    and (cast($3 as uuid) is null or project_id = $3)
                  order by created_at asc
                  for update skip locked
                  limit 1
                )
                returning project_id, payload
                "#,
            )
            .bind(state.scope.organization_id)
            .bind(state.scope.team_id)
            .bind(state.scope.project_id)
            .fetch_optional(&mut *tx)
            .await
            .expect("claim reconcile job");
            let Some(row) = row else {
                tx.commit().await.ok();
                break;
            };
            let project_id: Uuid = row.try_get("project_id").expect("project_id");
            let payload: serde_json::Value = row.try_get("payload").expect("payload");
            let claim_ids: Vec<Uuid> = payload
                .get("newClaimIds")
                .and_then(|value| value.as_array())
                .map(|array| {
                    array
                        .iter()
                        .filter_map(|value| {
                            value.as_str().and_then(|s| Uuid::parse_str(s).ok())
                        })
                        .collect()
                })
                .unwrap_or_default();
            let scoped = RepositoryContext {
                project_id: Some(project_id),
                ..state.scope.clone()
            };
            reconcile_claim_state_for_claims(&mut tx, &scoped, project_id, &claim_ids)
                .await
                .expect("reconcile chunk should succeed");
            tx.commit().await.expect("reconcile tx commit");
        }
    }

    #[tokio::test]
    async fn integration_history_flag_controls_superseded_claim_visibility() {
        let Some(state) = integration_state(Uuid::new_v4()).await else {
            return;
        };

        ingest_session(
            &state,
            "pckc-history-1",
            "src/sync.rs",
            &[(
                "Decision: use legacy polling for status sync",
                CanonicalEventType::Prompt,
            )],
        )
        .await;
        ingest_session(
            &state,
            "pckc-history-2",
            "src/sync.rs",
            &[(
                "Constraint: do not use legacy polling for status sync",
                CanonicalEventType::Prompt,
            )],
        )
        .await;

        let default_hits = perform_search(
            &state,
            MemorySearchRequest {
                query: "legacy polling status sync".to_string(),
                project_id: state.scope.project_id,
                session_id: None,
                provider: None,
                repo_url: None,
                branch: None,
                types: Vec::new(),
                tags: Vec::new(),
                from: None,
                to: None,
                mode: SearchMode::Lexical,
                disclosure_level: DisclosureLevel::Overview,
                retrieval_intent: Some(RetrievalIntent::MemoryOnly),
                include_historical: Some(false),
                limit: 8,
                cursor: None,
            },
        )
        .await
        .expect("default search should succeed")
        .hits;

        assert!(
            default_hits
                .iter()
                .any(|hit| hit.memory_type == MemoryType::Constraint)
        );
        assert!(
            !default_hits
                .iter()
                .any(|hit| hit.memory_type == MemoryType::Decision)
        );

        let historical_hits = perform_search(
            &state,
            MemorySearchRequest {
                query: "legacy polling status sync".to_string(),
                project_id: state.scope.project_id,
                session_id: None,
                provider: None,
                repo_url: None,
                branch: None,
                types: Vec::new(),
                tags: Vec::new(),
                from: None,
                to: None,
                mode: SearchMode::Lexical,
                disclosure_level: DisclosureLevel::Overview,
                retrieval_intent: Some(RetrievalIntent::MemoryOnly),
                include_historical: Some(true),
                limit: 8,
                cursor: None,
            },
        )
        .await
        .expect("historical search should succeed")
        .hits;

        let superseded_decision = historical_hits
            .iter()
            .find(|hit| hit.memory_type == MemoryType::Decision)
            .expect("historical search should include the superseded decision");
        assert!(superseded_decision.superseded_by.is_some());
        assert!(
            historical_hits
                .iter()
                .any(|hit| hit.memory_type == MemoryType::Constraint)
        );
    }

    #[tokio::test]
    async fn integration_contradictions_surface_in_memory_get_and_context_build() {
        let Some(state) = integration_state(Uuid::new_v4()).await else {
            return;
        };

        ingest_session(
            &state,
            "pckc-conflict-1",
            "src/cache.rs",
            &[(
                "Decision: checkout cache is enabled for requests",
                CanonicalEventType::Prompt,
            )],
        )
        .await;
        ingest_session(
            &state,
            "pckc-conflict-2",
            "src/cache.rs",
            &[(
                "Verified current truth: checkout cache is disabled for requests",
                CanonicalEventType::Prompt,
            )],
        )
        .await;

        let hits = perform_search(
            &state,
            MemorySearchRequest {
                query: "checkout cache requests".to_string(),
                project_id: state.scope.project_id,
                session_id: None,
                provider: None,
                repo_url: None,
                branch: None,
                types: Vec::new(),
                tags: Vec::new(),
                from: None,
                to: None,
                mode: SearchMode::Lexical,
                disclosure_level: DisclosureLevel::Overview,
                retrieval_intent: Some(RetrievalIntent::MemoryOnly),
                include_historical: Some(false),
                limit: 8,
                cursor: None,
            },
        )
        .await
        .expect("conflict search should succeed")
        .hits;

        assert!(
            hits.iter().any(|hit| hit.active_conflict_count > 0),
            "expected at least one active conflict hit"
        );

        let target = hits
            .iter()
            .find(|hit| hit.active_conflict_count > 0)
            .expect("search should yield a conflict hit");
        let memory = perform_memory_get(&state, target.id)
            .await
            .expect("memory_get should succeed");
        assert!(
            memory
                .claim_relations
                .iter()
                .any(|relation| relation.relation_type == ClaimRelationType::Contradicts)
        );
        assert!(!memory.proof_handles.is_empty());

        let context = perform_context_build(
            &state,
            ContextBuildRequest {
                provider: Provider::Codex,
                objective: "What is the current checkout cache setting?".to_string(),
                retrieval_intent: Some(RetrievalIntent::MemoryOnly),
                include_historical: Some(false),
                project_id: state.scope.project_id,
                repo_url: None,
                branch: None,
                file_paths: Vec::new(),
                max_token_budget: 600,
            },
        )
        .await
        .expect("context_build should succeed");

        assert!(!context.context_pack.conflicts.is_empty());
        assert!(
            context
                .context_pack
                .unknowns
                .iter()
                .any(|value| value.contains("Conflicting claims"))
        );
        assert!(!context.context_pack.recommended_verification.is_empty());
        assert!(!context.context_pack.proof_handles.is_empty());
    }

    // v2.2.1: `claim_supersession_candidates_sort_by_memory_then_claim_id` and
    // `claim_reconciliation_scope_keys_are_sorted_and_deduplicated` removed
    // along with the per-draft advisory-lock fan-out they exercised. The
    // replacement policy (single per-session lock + async reconcile job) is
    // covered by `integration_history_flag_controls_superseded_claim_visibility`
    // and `integration_contradictions_surface_in_memory_get_and_context_build`
    // which now drain the queue via `run_pending_reconciliation`.
}
