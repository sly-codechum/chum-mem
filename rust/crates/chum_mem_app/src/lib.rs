//! Shared application helpers for the Rust API and worker binaries.

use chum_mem_config::AppConfig;
use chum_mem_contracts::ActorType;
use serde::Serialize;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use tracing_subscriber::EnvFilter;

#[derive(Debug, Clone, Copy)]
pub struct ServiceMetadata {
    pub name: &'static str,
    pub version: &'static str,
    pub role: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthResponse {
    pub service: &'static str,
    pub version: &'static str,
    pub role: &'static str,
    pub status: &'static str,
    pub started_at: String,
    pub uptime_seconds: i64,
    pub config: HealthConfigSummary,
    pub routes: Vec<&'static str>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthConfigSummary {
    pub database_url_present: bool,
    pub chroma_enabled: bool,
    pub project_scoped: bool,
    pub actor_type: ActorType,
    pub worker_poll_interval_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorBody {
    pub error: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DependencyStatus {
    pub name: &'static str,
    pub healthy: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadyResponse {
    pub service: &'static str,
    pub version: &'static str,
    pub role: &'static str,
    pub status: &'static str,
    #[serde(default)]
    pub dependencies: Vec<DependencyStatus>,
}

pub fn init_tracing(service_name: &'static str) {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new(format!("{service_name}=info,tower_http=info,axum=info"))
    });

    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .compact()
        .try_init();
}

pub fn build_health_response(
    metadata: ServiceMetadata,
    config: &AppConfig,
    started_at: OffsetDateTime,
    routes: &[&'static str],
) -> HealthResponse {
    let now = OffsetDateTime::now_utc();
    let uptime_seconds = (now - started_at).whole_seconds().max(0);

    HealthResponse {
        service: metadata.name,
        version: metadata.version,
        role: metadata.role,
        status: "ok",
        started_at: started_at
            .format(&Rfc3339)
            .expect("RFC3339 formatting should succeed"),
        uptime_seconds,
        config: HealthConfigSummary {
            database_url_present: !config.database_url.is_empty(),
            chroma_enabled: config.chroma_enabled(),
            project_scoped: config.project_id.is_some(),
            actor_type: config.actor_type,
            worker_poll_interval_ms: config.worker_poll_interval_ms,
        },
        routes: routes.to_vec(),
    }
}

pub fn not_yet_migrated(endpoint: &'static str) -> ErrorBody {
    ErrorBody {
        error: format!("Rust migration scaffold: `{endpoint}` is not implemented yet"),
    }
}

pub async fn shutdown_signal(service_name: &'static str) {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};

        let mut terminate =
            signal(SignalKind::terminate()).expect("installing SIGTERM handler should succeed");

        let initial_term =
            tokio::time::timeout(std::time::Duration::from_millis(50), terminate.recv())
                .await
                .ok()
                .flatten();
        if initial_term.is_some() {
            tracing::warn!(
                service = service_name,
                "ignoring immediate startup SIGTERM on Unix runtime"
            );
        }

        loop {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => break,
                maybe = terminate.recv() => {
                    if maybe.is_some() {
                        break;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(250)).await;
                }
            }
        }
    }

    #[cfg(not(unix))]
    {
        tokio::signal::ctrl_c()
            .await
            .expect("installing CTRL+C handler should succeed");
    }

    tracing::info!(service = service_name, "shutdown signal received");
}
