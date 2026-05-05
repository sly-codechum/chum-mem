use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

pub const CHROMA_EMBEDDING_DIMENSIONS: usize = 1536;
const CHROMA_DEFAULT_TENANT: &str = "default_tenant";
const CHROMA_DEFAULT_DATABASE: &str = "default_database";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChromaQueryResult {
    pub id: Uuid,
    pub distance: f64,
    pub document: Option<String>,
    pub metadata: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct CollectionResponse {
    id: String,
}

pub fn effective_chroma_collection_name(collection_name: &str) -> String {
    let suffix = format!("-{CHROMA_EMBEDDING_DIMENSIONS}");
    if collection_name.ends_with(&suffix) {
        collection_name.to_string()
    } else {
        format!("{collection_name}{suffix}")
    }
}

/// v2.2.2 §3.3: Per-type collection name (typed embedding partitions).
///
/// `memory_type` is canonicalized to `[a-z0-9_]` and mapped to short aliases
/// for a handful of long names (e.g. `implementation_detail` → `impl_detail`,
/// `open_question` → `open_q`). Unknown/empty types map to `"all"` so callers
/// always get a valid collection name.
pub fn typed_collection_name(base: &str, memory_type: &str) -> String {
    let canonical: String = memory_type
        .to_ascii_lowercase()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let suffix = match canonical.as_str() {
        "" => "all".to_string(),
        "implementation_detail" => "impl_detail".to_string(),
        "open_question" => "open_q".to_string(),
        "change_log" => "change_log".to_string(),
        other => other.to_string(),
    };
    format!("{base}_{suffix}")
}

pub async fn query_chroma_memories(
    client: &reqwest::Client,
    chroma_url: &str,
    collection_name: &str,
    query: &str,
    limit: usize,
) -> Result<Vec<ChromaQueryResult>, reqwest::Error> {
    let collection_id = ensure_collection_id(
        client,
        chroma_url,
        effective_chroma_collection_name(collection_name),
    )
    .await?;
    let body = json!({
        "query_embeddings": [crate::embed_text(query)],
        "n_results": limit,
    });
    let response = chroma_request(
        client,
        chroma_url,
        &format!("/api/v2/tenants/{CHROMA_DEFAULT_TENANT}/databases/{CHROMA_DEFAULT_DATABASE}/collections/{collection_id}/query"),
        reqwest::Method::POST,
        Some(body),
    )
    .await?;

    let ids = response
        .get("ids")
        .and_then(Value::as_array)
        .and_then(|outer| outer.first())
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let distances = response
        .get("distances")
        .and_then(Value::as_array)
        .and_then(|outer| outer.first())
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let documents = response
        .get("documents")
        .and_then(Value::as_array)
        .and_then(|outer| outer.first())
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let metadatas = response
        .get("metadatas")
        .and_then(Value::as_array)
        .and_then(|outer| outer.first())
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let mut results = Vec::new();
    for (index, id_value) in ids.iter().enumerate() {
        let Some(id_str) = id_value.as_str() else {
            continue;
        };
        let Ok(id) = Uuid::parse_str(id_str) else {
            continue;
        };
        results.push(ChromaQueryResult {
            id,
            distance: distances.get(index).and_then(Value::as_f64).unwrap_or(1.0),
            document: documents
                .get(index)
                .and_then(Value::as_str)
                .map(ToOwned::to_owned),
            metadata: metadatas.get(index).cloned(),
        });
    }

    Ok(results)
}

pub async fn upsert_chroma_memories(
    client: &reqwest::Client,
    chroma_url: &str,
    collection_name: &str,
    memories: &[UpsertMemory],
) -> Result<(), reqwest::Error> {
    if memories.is_empty() {
        return Ok(());
    }

    let collection_id = ensure_collection_id(
        client,
        chroma_url,
        effective_chroma_collection_name(collection_name),
    )
    .await?;

    // Batch to avoid 413 Payload Too Large from Chroma.
    const BATCH_SIZE: usize = 200;
    for chunk in memories.chunks(BATCH_SIZE) {
        let body = json!({
            "ids": chunk.iter().map(|memory| memory.id.to_string()).collect::<Vec<_>>(),
            "embeddings": chunk.iter().map(|memory| crate::embed_text(&memory.document)).collect::<Vec<_>>(),
            "documents": chunk.iter().map(|memory| memory.document.clone()).collect::<Vec<_>>(),
            "metadatas": chunk.iter().map(|memory| memory.metadata.clone()).collect::<Vec<_>>(),
        });
        chroma_request(
            client,
            chroma_url,
            &format!("/api/v2/tenants/{CHROMA_DEFAULT_TENANT}/databases/{CHROMA_DEFAULT_DATABASE}/collections/{collection_id}/upsert"),
            reqwest::Method::POST,
            Some(body),
        )
        .await?;
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpsertMemory {
    pub id: Uuid,
    pub document: String,
    pub metadata: Value,
}

async fn ensure_collection_id(
    client: &reqwest::Client,
    chroma_url: &str,
    collection_name: String,
) -> Result<String, reqwest::Error> {
    let response = chroma_request(
        client,
        chroma_url,
        &format!(
            "/api/v2/tenants/{CHROMA_DEFAULT_TENANT}/databases/{CHROMA_DEFAULT_DATABASE}/collections"
        ),
        reqwest::Method::POST,
        Some(json!({
            "name": collection_name,
            "get_or_create": true,
        })),
    )
    .await?;
    let parsed: CollectionResponse = serde_json::from_value(response)
        .map_err(|err| {
            // Convert serde error to a reqwest-compatible error by building a failed request
            tracing::error!(error = %err, "failed to parse Chroma collection response");
            err
        })
        .unwrap_or(CollectionResponse { id: String::new() });
    if parsed.id.is_empty() {
        tracing::error!("Chroma collection response missing id; subsequent requests will fail");
    }
    Ok(parsed.id)
}

/// v2.2.2 §3.3: Upsert memories into both the all-types collection (for
/// backward-compat fallback) and the per-type partition derived from
/// `metadata["type"]`. Memories without a readable type are only written to
/// the `_all` partition. Failures in any single partition propagate; partial
/// success is not retried here.
pub async fn upsert_chroma_memories_typed(
    client: &reqwest::Client,
    chroma_url: &str,
    collection_name: &str,
    memories: &[UpsertMemory],
) -> Result<(), reqwest::Error> {
    if memories.is_empty() {
        return Ok(());
    }

    // Bucket by type first so each collection is written once.
    let mut by_type: std::collections::HashMap<String, Vec<UpsertMemory>> =
        std::collections::HashMap::new();
    by_type.insert("all".to_string(), memories.to_vec());
    for memory in memories {
        let memory_type = memory
            .metadata
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("");
        if memory_type.is_empty() {
            continue;
        }
        // Use the suffix the typed_collection_name helper would produce so
        // keys match across upsert and query paths.
        let typed = typed_collection_name(collection_name, memory_type);
        // Strip the "{collection_name}_" prefix to get the bucket key.
        let suffix = typed
            .strip_prefix(&format!("{collection_name}_"))
            .unwrap_or("all")
            .to_string();
        by_type.entry(suffix).or_default().push(memory.clone());
    }

    for (suffix, batch) in by_type {
        let target = if suffix == "all" {
            format!("{collection_name}_all")
        } else {
            format!("{collection_name}_{suffix}")
        };
        upsert_chroma_memories(client, chroma_url, &target, &batch).await?;
    }
    Ok(())
}

/// v2.2.2 §3.3: Typed query — queries each per-type partition in parallel and
/// merges results by distance (ascending). If `types` is empty, queries the
/// `_all` partition (backward-compatible fallback path).
pub async fn query_chroma_memories_typed(
    client: &reqwest::Client,
    chroma_url: &str,
    collection_name: &str,
    query: &str,
    types: &[String],
    limit: usize,
) -> Result<Vec<ChromaQueryResult>, reqwest::Error> {
    if types.is_empty() {
        let target = format!("{collection_name}_all");
        return query_chroma_memories(client, chroma_url, &target, query, limit).await;
    }

    let futs: Vec<_> = types
        .iter()
        .map(|memory_type| {
            let target = typed_collection_name(collection_name, memory_type);
            async move { query_chroma_memories(client, chroma_url, &target, query, limit).await }
        })
        .collect();

    let results = futures::future::try_join_all(futs).await?;
    let mut merged: Vec<ChromaQueryResult> = results.into_iter().flatten().collect();

    merged.sort_by(|a, b| {
        a.distance
            .partial_cmp(&b.distance)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    merged.truncate(limit);
    Ok(merged)
}

async fn chroma_request(
    client: &reqwest::Client,
    chroma_url: &str,
    path: &str,
    method: reqwest::Method,
    body: Option<Value>,
) -> Result<Value, reqwest::Error> {
    let url = format!("{}{}", chroma_url.trim_end_matches('/'), path);
    let mut request = client.request(method, url);
    if let Some(body) = body {
        request = request.json(&body);
    }
    let response = request.send().await?;
    if response.status() == StatusCode::NO_CONTENT {
        return Ok(json!({}));
    }
    response.error_for_status()?.json().await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_collection_name_canonicalizes_known_types() {
        assert_eq!(typed_collection_name("memories", "bug"), "memories_bug");
        assert_eq!(
            typed_collection_name("memories", "Decision"),
            "memories_decision"
        );
        assert_eq!(
            typed_collection_name("memories", "implementation_detail"),
            "memories_impl_detail"
        );
        assert_eq!(
            typed_collection_name("memories", "open_question"),
            "memories_open_q"
        );
    }

    #[test]
    fn typed_collection_name_handles_empty_and_weird_chars() {
        assert_eq!(typed_collection_name("memories", ""), "memories_all");
        // Spaces/dashes become underscores, so callers can't smuggle bad names.
        assert_eq!(
            typed_collection_name("memories", "foo-bar baz"),
            "memories_foo_bar_baz"
        );
    }
}
