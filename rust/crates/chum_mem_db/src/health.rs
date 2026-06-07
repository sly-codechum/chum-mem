use chum_mem_app::{DependencyStatus, ReadyResponse, ServiceMetadata};
use chum_mem_config::AppConfig;
use reqwest::StatusCode;
use sqlx::PgPool;
use tracing::warn;

use crate::migrate::{EXPECTED_MIGRATION_HEAD, require_latest_migration_head};

#[derive(Debug, Clone)]
pub struct DependencyReadiness {
    pub healthy: bool,
    pub detail: String,
}

#[derive(Debug, Clone)]
pub struct ReadinessReport {
    pub database: DependencyReadiness,
    pub chroma: Option<DependencyReadiness>,
}

impl ReadinessReport {
    pub fn is_ready(&self) -> bool {
        self.database.healthy
            && self
                .chroma
                .as_ref()
                .map(|dependency| dependency.healthy)
                .unwrap_or(true)
    }

    pub fn into_response(self, metadata: ServiceMetadata) -> ReadyResponse {
        let mut dependencies = vec![DependencyStatus {
            name: "postgres",
            healthy: self.database.healthy,
            detail: self.database.detail,
        }];

        if let Some(chroma) = self.chroma {
            dependencies.push(DependencyStatus {
                name: "chroma",
                healthy: chroma.healthy,
                detail: chroma.detail,
            });
        }

        ReadyResponse {
            service: metadata.name,
            version: metadata.version,
            role: metadata.role,
            status: if dependencies.iter().all(|dependency| dependency.healthy) {
                "ready"
            } else {
                "degraded"
            },
            dependencies,
        }
    }
}

pub async fn check_readiness(
    pool: &PgPool,
    config: &AppConfig,
) -> Result<ReadinessReport, crate::DbError> {
    let database = match sqlx::query_scalar::<_, i32>("select 1")
        .fetch_one(pool)
        .await
    {
        Ok(_) => match require_latest_migration_head(pool).await {
            Ok(_) => DependencyReadiness {
                healthy: true,
                detail: format!("reachable and migrated through {EXPECTED_MIGRATION_HEAD}"),
            },
            Err(error) => DependencyReadiness {
                healthy: false,
                detail: error.to_string(),
            },
        },
        Err(error) => DependencyReadiness {
            healthy: false,
            detail: error.to_string(),
        },
    };

    let chroma = if config.chroma_enabled() {
        match &config.chroma_url {
            Some(base_url) => {
                let client = reqwest::Client::builder()
                    .timeout(config.readiness_timeout())
                    .build()
                    .map_err(crate::DbError::from)?;
                let heartbeat_url = base_url
                    .join("/api/v1/heartbeat")
                    .unwrap_or_else(|_| base_url.clone());

                let readiness = match client.get(heartbeat_url).send().await {
                    Ok(response) if response.status().is_success() => DependencyReadiness {
                        healthy: true,
                        detail: "heartbeat ok".to_string(),
                    },
                    Ok(response)
                        if matches!(
                            response.status(),
                            StatusCode::NOT_FOUND
                                | StatusCode::GONE
                                | StatusCode::METHOD_NOT_ALLOWED
                        ) =>
                    {
                        DependencyReadiness {
                            healthy: true,
                            detail: format!(
                                "service reachable; heartbeat endpoint returned {}",
                                response.status()
                            ),
                        }
                    }
                    Ok(response) => DependencyReadiness {
                        healthy: false,
                        detail: format!("unexpected status {}", response.status()),
                    },
                    Err(error) => {
                        warn!(error = %error, "chroma readiness probe failed");
                        DependencyReadiness {
                            healthy: false,
                            detail: error.to_string(),
                        }
                    }
                };

                Some(readiness)
            }
            None => None,
        }
    } else {
        None
    };

    Ok(ReadinessReport { database, chroma })
}
