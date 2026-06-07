use std::future::Future;
use std::pin::Pin;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

pub const VECTOR_EMBEDDING_DIMENSIONS: usize = 1536;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VectorStoreItem {
    pub id: Uuid,
    pub vector: Vec<f32>,
    pub document: String,
    pub metadata: Value,
}

#[derive(Debug, Clone, Default)]
pub struct ScopeOptions {
    pub collection_name: String,
}

#[derive(Debug, Clone, Default)]
pub struct DeleteOptions {
    pub collection_name: String,
}

#[derive(Debug, Clone)]
pub struct SearchOptions {
    pub collection_name: String,
    pub types: Vec<String>,
    pub limit: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VectorSearchResult {
    pub id: Uuid,
    pub distance: f64,
    pub document: Option<String>,
    pub metadata: Option<Value>,
}

pub type VectorStoreFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, VectorStoreError>> + Send + 'a>>;

pub trait VectorStore {
    fn initialize(&self) -> VectorStoreFuture<'_, ()>;
    fn upsert<'a>(&'a self, items: &'a [VectorStoreItem]) -> VectorStoreFuture<'a, ()>;
    fn delete<'a>(&'a self, ids: &'a [Uuid], options: DeleteOptions) -> VectorStoreFuture<'a, ()>;
    fn get_by_id(
        &self,
        id: Uuid,
        options: ScopeOptions,
    ) -> VectorStoreFuture<'_, Option<VectorStoreItem>>;
    fn search<'a>(
        &'a self,
        query_vector: &'a [f32],
        options: SearchOptions,
    ) -> VectorStoreFuture<'a, Vec<VectorSearchResult>>;
}

#[derive(Debug, thiserror::Error)]
pub enum VectorStoreError {
    #[error("http vector store request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("io vector store operation failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("json vector store operation failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid vector store sidecar `{path}`: {source}")]
    InvalidSidecar {
        path: String,
        #[source]
        source: serde_json::Error,
    },
    #[error("invalid vector store sidecar `{path}`: {reason}")]
    InvalidSidecarState { path: String, reason: String },
    #[error("turbovec construction failed: {0}")]
    TurboVecConstruct(#[from] turbovec::ConstructError),
    #[error("turbovec add failed: {0}")]
    TurboVecAdd(#[from] turbovec::AddError),
    #[error("vector dimension mismatch: expected {expected}, got {got}")]
    DimensionMismatch { expected: usize, got: usize },
    #[error("vector id collision: {external_id} maps to both {existing} and {incoming}")]
    IdCollision {
        external_id: u64,
        existing: Uuid,
        incoming: Uuid,
    },
    #[error("vector store sidecar missing for existing index `{0}`")]
    MissingSidecar(String),
    #[error("vector store collection is locked by another writer: {0}")]
    Locked(String),
}

pub fn vector_from_f64(values: &[f64]) -> Result<Vec<f32>, VectorStoreError> {
    if values.len() != VECTOR_EMBEDDING_DIMENSIONS {
        return Err(VectorStoreError::DimensionMismatch {
            expected: VECTOR_EMBEDDING_DIMENSIONS,
            got: values.len(),
        });
    }
    Ok(values.iter().map(|value| *value as f32).collect())
}
