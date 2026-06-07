use std::collections::{BTreeMap, HashMap};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use turbovec::IdMapIndex;
use uuid::Uuid;

use crate::chroma::typed_collection_name;
use crate::vector_store::{
    DeleteOptions, ScopeOptions, SearchOptions, VECTOR_EMBEDDING_DIMENSIONS, VectorSearchResult,
    VectorStore, VectorStoreError, VectorStoreFuture, VectorStoreItem,
};

#[derive(Debug, Clone)]
pub struct TurboVecStore {
    root: PathBuf,
    base_collection_name: String,
    bit_width: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TurboVecScope {
    pub project_id: Option<Uuid>,
    pub user_id: Option<Uuid>,
    pub namespace: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TurboVecRecord {
    id: Uuid,
    document: String,
    metadata: Value,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
// One sidecar is stored beside each TurboVec index file. It is deliberately
// partition-local: internal TurboVec `u64` IDs are stable only within the
// matching `.tvim` file and always map back to the original memory UUID here.
struct TurboVecSidecar {
    records: BTreeMap<String, TurboVecRecord>,
    #[serde(default)]
    uuid_to_external_id: BTreeMap<String, u64>,
    #[serde(default)]
    external_id_to_uuid: BTreeMap<String, Uuid>,
    #[serde(default)]
    next_external_id: u64,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    id_map: BTreeMap<String, Uuid>,
}

impl TurboVecStore {
    pub fn new(
        root: impl Into<PathBuf>,
        base_collection_name: impl Into<String>,
        bit_width: usize,
    ) -> Self {
        Self {
            root: root.into(),
            base_collection_name: base_collection_name.into(),
            bit_width,
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn base_collection_name(&self) -> &str {
        &self.base_collection_name
    }

    pub fn upsert_typed(&self, items: &[VectorStoreItem]) -> Result<(), VectorStoreError> {
        if items.is_empty() {
            return Ok(());
        }

        let mut by_type: HashMap<String, Vec<VectorStoreItem>> = HashMap::new();
        by_type.insert("all".to_string(), items.to_vec());
        for item in items {
            let memory_type = item
                .metadata
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("");
            if let Some(scope) = scope_from_metadata(&item.metadata) {
                let scoped_all = scoped_collection_name(&self.base_collection_name, &scope, "");
                by_type
                    .entry(collection_suffix(&self.base_collection_name, &scoped_all))
                    .or_default()
                    .push(item.clone());
            }
            if memory_type.is_empty() {
                continue;
            }
            let typed = typed_collection_name(&self.base_collection_name, memory_type);
            let suffix = typed
                .strip_prefix(&format!("{}_", self.base_collection_name))
                .unwrap_or("all")
                .to_string();
            by_type.entry(suffix).or_default().push(item.clone());

            if let Some(scope) = scope_from_metadata(&item.metadata) {
                let scoped_typed =
                    scoped_collection_name(&self.base_collection_name, &scope, memory_type);
                by_type
                    .entry(collection_suffix(&self.base_collection_name, &scoped_typed))
                    .or_default()
                    .push(item.clone());
            }
        }

        for (suffix, batch) in by_type {
            let collection_name = if suffix == "all" {
                format!("{}_all", self.base_collection_name)
            } else {
                format!("{}_{}", self.base_collection_name, suffix)
            };
            self.upsert_collection(&collection_name, &batch)?;
        }
        Ok(())
    }

    pub fn search_typed_scoped(
        &self,
        query_vector: &[f32],
        types: &[String],
        limit: usize,
        scope: &TurboVecScope,
    ) -> Result<Vec<VectorSearchResult>, VectorStoreError> {
        if scope.is_empty() {
            return self.search_typed(query_vector, types, limit);
        }
        let collection_names = if types.is_empty() {
            vec![scoped_collection_name(
                &self.base_collection_name,
                scope,
                "",
            )]
        } else {
            types
                .iter()
                .map(|memory_type| {
                    scoped_collection_name(&self.base_collection_name, scope, memory_type)
                })
                .collect::<Vec<_>>()
        };
        self.search_collections(&collection_names, query_vector, limit)
    }

    pub fn search_typed(
        &self,
        query_vector: &[f32],
        types: &[String],
        limit: usize,
    ) -> Result<Vec<VectorSearchResult>, VectorStoreError> {
        let collection_names = if types.is_empty() {
            vec![format!("{}_all", self.base_collection_name)]
        } else {
            types
                .iter()
                .map(|memory_type| typed_collection_name(&self.base_collection_name, memory_type))
                .collect::<Vec<_>>()
        };

        self.search_collections(&collection_names, query_vector, limit)
    }

    fn upsert_collection(
        &self,
        collection_name: &str,
        items: &[VectorStoreItem],
    ) -> Result<(), VectorStoreError> {
        if items.is_empty() {
            return Ok(());
        }
        fs::create_dir_all(&self.root)?;
        let paths = self.paths(collection_name);
        let _lock = self.acquire_write_lock(&paths)?;
        let mut sidecar = self.load_sidecar(&paths)?;
        let mut index = self.load_or_create_index(&paths)?;

        let mut vectors = Vec::with_capacity(items.len() * VECTOR_EMBEDDING_DIMENSIONS);
        let mut ids = Vec::with_capacity(items.len());
        for item in items {
            if item.vector.len() != VECTOR_EMBEDDING_DIMENSIONS {
                return Err(VectorStoreError::DimensionMismatch {
                    expected: VECTOR_EMBEDDING_DIMENSIONS,
                    got: item.vector.len(),
                });
            }
            let external_id = external_id_for_uuid(&mut sidecar, item.id);
            if index.contains(external_id) {
                index.remove(external_id);
            }
            vectors.extend_from_slice(&item.vector);
            ids.push(external_id);
            sidecar.records.insert(
                item.id.to_string(),
                TurboVecRecord {
                    id: item.id,
                    document: item.document.clone(),
                    metadata: item.metadata.clone(),
                },
            );
        }

        index.add_with_ids(&vectors, &ids)?;
        self.write_index(&paths, &index)?;
        self.write_sidecar(&paths, &sidecar)?;
        Ok(())
    }

    fn search_collection(
        &self,
        collection_name: &str,
        query_vector: &[f32],
        limit: usize,
    ) -> Result<Vec<VectorSearchResult>, VectorStoreError> {
        if query_vector.len() != VECTOR_EMBEDDING_DIMENSIONS {
            return Err(VectorStoreError::DimensionMismatch {
                expected: VECTOR_EMBEDDING_DIMENSIONS,
                got: query_vector.len(),
            });
        }
        let paths = self.paths(collection_name);
        if !paths.index_path.exists() {
            return Ok(Vec::new());
        }
        let sidecar = self.load_sidecar(&paths)?;
        let index = IdMapIndex::load(&paths.index_path)?;
        if index.is_empty() || limit == 0 {
            return Ok(Vec::new());
        }
        let (scores, ids) = index.search(query_vector, limit);
        let mut results = Vec::new();
        for (score, external_id) in scores.into_iter().zip(ids) {
            let Some(memory_id) = sidecar.external_id_to_uuid.get(&external_id.to_string()) else {
                continue;
            };
            let Some(record) = sidecar.records.get(&memory_id.to_string()) else {
                continue;
            };
            results.push(VectorSearchResult {
                id: record.id,
                distance: (1.0_f64 - f64::from(score)).max(0.0),
                document: Some(record.document.clone()),
                metadata: Some(record.metadata.clone()),
            });
        }
        Ok(results)
    }

    fn delete_collection(
        &self,
        collection_name: &str,
        ids: &[Uuid],
    ) -> Result<(), VectorStoreError> {
        if collection_name == self.base_collection_name {
            return self.delete_from_matching_collections(ids);
        }
        let paths = self.paths(collection_name);
        if !paths.index_path.exists() {
            return Ok(());
        }
        let _lock = self.acquire_write_lock(&paths)?;
        let mut sidecar = self.load_sidecar(&paths)?;
        let mut index = IdMapIndex::load(&paths.index_path)?;
        for id in ids {
            if let Some(external_id) = sidecar.uuid_to_external_id.remove(&id.to_string()) {
                index.remove(external_id);
                sidecar.external_id_to_uuid.remove(&external_id.to_string());
            }
            sidecar.records.remove(&id.to_string());
        }
        self.write_index(&paths, &index)?;
        self.write_sidecar(&paths, &sidecar)?;
        Ok(())
    }

    fn delete_from_matching_collections(&self, ids: &[Uuid]) -> Result<(), VectorStoreError> {
        if !self.root.exists() {
            return Ok(());
        }
        let safe_base = sanitize_collection_name(&self.base_collection_name);
        for entry in fs::read_dir(&self.root)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) != Some("tvim") {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|value| value.to_str()) else {
                continue;
            };
            if stem.starts_with(&format!("{safe_base}_")) {
                self.delete_collection(stem, ids)?;
            }
        }
        Ok(())
    }

    fn get_collection_item(
        &self,
        collection_name: &str,
        id: Uuid,
    ) -> Result<Option<VectorStoreItem>, VectorStoreError> {
        let paths = self.paths(collection_name);
        if !paths.index_path.exists() {
            return Ok(None);
        }
        let sidecar = self.load_sidecar(&paths)?;
        Ok(sidecar
            .records
            .get(&id.to_string())
            .map(|record| VectorStoreItem {
                id: record.id,
                vector: Vec::new(),
                document: record.document.clone(),
                metadata: record.metadata.clone(),
            }))
    }

    fn load_or_create_index(&self, paths: &TurboVecPaths) -> Result<IdMapIndex, VectorStoreError> {
        if paths.index_path.exists() {
            Ok(IdMapIndex::load(&paths.index_path)?)
        } else {
            Ok(IdMapIndex::new(
                VECTOR_EMBEDDING_DIMENSIONS,
                self.bit_width,
            )?)
        }
    }

    fn load_sidecar(&self, paths: &TurboVecPaths) -> Result<TurboVecSidecar, VectorStoreError> {
        if paths.sidecar_path.exists() {
            let data = fs::read_to_string(&paths.sidecar_path)?;
            let sidecar =
                serde_json::from_str(&data).map_err(|source| VectorStoreError::InvalidSidecar {
                    path: paths.sidecar_path.display().to_string(),
                    source,
                })?;
            let sidecar = normalize_sidecar(sidecar);
            validate_sidecar(&sidecar, &paths.sidecar_path)?;
            Ok(sidecar)
        } else if paths.index_path.exists() {
            Err(VectorStoreError::MissingSidecar(
                paths.sidecar_path.display().to_string(),
            ))
        } else {
            Ok(TurboVecSidecar::default())
        }
    }

    fn write_sidecar(
        &self,
        paths: &TurboVecPaths,
        sidecar: &TurboVecSidecar,
    ) -> Result<(), VectorStoreError> {
        let data = serde_json::to_string_pretty(sidecar)?;
        let temp_path = paths.sidecar_path.with_extension("json.tmp");
        write_atomic_text(&temp_path, &paths.sidecar_path, &data)?;
        Ok(())
    }

    fn search_collections(
        &self,
        collection_names: &[String],
        query_vector: &[f32],
        limit: usize,
    ) -> Result<Vec<VectorSearchResult>, VectorStoreError> {
        let mut merged = Vec::new();
        for collection_name in collection_names {
            merged.extend(self.search_collection(collection_name, query_vector, limit)?);
        }
        merged.sort_by(|a, b| {
            a.distance
                .partial_cmp(&b.distance)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        merged.truncate(limit);
        Ok(merged)
    }

    fn acquire_write_lock(
        &self,
        paths: &TurboVecPaths,
    ) -> Result<TurboVecWriteLock, VectorStoreError> {
        let lock_path = paths.lock_path.clone();
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock_path)
        {
            Ok(mut file) => {
                writeln!(file, "pid={}", std::process::id())?;
                Ok(TurboVecWriteLock { path: lock_path })
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                Err(VectorStoreError::Locked(lock_path.display().to_string()))
            }
            Err(error) => Err(error.into()),
        }
    }

    fn write_index(
        &self,
        paths: &TurboVecPaths,
        index: &IdMapIndex,
    ) -> Result<(), VectorStoreError> {
        let temp_path = paths.index_path.with_extension("tvim.tmp");
        index.write(&temp_path)?;
        let file = OpenOptions::new().read(true).open(&temp_path)?;
        file.sync_all()?;
        fs::rename(temp_path, &paths.index_path)?;
        Ok(())
    }

    fn paths(&self, collection_name: &str) -> TurboVecPaths {
        let safe_collection = sanitize_collection_name(collection_name);
        TurboVecPaths {
            index_path: self.root.join(format!("{safe_collection}.tvim")),
            sidecar_path: self.root.join(format!("{safe_collection}.json")),
            lock_path: self.root.join(format!("{safe_collection}.lock")),
        }
    }
}

impl VectorStore for TurboVecStore {
    fn initialize(&self) -> VectorStoreFuture<'_, ()> {
        Box::pin(async move {
            fs::create_dir_all(&self.root)?;
            Ok(())
        })
    }

    fn upsert<'a>(&'a self, items: &'a [VectorStoreItem]) -> VectorStoreFuture<'a, ()> {
        Box::pin(async move { self.upsert_typed(items) })
    }

    fn delete<'a>(&'a self, ids: &'a [Uuid], options: DeleteOptions) -> VectorStoreFuture<'a, ()> {
        Box::pin(async move { self.delete_collection(&options.collection_name, ids) })
    }

    fn get_by_id(
        &self,
        id: Uuid,
        options: ScopeOptions,
    ) -> VectorStoreFuture<'_, Option<VectorStoreItem>> {
        Box::pin(async move { self.get_collection_item(&options.collection_name, id) })
    }

    fn search<'a>(
        &'a self,
        query_vector: &'a [f32],
        options: SearchOptions,
    ) -> VectorStoreFuture<'a, Vec<VectorSearchResult>> {
        Box::pin(async move {
            if options.collection_name == self.base_collection_name {
                self.search_typed(query_vector, &options.types, options.limit)
            } else {
                self.search_collection(&options.collection_name, query_vector, options.limit)
            }
        })
    }
}

