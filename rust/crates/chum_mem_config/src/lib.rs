//! Typed environment configuration for the Rust migration.
//!
//! This crate preserves the existing `.env.example` and Docker Compose variable
//! surface while enforcing explicit parsing and startup validation.

use std::collections::HashMap;
use std::env;
use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::str::FromStr;
use std::time::Duration;

use chum_mem_contracts::{ActorType, TeamRole};
use thiserror::Error;
use url::Url;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceKind {
    Api,
    Worker,
    Web,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VectorStoreBackend {
    Chroma,
    TurboVec,
}

impl FromStr for VectorStoreBackend {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "chroma" | "chromadb" => Ok(Self::Chroma),
            "turbovec" | "turbo_vec" | "turbo-vec" => Ok(Self::TurboVec),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub database_url: String,
    pub db_min_connections: u32,
    pub db_max_connections: u32,
    pub db_connect_timeout_secs: u64,
    pub db_acquire_timeout_secs: u64,
    pub run_db_migrations: bool,
    pub readiness_timeout_ms: u64,
    pub mcp_host: String,
    pub mcp_port: u16,
    pub web_port: u16,
    pub dashboard_api_url: Url,
    pub vector_store_backend: VectorStoreBackend,
    pub chroma_url: Option<Url>,
    pub chroma_collection: String,
    pub turbovec_path: Option<PathBuf>,
    pub turbovec_bit_width: usize,
    pub organization_id: Uuid,
    pub team_id: Uuid,
    pub project_id: Option<Uuid>,
    pub user_id: Option<Uuid>,
    pub actor_type: ActorType,
    pub team_role: TeamRole,
    pub worker_poll_interval_ms: u64,
    pub worker_concurrency: usize,
    pub knowledge_graph_max_cluster_nodes: u32,
    pub knowledge_graph_max_cluster_edges: u32,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ConfigError {
    #[error("missing required environment variable `{0}`")]
    Missing(&'static str),
    #[error("invalid environment variable `{key}`: {value}")]
    Invalid { key: &'static str, value: String },
}

impl AppConfig {
    pub fn from_env() -> Result<Self, ConfigError> {
        let values: HashMap<String, String> = env::vars().collect();
        Self::from_map(&values)
    }

    pub fn from_map(values: &HashMap<String, String>) -> Result<Self, ConfigError> {
        let vector_store_backend =
            parse_or_default(values, "VECTOR_STORE_BACKEND", VectorStoreBackend::Chroma)?;
        let turbovec_path = optional(values, "TURBOVEC_PATH").map(PathBuf::from);
        let turbovec_bit_width = if vector_store_backend == VectorStoreBackend::TurboVec {
            let bit_width = parse_or_default(values, "TURBOVEC_BIT_WIDTH", 4usize)?;
            if !(2..=4).contains(&bit_width) {
                return Err(ConfigError::Invalid {
                    key: "TURBOVEC_BIT_WIDTH",
                    value: bit_width.to_string(),
                });
            }
            if turbovec_path.is_none() {
                return Err(ConfigError::Missing("TURBOVEC_PATH"));
            }
            bit_width
        } else {
            4
        };

        Ok(Self {
            database_url: required(values, "DATABASE_URL")?,
            db_min_connections: parse_or_default(values, "DB_MIN_CONNECTIONS", 1u32)?,
            db_max_connections: parse_or_default(values, "DB_MAX_CONNECTIONS", 25u32)?,
            db_connect_timeout_secs: parse_or_default(values, "DB_CONNECT_TIMEOUT_SECS", 15u64)?,
            db_acquire_timeout_secs: parse_or_default(values, "DB_ACQUIRE_TIMEOUT_SECS", 15u64)?,
            run_db_migrations: parse_or_default(values, "RUN_DB_MIGRATIONS", true)?,
            readiness_timeout_ms: parse_or_default(values, "READINESS_TIMEOUT_MS", 5_000u64)?,
            mcp_host: optional(values, "MCP_HOST").unwrap_or_else(|| "0.0.0.0".to_string()),
            mcp_port: parse_or_default(values, "MCP_PORT", 65301u16)?,
            web_port: parse_or_default(values, "WEB_PORT", 65300u16)?,
            dashboard_api_url: parse_or_default(
                values,
                "DASHBOARD_API_URL",
                Url::parse("http://localhost:65301").expect("static dashboard URL is valid"),
            )?,
            vector_store_backend,
            chroma_url: optional_parse(values, "CHROMA_URL")?,
            chroma_collection: optional(values, "CHROMA_COLLECTION")
                .unwrap_or_else(|| "memories".to_string()),
            turbovec_path,
            turbovec_bit_width,
            organization_id: parse_required(values, "CHUM_MEM_ORGANIZATION_ID")?,
            team_id: parse_required(values, "CHUM_MEM_TEAM_ID")?,
            project_id: optional_parse(values, "CHUM_MEM_PROJECT_ID")?,
            user_id: optional_parse(values, "CHUM_MEM_USER_ID")?,
            actor_type: parse_or_default(values, "CHUM_MEM_ACTOR_TYPE", ActorType::System)?,
            team_role: parse_or_default(values, "CHUM_MEM_TEAM_ROLE", TeamRole::Admin)?,
            worker_poll_interval_ms: parse_or_default(values, "WORKER_POLL_INTERVAL_MS", 5_000u64)?,
            worker_concurrency: parse_or_default(values, "WORKER_CONCURRENCY", 4usize)?,
            knowledge_graph_max_cluster_nodes: parse_or_default(
                values,
                "KNOWLEDGE_GRAPH_MAX_CLUSTER_NODES",
                100_000u32,
            )?,
            knowledge_graph_max_cluster_edges: parse_or_default(
                values,
                "KNOWLEDGE_GRAPH_MAX_CLUSTER_EDGES",
                200_000u32,
            )?,
        })
    }

    pub fn bind_address(&self, kind: ServiceKind) -> Result<SocketAddr, ConfigError> {
        let ip = IpAddr::from_str(&self.mcp_host).map_err(|_| ConfigError::Invalid {
            key: "MCP_HOST",
            value: self.mcp_host.clone(),
        })?;

        let port = match kind {
            ServiceKind::Api => self.mcp_port,
            ServiceKind::Web => self.web_port,
            ServiceKind::Worker => 0,
        };

        Ok(SocketAddr::new(ip, port))
    }

    pub fn worker_poll_interval(&self) -> Duration {
        Duration::from_millis(self.worker_poll_interval_ms)
    }

    pub fn db_connect_timeout(&self) -> Duration {
        Duration::from_secs(self.db_connect_timeout_secs)
    }

    pub fn db_acquire_timeout(&self) -> Duration {
        Duration::from_secs(self.db_acquire_timeout_secs)
    }

    pub fn readiness_timeout(&self) -> Duration {
        Duration::from_millis(self.readiness_timeout_ms)
    }

    pub fn chroma_enabled(&self) -> bool {
        self.vector_store_backend == VectorStoreBackend::Chroma && self.chroma_url.is_some()
    }

    pub fn turbovec_enabled(&self) -> bool {
        self.vector_store_backend == VectorStoreBackend::TurboVec && self.turbovec_path.is_some()
    }

    pub fn vector_store_enabled(&self) -> bool {
        self.chroma_enabled() || self.turbovec_enabled()
    }
}

fn required(values: &HashMap<String, String>, key: &'static str) -> Result<String, ConfigError> {
    values
        .get(key)
        .cloned()
        .filter(|value| !value.trim().is_empty())
        .ok_or(ConfigError::Missing(key))
}

fn optional(values: &HashMap<String, String>, key: &'static str) -> Option<String> {
    values
        .get(key)
        .cloned()
        .filter(|value| !value.trim().is_empty())
}

fn parse_required<T>(values: &HashMap<String, String>, key: &'static str) -> Result<T, ConfigError>
where
    T: FromStr,
{
    let raw = required(values, key)?;
    raw.parse::<T>()
        .map_err(|_| ConfigError::Invalid { key, value: raw })
}

fn optional_parse<T>(
    values: &HashMap<String, String>,
    key: &'static str,
) -> Result<Option<T>, ConfigError>
where
    T: FromStr,
{
    match optional(values, key) {
        Some(raw) => raw
            .parse::<T>()
            .map(Some)
            .map_err(|_| ConfigError::Invalid { key, value: raw }),
        None => Ok(None),
    }
}

fn parse_or_default<T>(
    values: &HashMap<String, String>,
    key: &'static str,
    default: T,
) -> Result<T, ConfigError>
where
    T: FromStr,
{
    match optional(values, key) {
        Some(raw) => raw
            .parse::<T>()
            .map_err(|_| ConfigError::Invalid { key, value: raw }),
        None => Ok(default),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> HashMap<String, String> {
        HashMap::from([
            (
                "DATABASE_URL".to_string(),
                "postgres://chum_mem:chum_mem@postgres:65432/chum_mem".to_string(),
            ),
            (
                "CHUM_MEM_ORGANIZATION_ID".to_string(),
                Uuid::nil().to_string(),
            ),
            (
                "CHUM_MEM_TEAM_ID".to_string(),
                "00000000-0000-0000-0000-000000000002".to_string(),
            ),
        ])
    }

    #[test]
    fn loads_defaults_from_minimal_map() {
        let config = AppConfig::from_map(&fixture()).expect("config should parse");
        assert_eq!(config.mcp_port, 65301);
        assert_eq!(config.web_port, 65300);
        assert_eq!(config.worker_poll_interval_ms, 5_000);
        assert!(config.run_db_migrations);
        assert_eq!(config.vector_store_backend, VectorStoreBackend::Chroma);
        assert!(!config.chroma_enabled());
        assert!(!config.vector_store_enabled());
    }

    #[test]
    fn enables_turbovec_backend_when_configured() {
        let mut values = fixture();
        values.insert("VECTOR_STORE_BACKEND".to_string(), "turbovec".to_string());
        values.insert(
            "TURBOVEC_PATH".to_string(),
            "/tmp/chum-turbovec".to_string(),
        );

        let config = AppConfig::from_map(&values).expect("config should parse");
        assert_eq!(config.vector_store_backend, VectorStoreBackend::TurboVec);
        assert!(!config.chroma_enabled());
        assert!(config.turbovec_enabled());
        assert!(config.vector_store_enabled());
    }

    #[test]
    fn rejects_unknown_vector_store_backend() {
        let mut values = fixture();
        values.insert("VECTOR_STORE_BACKEND".to_string(), "unknown".to_string());

        let error = AppConfig::from_map(&values).expect_err("unknown backend should fail");
        assert_eq!(
            error,
            ConfigError::Invalid {
                key: "VECTOR_STORE_BACKEND",
                value: "unknown".to_string(),
            }
        );
    }

    #[test]
    fn requires_turbovec_path_only_for_turbovec_backend() {
        let mut values = fixture();
        values.insert("VECTOR_STORE_BACKEND".to_string(), "turbovec".to_string());

        let error = AppConfig::from_map(&values).expect_err("missing TurboVec path should fail");
        assert_eq!(error, ConfigError::Missing("TURBOVEC_PATH"));

        let mut chroma_values = fixture();
        chroma_values.insert("VECTOR_STORE_BACKEND".to_string(), "chroma".to_string());
        let config = AppConfig::from_map(&chroma_values).expect("Chroma should not need TurboVec");
        assert_eq!(config.vector_store_backend, VectorStoreBackend::Chroma);
    }

    #[test]
    fn rejects_invalid_turbovec_bit_width() {
        let mut values = fixture();
        values.insert("VECTOR_STORE_BACKEND".to_string(), "turbovec".to_string());
        values.insert(
            "TURBOVEC_PATH".to_string(),
            "/tmp/chum-turbovec".to_string(),
        );
        values.insert("TURBOVEC_BIT_WIDTH".to_string(), "8".to_string());

        let error = AppConfig::from_map(&values).expect_err("invalid bit width should fail");
        assert_eq!(
            error,
            ConfigError::Invalid {
                key: "TURBOVEC_BIT_WIDTH",
                value: "8".to_string(),
            }
        );
    }

    #[test]
    fn ignores_turbovec_bit_width_when_chroma_backend_selected() {
        let mut values = fixture();
        values.insert("VECTOR_STORE_BACKEND".to_string(), "chroma".to_string());
        values.insert("TURBOVEC_BIT_WIDTH".to_string(), "8".to_string());

        let config = AppConfig::from_map(&values).expect("Chroma rollback should ignore TurboVec");
        assert_eq!(config.vector_store_backend, VectorStoreBackend::Chroma);
        assert_eq!(config.turbovec_bit_width, 4);
    }

    #[test]
    fn rejects_invalid_uuid_values() {
        let mut values = fixture();
        values.insert(
            "CHUM_MEM_ORGANIZATION_ID".to_string(),
            "not-a-uuid".to_string(),
        );

        let error = AppConfig::from_map(&values).expect_err("invalid uuid should fail");
        assert_eq!(
            error,
            ConfigError::Invalid {
                key: "CHUM_MEM_ORGANIZATION_ID",
                value: "not-a-uuid".to_string(),
            }
        );
    }
}