impl TurboVecScope {
    fn is_empty(&self) -> bool {
        self.project_id.is_none() && self.user_id.is_none() && self.namespace.is_none()
    }
}

#[derive(Debug, Clone)]
struct TurboVecPaths {
    index_path: PathBuf,
    sidecar_path: PathBuf,
    lock_path: PathBuf,
}

fn sanitize_collection_name(value: &str) -> String {
    value
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn scoped_collection_name(base: &str, scope: &TurboVecScope, memory_type: &str) -> String {
    let scoped_base = format!("{base}_{}", scope_partition_suffix(scope));
    typed_collection_name(&scoped_base, memory_type)
}

fn scope_partition_suffix(scope: &TurboVecScope) -> String {
    let mut parts = Vec::new();
    if let Some(project_id) = scope.project_id {
        parts.push(format!("project_{}", project_id.simple()));
    }
    if let Some(user_id) = scope.user_id {
        parts.push(format!("user_{}", user_id.simple()));
    }
    if let Some(namespace) = scope.namespace.as_deref() {
        parts.push(format!("namespace_{}", safe_component(namespace)));
    }
    if parts.is_empty() {
        "global".to_string()
    } else {
        parts.join("_")
    }
}

fn collection_suffix(base: &str, collection_name: &str) -> String {
    collection_name
        .strip_prefix(&format!("{base}_"))
        .unwrap_or(collection_name)
        .to_string()
}

fn scope_from_metadata(metadata: &Value) -> Option<TurboVecScope> {
    let scope = TurboVecScope {
        project_id: metadata_uuid(metadata, "projectId"),
        user_id: metadata_uuid(metadata, "userId"),
        namespace: metadata
            .get("namespace")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(ToOwned::to_owned),
    };
    (!scope.is_empty()).then_some(scope)
}

fn metadata_uuid(metadata: &Value, key: &str) -> Option<Uuid> {
    metadata
        .get(key)
        .and_then(Value::as_str)
        .and_then(|value| Uuid::parse_str(value).ok())
}

fn safe_component(value: &str) -> String {
    let canonical = value.trim().to_ascii_lowercase();
    let sanitized: String = canonical
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let sanitized = sanitized.trim_matches('_');
    let sanitized = if sanitized.is_empty() {
        "value"
    } else {
        sanitized
    };
    if sanitized == canonical {
        sanitized.to_string()
    } else {
        let mut hasher = Sha256::new();
        hasher.update(value.as_bytes());
        let digest = hasher.finalize();
        format!(
            "{sanitized}_{:02x}{:02x}{:02x}{:02x}",
            digest[0], digest[1], digest[2], digest[3]
        )
    }
}

fn write_atomic_text(
    temp_path: &Path,
    target_path: &Path,
    data: &str,
) -> Result<(), VectorStoreError> {
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(temp_path)?;
    file.write_all(data.as_bytes())?;
    file.sync_all()?;
    drop(file);
    fs::rename(temp_path, target_path)?;
    Ok(())
}

struct TurboVecWriteLock {
    path: PathBuf,
}

impl Drop for TurboVecWriteLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn normalize_sidecar(mut sidecar: TurboVecSidecar) -> TurboVecSidecar {
    if sidecar.external_id_to_uuid.is_empty() && !sidecar.id_map.is_empty() {
        for (external_id, uuid) in std::mem::take(&mut sidecar.id_map) {
            if let Ok(parsed_external_id) = external_id.parse::<u64>() {
                sidecar
                    .uuid_to_external_id
                    .entry(uuid.to_string())
                    .or_insert(parsed_external_id);
                sidecar
                    .external_id_to_uuid
                    .entry(parsed_external_id.to_string())
                    .or_insert(uuid);
            }
        }
    }

    let next_from_existing = sidecar
        .external_id_to_uuid
        .keys()
        .filter_map(|key| key.parse::<u64>().ok())
        .max()
        .unwrap_or(0)
        .saturating_add(1);
    sidecar.next_external_id = sidecar.next_external_id.max(next_from_existing).max(1);
    sidecar
}

fn validate_sidecar(sidecar: &TurboVecSidecar, path: &Path) -> Result<(), VectorStoreError> {
    for (uuid_key, external_id) in &sidecar.uuid_to_external_id {
        let uuid =
            Uuid::parse_str(uuid_key).map_err(|_| VectorStoreError::InvalidSidecarState {
                path: path.display().to_string(),
                reason: format!("invalid UUID key `{uuid_key}` in uuidToExternalId"),
            })?;
        let mapped_uuid = sidecar
            .external_id_to_uuid
            .get(&external_id.to_string())
            .ok_or_else(|| VectorStoreError::InvalidSidecarState {
                path: path.display().to_string(),
                reason: format!("missing reverse mapping for external id `{external_id}`"),
            })?;
        if *mapped_uuid != uuid {
            return Err(VectorStoreError::InvalidSidecarState {
                path: path.display().to_string(),
                reason: format!(
                    "external id `{external_id}` maps to `{mapped_uuid}` but `{uuid}` expected"
                ),
            });
        }
    }

    for (external_id, uuid) in &sidecar.external_id_to_uuid {
        let mapped_external = sidecar
            .uuid_to_external_id
            .get(&uuid.to_string())
            .ok_or_else(|| VectorStoreError::InvalidSidecarState {
                path: path.display().to_string(),
                reason: format!("missing forward mapping for UUID `{uuid}`"),
            })?;
        if external_id != &mapped_external.to_string() {
            return Err(VectorStoreError::InvalidSidecarState {
                path: path.display().to_string(),
                reason: format!(
                    "UUID `{uuid}` maps to `{mapped_external}` but `{external_id}` expected"
                ),
            });
        }
    }

    for (record_key, record) in &sidecar.records {
        if record_key != &record.id.to_string() {
            return Err(VectorStoreError::InvalidSidecarState {
                path: path.display().to_string(),
                reason: format!(
                    "record key `{record_key}` does not match record id `{}`",
                    record.id
                ),
            });
        }
    }

    Ok(())
}

fn external_id_for_uuid(sidecar: &mut TurboVecSidecar, id: Uuid) -> u64 {
    if let Some(external_id) = sidecar.uuid_to_external_id.get(&id.to_string()) {
        return *external_id;
    }

    let mut external_id = sidecar.next_external_id.max(1);
    while sidecar
        .external_id_to_uuid
        .contains_key(&external_id.to_string())
    {
        external_id = external_id.saturating_add(1);
    }
    sidecar
        .uuid_to_external_id
        .insert(id.to_string(), external_id);
    sidecar
        .external_id_to_uuid
        .insert(external_id.to_string(), id);
    sidecar.next_external_id = external_id.saturating_add(1);
    external_id
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_store(name: &str) -> TurboVecStore {
        let root =
            std::env::temp_dir().join(format!("chum-mem-turbovec-{name}-{}", Uuid::new_v4()));
        TurboVecStore::new(root, "memories", 4)
    }

    fn item(id: Uuid, document: &str, memory_type: &str, first_coord: f32) -> VectorStoreItem {
        let mut vector = vec![0.0_f32; VECTOR_EMBEDDING_DIMENSIONS];
        vector[0] = first_coord;
        vector[1] = 1.0 - first_coord;
        VectorStoreItem {
            id,
            vector,
            document: document.to_string(),
            metadata: serde_json::json!({
                "type": memory_type,
                "title": document,
            }),
        }
    }

    fn scoped_item(
        id: Uuid,
        document: &str,
        memory_type: &str,
        project_id: Uuid,
        namespace: &str,
        first_coord: f32,
    ) -> VectorStoreItem {
        let mut base = item(id, document, memory_type, first_coord);
        base.metadata = serde_json::json!({
            "type": memory_type,
            "title": document,
            "projectId": project_id,
            "namespace": namespace,
        });
        base
    }

    #[test]
    fn upsert_and_get_by_id_preserves_uuid_document_and_metadata() {
        let store = temp_store("search");
        let first_id = Uuid::new_v4();
        let second_id = Uuid::new_v4();
        store
            .upsert_typed(&[
                item(first_id, "worker bug memory", "bug", 1.0),
                item(second_id, "design decision memory", "decision", 0.0),
            ])
            .expect("upsert should succeed");

        let found = store
            .get_collection_item("memories_bug", first_id)
            .expect("lookup should succeed")
            .expect("record should exist");

        assert_eq!(found.id, first_id);
        assert_eq!(found.document, "worker bug memory");
        assert_eq!(
            found.metadata.get("type").and_then(Value::as_str),
            Some("bug")
        );
    }

    #[test]
    fn upsert_replaces_existing_id() {
        let store = temp_store("replace");
        let id = Uuid::new_v4();
        store
            .upsert_typed(&[item(id, "old title", "fact", 1.0)])
            .expect("initial upsert should succeed");
        store
            .upsert_typed(&[item(id, "new title", "fact", 1.0)])
            .expect("replacement upsert should succeed");

        let found = store
            .get_collection_item("memories_fact", id)
            .expect("lookup should succeed")
            .expect("record should exist");

        assert_eq!(found.id, id);
        assert_eq!(found.document, "new title");
    }

    #[test]
    fn search_empty_index_returns_no_results() {
        let store = temp_store("empty-search");
        let query = vec![0.0_f32; VECTOR_EMBEDDING_DIMENSIONS];

        let results = store
            .search_typed(&query, &["fact".to_string()], 10)
            .expect("empty search should succeed");

        assert!(results.is_empty());
    }

    #[test]
    fn delete_is_idempotent_and_removes_sidecar_record() {
        let store = temp_store("delete");
        let id = Uuid::new_v4();
        store
            .upsert_typed(&[item(id, "delete me", "fact", 1.0)])
            .expect("upsert should succeed");

        store
            .delete_collection("memories_fact", &[id])
            .expect("delete should succeed");
        store
            .delete_collection("memories_fact", &[id])
            .expect("duplicate delete should succeed");

        let found = store
            .get_collection_item("memories_fact", id)
            .expect("lookup should succeed");
        assert!(found.is_none());
    }

    #[test]
    fn sidecar_allocates_distinct_ids_for_uuid_prefix_collisions() {
        let mut sidecar = TurboVecSidecar::default();
        let first = Uuid::from_bytes([
            0x10, 0x20, 0x30, 0x40, 0x50, 0x60, 0x70, 0x80, 0x90, 0xa0, 0xb0, 0xc0, 0xd0, 0xe0,
            0xf0, 0x00,
        ]);
        let second = Uuid::from_bytes([
            0x10, 0x20, 0x30, 0x40, 0x50, 0x60, 0x70, 0x80, 0x90, 0xa0, 0xb0, 0xc0, 0xd0, 0xe0,
            0xf0, 0x01,
        ]);

        let first_external = external_id_for_uuid(&mut sidecar, first);
        let second_external = external_id_for_uuid(&mut sidecar, second);

        assert_ne!(first_external, second_external);
        assert_eq!(
            sidecar.external_id_to_uuid[&first_external.to_string()],
            first
        );
        assert_eq!(
            sidecar.external_id_to_uuid[&second_external.to_string()],
            second
        );
    }

    #[test]
    fn duplicate_uuid_reuses_same_internal_id() {
        let mut sidecar = TurboVecSidecar::default();
        let id = Uuid::new_v4();

        let first = external_id_for_uuid(&mut sidecar, id);
        let second = external_id_for_uuid(&mut sidecar, id);

        assert_eq!(first, second);
        assert_eq!(sidecar.uuid_to_external_id.len(), 1);
        assert_eq!(sidecar.external_id_to_uuid.len(), 1);
    }

    #[test]
    fn sidecar_mapping_survives_reload() {
        let store = temp_store("sidecar-reload");
        fs::create_dir_all(store.root()).expect("root should be created");
        let paths = store.paths("memories_fact");
        let id = Uuid::new_v4();
        let mut sidecar = TurboVecSidecar::default();
        let external_id = external_id_for_uuid(&mut sidecar, id);

        store
            .write_sidecar(&paths, &sidecar)
            .expect("sidecar write should succeed");
        let reloaded = store.load_sidecar(&paths).expect("sidecar should load");

        assert_eq!(
            reloaded.uuid_to_external_id.get(&id.to_string()).copied(),
            Some(external_id)
        );
        assert_eq!(
            reloaded.external_id_to_uuid.get(&external_id.to_string()),
            Some(&id)
        );
    }

    #[test]
    fn legacy_sidecar_normalization_preserves_existing_mapping() {
        let id = Uuid::new_v4();
        let mut legacy = TurboVecSidecar::default();
        legacy.id_map.insert("42".to_string(), id);

        let normalized = normalize_sidecar(legacy);

        assert_eq!(
            normalized.uuid_to_external_id.get(&id.to_string()).copied(),
            Some(42)
        );
        assert_eq!(normalized.external_id_to_uuid.get("42"), Some(&id));
        assert!(normalized.next_external_id > 42);
    }

    #[test]
    fn corrupt_sidecar_returns_actionable_error() {
        let store = temp_store("corrupt-sidecar");
        fs::create_dir_all(store.root()).expect("root should be created");
        let paths = store.paths("memories_fact");
        fs::write(&paths.sidecar_path, "{not-json").expect("corrupt sidecar should be written");

        let err = store
            .load_sidecar(&paths)
            .expect_err("corrupt sidecar should fail");

        match err {
            VectorStoreError::InvalidSidecar { path, .. } => {
                assert!(path.ends_with("memories_fact.json"));
            }
            other => panic!("expected InvalidSidecar, got {other:?}"),
        }
    }

    #[test]
    fn inconsistent_sidecar_mapping_fails_loudly() {
        let store = temp_store("bad-sidecar-state");
        fs::create_dir_all(store.root()).expect("root should be created");
        let paths = store.paths("memories_fact");
        let first = Uuid::new_v4();
        let second = Uuid::new_v4();
        let sidecar = format!(
            r#"{{"records":{{}},"uuidToExternalId":{{"{first}":7}},"externalIdToUuid":{{"7":"{second}"}},"nextExternalId":8}}"#
        );
        fs::write(&paths.sidecar_path, sidecar).expect("sidecar should be written");

        let err = store
            .load_sidecar(&paths)
            .expect_err("inconsistent mapping should fail");

        match err {
            VectorStoreError::InvalidSidecarState { path, reason } => {
                assert!(path.ends_with("memories_fact.json"));
                assert!(reason.contains("external id `7`"));
            }
            other => panic!("expected InvalidSidecarState, got {other:?}"),
        }
    }

    #[test]
    fn atomic_sidecar_save_replaces_target_and_removes_temp_file() {
        let store = temp_store("atomic-sidecar");
        fs::create_dir_all(store.root()).expect("root should be created");
        let paths = store.paths("memories_fact");
        let mut sidecar = TurboVecSidecar::default();
        external_id_for_uuid(&mut sidecar, Uuid::new_v4());

        store
            .write_sidecar(&paths, &sidecar)
            .expect("sidecar write should succeed");

        assert!(paths.sidecar_path.exists());
        assert!(!paths.sidecar_path.with_extension("json.tmp").exists());
    }

    #[test]
    fn scoped_partition_names_are_deterministic_and_filesystem_safe() {
        let scope = TurboVecScope {
            project_id: Some(Uuid::parse_str("00000000-0000-0000-0000-000000000003").unwrap()),
            user_id: None,
            namespace: Some("Team A/Prod:West".to_string()),
        };

        let first = scoped_collection_name("memories", &scope, "implementation_detail");
        let second = scoped_collection_name("memories", &scope, "implementation_detail");

        assert_eq!(first, second);
        assert!(first.starts_with("memories_project_00000000000000000000000000000003_namespace_"));
        assert!(first.ends_with("_impl_detail"));
        assert!(first.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'));
    }

    #[test]
    fn scoped_missing_partition_search_returns_empty_results() {
        let store = temp_store("missing-scoped-search");
        let query = vec![0.0_f32; VECTOR_EMBEDDING_DIMENSIONS];
        let scope = TurboVecScope {
            project_id: Some(Uuid::new_v4()),
            user_id: None,
            namespace: Some("docs".to_string()),
        };

        let results = store
            .search_typed_scoped(&query, &["fact".to_string()], 10, &scope)
            .expect("missing scoped partition should not fail");

        assert!(results.is_empty());
    }

    #[test]
    fn dimension_mismatch_fails_clearly() {
        let store = temp_store("bad-dim");
        let err = store
            .search_typed(&[0.0_f32; 2], &["fact".to_string()], 10)
            .expect_err("dimension mismatch should fail");

        match err {
            VectorStoreError::DimensionMismatch { expected, got } => {
                assert_eq!(expected, VECTOR_EMBEDDING_DIMENSIONS);
                assert_eq!(got, 2);
            }
            other => panic!("expected DimensionMismatch, got {other:?}"),
        }
    }

    #[test]
    fn existing_lock_file_blocks_writer() {
        let store = temp_store("lock");
        fs::create_dir_all(store.root()).expect("root should be created");
        let paths = store.paths("memories_all");
        fs::write(&paths.lock_path, "pid=test").expect("lock file should be written");

        let err = store
            .upsert_typed(&[item(Uuid::new_v4(), "locked", "fact", 1.0)])
            .expect_err("lock should block writer");

        match err {
            VectorStoreError::Locked(path) => assert!(path.ends_with("memories_all.lock")),
            other => panic!("expected Locked, got {other:?}"),
        }
    }

    #[test]
    fn failed_save_path_returns_error() {
        let root = std::env::temp_dir().join(format!("chum-mem-turbovec-file-{}", Uuid::new_v4()));
        fs::write(&root, "not a directory").expect("file root should be created");
        let store = TurboVecStore::new(root, "memories", 4);

        let err = store
            .upsert_typed(&[item(Uuid::new_v4(), "cannot save", "fact", 1.0)])
            .expect_err("file root should fail");

        assert!(matches!(err, VectorStoreError::Io(_)));
    }

    #[test]
    #[ignore = "TurboVec 1536-dimensional first search initializes heavy SIMD/BLAS caches"]
    fn search_returns_typed_results() {
        let store = temp_store("typed-search");
        let id = Uuid::new_v4();
        store
            .upsert_typed(&[item(id, "typed searchable title", "fact", 1.0)])
            .expect("upsert should succeed");

        let mut query = vec![0.0_f32; VECTOR_EMBEDDING_DIMENSIONS];
        query[0] = 1.0;
        let results = store
            .search_typed(&query, &["fact".to_string()], 10)
            .expect("search should succeed");

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, id);
        assert_eq!(
            results[0].document.as_deref(),
            Some("typed searchable title")
        );
        assert_eq!(
            results[0]
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.get("type"))
                .and_then(Value::as_str),
            Some("fact")
        );
    }

    #[test]
    #[ignore = "TurboVec 1536-dimensional scoped search is slow in routine unit runs"]
    fn scoped_search_returns_only_requested_project_partition() {
        let store = temp_store("scoped-search");
        let project_a = Uuid::new_v4();
        let project_b = Uuid::new_v4();
        let id_a = Uuid::new_v4();
        let id_b = Uuid::new_v4();
        store
            .upsert_typed(&[
                scoped_item(id_a, "project a memory", "fact", project_a, "docs", 1.0),
                scoped_item(id_b, "project b memory", "fact", project_b, "docs", 1.0),
            ])
            .expect("upsert should succeed");

        let mut query = vec![0.0_f32; VECTOR_EMBEDDING_DIMENSIONS];
        query[0] = 1.0;
        let results = store
            .search_typed_scoped(
                &query,
                &["fact".to_string()],
                10,
                &TurboVecScope {
                    project_id: Some(project_a),
                    user_id: None,
                    namespace: Some("docs".to_string()),
                },
            )
            .expect("scoped search should succeed");

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, id_a);
    }

    #[test]
    #[ignore = "TurboVec 1536-dimensional top-k search is slow in routine unit runs"]
    fn search_respects_top_k() {
        let store = temp_store("top-k");
        store
            .upsert_typed(&[
                item(Uuid::new_v4(), "first", "fact", 1.0),
                item(Uuid::new_v4(), "second", "fact", 0.9),
                item(Uuid::new_v4(), "third", "fact", 0.8),
            ])
            .expect("upsert should succeed");

        let mut query = vec![0.0_f32; VECTOR_EMBEDDING_DIMENSIONS];
        query[0] = 1.0;
        let results = store
            .search_typed(&query, &["fact".to_string()], 2)
            .expect("search should succeed");

        assert_eq!(results.len(), 2);
    }
}
