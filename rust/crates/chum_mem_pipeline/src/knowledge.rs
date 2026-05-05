use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::{SessionEpisodeDraft, SessionEventRecord, event_text};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeNode {
    pub id: String,
    pub label: String,
    #[serde(rename = "type")]
    pub node_type: String,
    pub source_type: String,
    pub source_id: String,
    #[serde(default)]
    pub metadata: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub community_id: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeEdge {
    pub source: String,
    pub target: String,
    pub relation: String,
    pub evidence: String,
    pub weight: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_file: Option<String>,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CommunityInfo {
    pub community_id: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub node_count: usize,
    pub cohesion_score: f64,
    #[serde(default)]
    pub representative_nodes: Vec<String>,
    #[serde(default)]
    pub bridge_nodes: Vec<String>,
    /// v2.2.2: Hierarchical community path (e.g., "3.7.12" for level-0=3, level-1=7, level-2=12).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub community_path: Option<String>,
    /// v2.2.2: Hierarchy level (0 = coarsest).
    #[serde(default)]
    pub level: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GraphStatistics {
    pub node_count: usize,
    pub edge_count: usize,
    pub community_count: usize,
    pub evidence_distribution: EvidenceDistribution,
    pub avg_degree: f64,
    pub density: f64,
    pub isolated_nodes: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceDistribution {
    pub extracted: usize,
    pub inferred: usize,
    pub ambiguous: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeGraph {
    pub version: String,
    pub generated_at: String,
    pub project_id: Uuid,
    pub nodes: Vec<KnowledgeNode>,
    pub edges: Vec<KnowledgeEdge>,
    #[serde(default)]
    pub communities: Vec<CommunityInfo>,
    pub statistics: GraphStatistics,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GraphProjection {
    pub total_nodes: usize,
    pub total_edges: usize,
    pub returned_nodes: usize,
    pub returned_edges: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GraphQueryResponse {
    #[serde(default)]
    pub nodes: Vec<KnowledgeNode>,
    #[serde(default)]
    pub edges: Vec<KnowledgeEdge>,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MemoryNodeInput {
    pub id: Uuid,
    pub memory_type: String,
    pub title: String,
    pub content: String,
    pub summary: String,
    pub importance_score: f64,
    pub metadata: Value,
}

pub fn build_knowledge_graph(
    project_id: Uuid,
    session_id: Uuid,
    events: &[SessionEventRecord],
    episodes: &[SessionEpisodeDraft],
    memories: &[MemoryNodeInput],
    prior_session_ids: &[Uuid],
) -> KnowledgeGraph {
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    let mut seen_nodes = HashSet::new();

    add_node(
        &mut nodes,
        &mut seen_nodes,
        KnowledgeNode {
            id: format!("session:{session_id}"),
            label: format!("Session {}", &session_id.to_string()[..8]),
            node_type: "session".to_string(),
            source_type: "session_event".to_string(),
            source_id: session_id.to_string(),
            metadata: json!({ "eventCount": events.len() }),
            community_id: None,
        },
    );

    extract_structural(session_id, events, &mut nodes, &mut edges, &mut seen_nodes);
    extract_semantic(
        session_id,
        episodes,
        memories,
        prior_session_ids,
        events,
        &mut nodes,
        &mut edges,
        &mut seen_nodes,
    );

    let mut graph = KnowledgeGraph {
        version: "1.0.0".to_string(),
        generated_at: time::OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_else(|_| time::OffsetDateTime::now_utc().unix_timestamp().to_string()),
        project_id,
        nodes,
        edges: dedupe_edges(edges),
        communities: Vec::new(),
        statistics: empty_stats(),
    };
    graph.statistics = compute_statistics(&graph.nodes, &graph.edges, &graph.communities);
    graph
}

pub fn merge_graphs(base: &KnowledgeGraph, increment: &KnowledgeGraph) -> KnowledgeGraph {
    // Use IndexMap to preserve insertion order (matching Node.js Map behavior).
    // BTreeMap would sort alphabetically by ID, separating related nodes
    // (e.g., session:X comes after episode:X, memory:X), which causes
    // project_graph_for_dashboard to select topologically disconnected nodes.
    let mut node_map = indexmap::IndexMap::new();
    for node in &base.nodes {
        node_map.insert(node.id.clone(), node.clone());
    }
    for node in &increment.nodes {
        node_map.insert(node.id.clone(), node.clone());
    }

    let mut merged_edges = base.edges.clone();
    merged_edges.extend(increment.edges.clone());
    let edges = dedupe_edges(merged_edges);
    let communities = if increment.communities.is_empty() {
        base.communities.clone()
    } else {
        increment.communities.clone()
    };

    let mut graph = KnowledgeGraph {
        version: increment.version.clone(),
        generated_at: increment.generated_at.clone(),
        project_id: increment.project_id,
        nodes: node_map.into_values().collect(),
        edges,
        communities,
        statistics: empty_stats(),
    };
    graph.statistics = compute_statistics(&graph.nodes, &graph.edges, &graph.communities);
    graph
}

pub fn assign_communities_with_budget(
    graph: &KnowledgeGraph,
    max_nodes: usize,
    max_edges: usize,
) -> KnowledgeGraph {
    if graph.nodes.len() < 3 {
        let mut unchanged = graph.clone();
        unchanged.statistics =
            compute_statistics(&unchanged.nodes, &unchanged.edges, &unchanged.communities);
        return unchanged;
    }
    if graph.nodes.len() > max_nodes || graph.edges.len() > max_edges {
        let mut unchanged = graph.clone();
        unchanged.statistics =
            compute_statistics(&unchanged.nodes, &unchanged.edges, &unchanged.communities);
        return unchanged;
    }

    let mut adjacency = build_adjacency_weighted(&graph.edges);

    // v2.2.2: God Node damping — reduce edge weights for high-degree nodes
    // so they don't dominate community assignment.
    damp_hub_edges(&graph.nodes, &mut adjacency);

    // Match Node.js: find connected components first, cluster each separately
    let components = find_connected_components(&graph.nodes, &adjacency);
    let min_nodes_for_clustering = 3;

    let mut node_community = HashMap::new();
    let mut next_community_id = 0usize;

    for component in &components {
        if component.len() < min_nodes_for_clustering {
            // Small component: assign all to single community
            for node_id in component {
                node_community.insert(node_id.clone(), next_community_id);
            }
            next_community_id += 1;
        } else if component.len() < 20 {
            // Match Node.js: single component with < 20 nodes gets a single community
            for node_id in component {
                node_community.insert(node_id.clone(), next_community_id);
            }
            next_community_id += 1;
        } else {
            // Run modularity clustering on this component
            let sub_nodes: Vec<KnowledgeNode> = graph
                .nodes
                .iter()
                .filter(|n| component.contains(&n.id))
                .cloned()
                .collect();
            let sub_assignments = greedy_modularity_clustering(&sub_nodes, &adjacency);

            // Remap local community IDs to global IDs
            let mut local_to_global = HashMap::new();
            for (node_id, local_id) in sub_assignments {
                let global_id = *local_to_global.entry(local_id).or_insert_with(|| {
                    let id = next_community_id;
                    next_community_id += 1;
                    id
                });
                node_community.insert(node_id, global_id);
            }
        }
    }

    // Split oversized communities (matching Node.js splitOversizedCommunities)
    let max_community_size = (graph.nodes.len() as f64 * 0.25).floor().max(10.0) as usize;
    split_oversized_communities(&mut node_community, &adjacency, max_community_size);

    let mut community_ids: Vec<usize> = node_community
        .values()
        .copied()
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    community_ids.sort();
    let remap: HashMap<usize, usize> = community_ids
        .iter()
        .enumerate()
        .map(|(i, &old)| (old, i))
        .collect();

    for v in node_community.values_mut() {
        if let Some(&new_id) = remap.get(v) {
            *v = new_id;
        }
    }

    let mut communities_by_id: BTreeMap<usize, Vec<String>> = BTreeMap::new();
    for (node_id, &community_id) in &node_community {
        communities_by_id
            .entry(community_id)
            .or_default()
            .push(node_id.clone());
    }

    let node_map: HashMap<&str, &KnowledgeNode> =
        graph.nodes.iter().map(|n| (n.id.as_str(), n)).collect();

    // v2.2.2: Build level-0 communities and then run hierarchical sub-clustering
    let mut communities = Vec::new();
    let mut node_community_path: HashMap<String, String> = HashMap::new();
    // Sub-community IDs must live in a namespace disjoint from parent IDs to
    // satisfy the `(project_id, community_id)` unique constraint on
    // `public.knowledge_communities`. Start the counter above any parent id.
    let mut next_sub_community_id = communities_by_id
        .keys()
        .copied()
        .max()
        .map(|m| m + 1)
        .unwrap_or(0);
    for (community_id, members) in &communities_by_id {
        let mut info = build_community(*community_id, members, &graph.edges, &node_map);
        info.community_path = Some(community_id.to_string());
        info.level = 0;
        communities.push(info);

        // Set level-0 path for all members
        for m in members {
            node_community_path.insert(m.clone(), community_id.to_string());
        }

        // v2.2.2: Hierarchical sub-clustering (level 1) for larger communities
        let min_for_split = 3;
        if members.len() >= min_for_split * 2 {
            let sub_nodes: Vec<KnowledgeNode> = members
                .iter()
                .filter_map(|id| node_map.get(id.as_str()))
                .cloned()
                .cloned()
                .collect();
            let sub_assignments = greedy_modularity_clustering(&sub_nodes, &adjacency);
            let mut sub_groups: BTreeMap<usize, Vec<String>> = BTreeMap::new();
            for (nid, local_id) in &sub_assignments {
                sub_groups.entry(*local_id).or_default().push(nid.clone());
            }
            // Only keep if we actually split (more than 1 sub-community)
            if sub_groups.len() > 1 {
                for (sub_id, sub_members) in &sub_groups {
                    let path = format!("{}.{}", community_id, sub_id);
                    let global_sub_id = next_sub_community_id;
                    next_sub_community_id += 1;
                    let mut sub_info =
                        build_community(global_sub_id, sub_members, &graph.edges, &node_map);
                    sub_info.community_path = Some(path.clone());
                    sub_info.level = 1;
                    communities.push(sub_info);
                    for m in sub_members {
                        node_community_path.insert(m.clone(), path.clone());
                    }
                }
            }
        }
    }

    let nodes = graph
        .nodes
        .iter()
        .cloned()
        .map(|mut node| {
            node.community_id = node_community.get(&node.id).copied();
            node
        })
        .collect::<Vec<_>>();
    let mut updated = graph.clone();
    updated.nodes = nodes;
    updated.communities = communities;
    updated.statistics = compute_statistics(&updated.nodes, &updated.edges, &updated.communities);
    updated
}

pub fn project_graph_for_dashboard(
    graph: &KnowledgeGraph,
    max_nodes: usize,
    max_edges: usize,
) -> (KnowledgeGraph, GraphProjection) {
    if graph.nodes.len() <= max_nodes && graph.edges.len() <= max_edges {
        return (
            graph.clone(),
            GraphProjection {
                total_nodes: graph.nodes.len(),
                total_edges: graph.edges.len(),
                returned_nodes: graph.nodes.len(),
                returned_edges: graph.edges.len(),
            },
        );
    }

    // Build adjacency index so we can pull in edge-neighbors of selected nodes.
    // Without this, alphabetically-sorted snapshots (BTreeMap legacy) would select
    // nodes like cmd:* and episode:* but miss the session:* nodes that connect them,
    // resulting in 0 edges surviving the filter.
    let mut neighbors: HashMap<&str, Vec<&str>> = HashMap::new();
    for edge in &graph.edges {
        neighbors
            .entry(edge.source.as_str())
            .or_default()
            .push(edge.target.as_str());
        neighbors
            .entry(edge.target.as_str())
            .or_default()
            .push(edge.source.as_str());
    }

    let mut selected = indexmap::IndexSet::new();
    let mut per_bucket = HashMap::from([
        ("files".to_string(), 0usize),
        ("docs".to_string(), 0usize),
        ("symbols".to_string(), 0usize),
    ]);
    let per_bucket_cap = (max_nodes / 3).max(1);

    // Phase 1: fill per-bucket seeds (matching Node.js)
    for node in &graph.nodes {
        let bucket = node_bucket(&node.node_type);
        let count = per_bucket.get(bucket).copied().unwrap_or_default();
        if count >= per_bucket_cap {
            continue;
        }
        selected.insert(node.id.clone());
        per_bucket.insert(bucket.to_string(), count + 1);
    }

    // Phase 2: expand selection with edge-neighbors of already-selected nodes,
    // so that edges between seed nodes and their neighbors survive projection.
    let seeds: Vec<String> = selected.iter().cloned().collect();
    for seed_id in &seeds {
        if selected.len() >= max_nodes {
            break;
        }
        if let Some(nbrs) = neighbors.get(seed_id.as_str()) {
            for &nbr in nbrs {
                if selected.len() >= max_nodes {
                    break;
                }
                selected.insert(nbr.to_string());
            }
        }
    }

    // Phase 3: fill remaining slots from the graph (matching Node.js)
    for node in &graph.nodes {
        if selected.len() >= max_nodes {
            break;
        }
        selected.insert(node.id.clone());
    }

    let nodes = graph
        .nodes
        .iter()
        .filter(|node| selected.contains(&node.id))
        .cloned()
        .collect::<Vec<_>>();
    let edges = graph
        .edges
        .iter()
        .filter(|edge| selected.contains(&edge.source) && selected.contains(&edge.target))
        .take(max_edges)
        .cloned()
        .collect::<Vec<_>>();
    let communities = graph
        .communities
        .iter()
        .filter(|community| {
            community
                .representative_nodes
                .iter()
                .any(|node_id| selected.contains(node_id))
                || community
                    .bridge_nodes
                    .iter()
                    .any(|node_id| selected.contains(node_id))
        })
        .cloned()
        .collect::<Vec<_>>();

    (
        KnowledgeGraph {
            version: graph.version.clone(),
            generated_at: graph.generated_at.clone(),
            project_id: graph.project_id,
            nodes: nodes.clone(),
            edges: edges.clone(),
            communities,
            statistics: graph.statistics.clone(),
        },
        GraphProjection {
            total_nodes: graph.nodes.len(),
            total_edges: graph.edges.len(),
            returned_nodes: nodes.len(),
            returned_edges: edges.len(),
        },
    )
}

pub fn generate_knowledge_report(graph: &KnowledgeGraph) -> String {
    let node_map: HashMap<&str, &KnowledgeNode> =
        graph.nodes.iter().map(|n| (n.id.as_str(), n)).collect();

    // Calculate hub nodes (by in+out degree)
    let mut in_degree = HashMap::<&str, usize>::new();
    let mut out_degree = HashMap::<&str, usize>::new();
    for edge in &graph.edges {
        *out_degree.entry(&edge.source).or_default() += 1;
        *in_degree.entry(&edge.target).or_default() += 1;
    }

    struct HubNode<'a> {
        label: &'a str,
        node_type: &'a str,
        in_degree: usize,
        out_degree: usize,
        degree: usize,
    }

    let mut hub_nodes: Vec<HubNode> = graph
        .nodes
        .iter()
        .map(|n| {
            let ind = in_degree.get(n.id.as_str()).copied().unwrap_or(0);
            let outd = out_degree.get(n.id.as_str()).copied().unwrap_or(0);
            HubNode {
                label: &n.label,
                node_type: &n.node_type,
                in_degree: ind,
                out_degree: outd,
                degree: ind + outd,
            }
        })
        .filter(|h| h.degree > 1)
        .collect();
    hub_nodes.sort_by_key(|h| std::cmp::Reverse(h.degree));
    hub_nodes.truncate(10);

    let pct = |n: usize, total: usize| -> String {
        if total == 0 {
            "0%".to_string()
        } else {
            format!("{:.0}%", n as f64 / total as f64 * 100.0)
        }
    };

    // Evidence distribution one-liner (graphify style)
    let ed = &graph.statistics.evidence_distribution;
    let total_evidence = ed.extracted + ed.inferred + ed.ambiguous;
    let extraction_line = if total_evidence > 0 {
        format!(
            "- Extraction: {} EXTRACTED \u{00b7} {} INFERRED \u{00b7} {} AMBIGUOUS",
            pct(ed.extracted, total_evidence),
            pct(ed.inferred, total_evidence),
            pct(ed.ambiguous, total_evidence),
        )
    } else {
        "- Extraction: no edges".to_string()
    };

    // Node type distribution
    let mut type_counts = HashMap::<&str, usize>::new();
    for n in &graph.nodes {
        *type_counts.entry(&n.node_type).or_default() += 1;
    }
    let mut type_entries: Vec<_> = type_counts.iter().collect();
    type_entries.sort_by_key(|(_, c)| std::cmp::Reverse(**c));

    // Edge relation distribution
    let mut rel_counts = HashMap::<&str, usize>::new();
    for e in &graph.edges {
        *rel_counts.entry(&e.relation).or_default() += 1;
    }
    let mut rel_entries: Vec<_> = rel_counts.iter().collect();
    rel_entries.sort_by_key(|(_, c)| std::cmp::Reverse(**c));

    let mut lines = vec![
        format!("# Knowledge Report  ({})", &graph.generated_at[..10]),
        String::new(),
        "## Summary".to_string(),
        format!(
            "- {} nodes \u{00b7} {} edges \u{00b7} {} communities detected",
            graph.statistics.node_count,
            graph.statistics.edge_count,
            graph.statistics.community_count
        ),
        extraction_line,
        format!(
            "- Avg degree: {} \u{00b7} Density: {} \u{00b7} Isolated: {}",
            graph.statistics.avg_degree, graph.statistics.density, graph.statistics.isolated_nodes
        ),
        String::new(),
    ];

    // Node types
    if !type_entries.is_empty() {
        lines.push("### Node Types".to_string());
        lines.push(String::new());
        for (ntype, count) in &type_entries {
            lines.push(format!("- **{}**: {}", ntype, count));
        }
        lines.push(String::new());
    }

    // Edge relations
    if !rel_entries.is_empty() {
        lines.push("### Edge Relations".to_string());
        lines.push(String::new());
        for (rel, count) in &rel_entries {
            lines.push(format!("- **{}**: {}", rel, count));
        }
        lines.push(String::new());
    }

    // God Nodes (graphify style: numbered list with edge count)
    lines.push("## God Nodes (most connected)".to_string());
    lines.push(String::new());
    if hub_nodes.is_empty() {
        lines.push("No hub nodes detected.".to_string());
    } else {
        for (i, h) in hub_nodes.iter().enumerate() {
            lines.push(format!(
                "{}. `{}` ({}) - {} edges",
                i + 1,
                h.label,
                h.node_type,
                h.degree
            ));
        }
    }
    lines.push(String::new());

    // Communities (graphify style)
    if !graph.communities.is_empty() {
        lines.push("## Communities".to_string());
        lines.push(String::new());

        // Show level-0 communities first, then level-1
        let mut level0: Vec<_> = graph.communities.iter().filter(|c| c.level == 0).collect();
        let level1: Vec<_> = graph.communities.iter().filter(|c| c.level == 1).collect();
        level0.sort_by_key(|c| std::cmp::Reverse(c.node_count));

        for comm in &level0 {
            let label = comm.label.as_deref().unwrap_or("Unnamed");
            let rep_labels: Vec<String> = comm
                .representative_nodes
                .iter()
                .filter_map(|id| node_map.get(id.as_str()).map(|n| format!("`{}`", n.label)))
                .collect();
            let bridge_labels: Vec<String> = comm
                .bridge_nodes
                .iter()
                .filter_map(|id| node_map.get(id.as_str()).map(|n| format!("`{}`", n.label)))
                .collect();
            lines.push(format!(
                "### Community {} - \"{}\"",
                comm.community_id, label
            ));
            lines.push(format!("Cohesion: {:.2}", comm.cohesion_score));
            if !rep_labels.is_empty() {
                lines.push(format!(
                    "Nodes ({}): {}",
                    comm.node_count,
                    rep_labels.join(", ")
                ));
            } else {
                lines.push(format!("Nodes: {}", comm.node_count));
            }
            if !bridge_labels.is_empty() {
                lines.push(format!("Bridge nodes: {}", bridge_labels.join(", ")));
            }
            lines.push(String::new());
        }

        if !level1.is_empty() {
            lines.push(format!(
                "_+ {} level-1 sub-communities (omitted for brevity)_",
                level1.len()
            ));
            lines.push(String::new());
        }
    }

    lines.push("---".to_string());
    lines.push(format!(
        "_Report generated by chum-mem v{} \u{00b7} Project: {}_",
        graph.version, graph.project_id
    ));

    lines.join("\n")
}

pub fn to_node_link_json(graph: &KnowledgeGraph) -> String {
    serde_json::to_string_pretty(&json!({
        "directed": true,
        "multigraph": false,
        "graph": {
            "version": graph.version,
            "generatedAt": graph.generated_at,
            "projectId": graph.project_id,
        },
        "nodes": graph.nodes.iter().map(|node| {
            let mut value = json!({
                "id": node.id,
                "label": node.label,
                "type": node.node_type,
                "sourceType": node.source_type,
                "sourceId": node.source_id,
                "community": node.community_id,
            });
            if let Value::Object(ref mut object) = value
                && let Value::Object(metadata) = node.metadata.clone()
            {
                for (key, entry) in metadata {
                    object.insert(key, entry);
                }
            }
            value
        }).collect::<Vec<_>>(),
        "links": graph.edges.iter().map(|edge| {
            let mut value = json!({
                "source": edge.source,
                "target": edge.target,
                "relation": edge.relation,
                "evidence": edge.evidence,
                "weight": edge.weight,
            });
            if let Some(source_file) = &edge.source_file
                && let Value::Object(ref mut object) = value
            {
                object.insert("source_file".to_string(), Value::String(source_file.clone()));
            }
            if let Value::Object(ref mut object) = value
                && let Value::Object(metadata) = edge.metadata.clone()
            {
                for (key, entry) in metadata {
                    object.insert(key, entry);
                }
            }
            value
        }).collect::<Vec<_>>(),
        "communities": graph.communities,
        "statistics": graph.statistics,
    }))
    .unwrap_or_else(|_| "{}".to_string())
}

pub fn run_knowledge_query(
    graph: &KnowledgeGraph,
    query: &str,
    node_id: Option<&str>,
    target_node_id: Option<&str>,
    text: Option<&str>,
    depth: usize,
) -> GraphQueryResponse {
    match query {
        "hub_nodes" => {
            let mut degree = HashMap::<&str, usize>::new();
            for node in &graph.nodes {
                degree.insert(&node.id, 0);
            }
            for edge in &graph.edges {
                *degree.entry(&edge.source).or_default() += 1;
                *degree.entry(&edge.target).or_default() += 1;
            }
            let hubs = {
                let mut nodes: Vec<_> = graph
                    .nodes
                    .iter()
                    .filter(|node| {
                        let deg = *degree.get(node.id.as_str()).unwrap_or(&0);
                        // v2.2.2: Filter — only domain_hub and central_file types
                        let hub_type = classify_hub_type(node, deg);
                        hub_type == "domain_hub" || hub_type == "central_file"
                    })
                    .cloned()
                    .collect();
                nodes.sort_by_key(|node| {
                    std::cmp::Reverse(*degree.get(node.id.as_str()).unwrap_or(&0))
                });
                nodes.into_iter().take(10).collect::<Vec<_>>()
            };
            GraphQueryResponse {
                nodes: hubs,
                edges: Vec::new(),
                metadata: json!({ "query": query }),
            }
        }
        "neighbors" => {
            let Some(seed) = node_id else {
                return GraphQueryResponse {
                    nodes: Vec::new(),
                    edges: Vec::new(),
                    metadata: json!({ "error": "missing nodeId" }),
                };
            };
            collect_subgraph(graph, &[seed.to_string()], depth)
        }
        "shortest_path" => {
            let (Some(source), Some(target)) = (node_id, target_node_id) else {
                return GraphQueryResponse {
                    nodes: Vec::new(),
                    edges: Vec::new(),
                    metadata: json!({ "error": "missing node ids" }),
                };
            };
            shortest_path(graph, source, target)
        }
        "goal_directed" => {
            let Some(query_text) = text else {
                return GraphQueryResponse {
                    nodes: Vec::new(),
                    edges: Vec::new(),
                    metadata: json!({ "error": "missing text" }),
                };
            };
            let max_hops = depth.max(1);
            goal_directed_search(graph, node_id, query_text, max_hops)
        }
        "communities" => {
            let nodes: Vec<KnowledgeNode> = graph
                .nodes
                .iter()
                .filter(|node| node.community_id.is_some())
                .cloned()
                .collect();
            let node_set: HashSet<&str> = nodes.iter().map(|n| n.id.as_str()).collect();
            let edges: Vec<KnowledgeEdge> = graph
                .edges
                .iter()
                .filter(|edge| {
                    node_set.contains(edge.source.as_str())
                        || node_set.contains(edge.target.as_str())
                })
                .cloned()
                .collect();
            GraphQueryResponse {
                nodes,
                edges,
                metadata: json!({ "communities": graph.communities }),
            }
        }
        "search" => {
            let search = text.unwrap_or("").to_lowercase();
            let seed = node_id.unwrap_or_default();

            // Match Node.js: use regex [a-z0-9_./:-]+ for tokenization
            let search_tokens: Vec<String> = {
                use regex::Regex;
                use std::sync::LazyLock;
                static SEARCH_TOKEN_RE: LazyLock<Regex> =
                    LazyLock::new(|| Regex::new(r"[a-z0-9_./:+-]+").unwrap());
                SEARCH_TOKEN_RE
                    .find_iter(&search)
                    .map(|m| m.as_str().to_string())
                    .collect()
            };

            // Score nodes with path-, symbol-, and section-aware heuristics.
            let mut scored: Vec<(f64, &KnowledgeNode)> = graph
                .nodes
                .iter()
                .filter_map(|node| {
                    if !seed.is_empty() && node.id == seed {
                        return Some((1000.0, node));
                    }
                    if search.is_empty() || search_tokens.is_empty() {
                        return None;
                    }
                    let score = score_search_node(node, &search, &search_tokens);

                    if score > 0.0 {
                        Some((score, node))
                    } else {
                        None
                    }
                })
                .collect();

            // Sort by score, then by label for tiebreaking (matching Node.js)
            scored.sort_by(|a, b| {
                b.0.partial_cmp(&a.0)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| a.1.label.cmp(&b.1.label))
            });
            let nodes = diversify_scored_nodes(scored, 10);
            let selected: HashSet<String> = nodes.iter().map(|node| node.id.clone()).collect();
            let edges = graph
                .edges
                .iter()
                .filter(|edge| selected.contains(&edge.source) || selected.contains(&edge.target))
                .cloned()
                .collect::<Vec<_>>();
            GraphQueryResponse {
                nodes,
                edges,
                metadata: json!({ "query": query }),
            }
        }
        _ => GraphQueryResponse {
            nodes: Vec::new(),
            edges: Vec::new(),
            metadata: json!({ "error": "unsupported query" }),
        },
    }
}

fn score_search_node(node: &KnowledgeNode, search: &str, search_tokens: &[String]) -> f64 {
    let haystack = format!("{} {} {}", node.id, node.label, node.metadata).to_lowercase();
    let id_lower = node.id.to_lowercase();
    let label_lower = node.label.to_lowercase();
    let source_id_lower = node.source_id.to_lowercase();
    let full_path = metadata_str(&node.metadata, "fullPath")
        .map(|value| value.to_lowercase())
        .or_else(|| {
            Some(source_id_lower.clone())
                .filter(|_| node.node_type == "file" || node.node_type == "document")
        });
    let source_file = metadata_str(&node.metadata, "sourceFile").map(|value| value.to_lowercase());
    let basename = metadata_str(&node.metadata, "basename")
        .map(|value| value.to_lowercase())
        .or_else(|| {
            full_path
                .as_deref()
                .and_then(|path| Path::new(path).file_name().and_then(|value| value.to_str()))
                .map(|value| value.to_lowercase())
        });
    let extension = metadata_str(&node.metadata, "extension")
        .map(|value| value.to_lowercase())
        .or_else(|| {
            basename
                .as_deref()
                .and_then(|value| Path::new(value).extension().and_then(|ext| ext.to_str()))
                .map(|value| value.to_lowercase())
        });
    let symbol_kind = metadata_str(&node.metadata, "symbolKind").map(|value| value.to_lowercase());
    let import_source =
        metadata_str(&node.metadata, "importSource").map(|value| value.to_lowercase());
    let heading = metadata_str(&node.metadata, "heading")
        .map(|value| value.to_lowercase())
        .or_else(|| {
            Some(label_lower.clone()).filter(|_| {
                matches!(
                    node.node_type.as_str(),
                    "section" | "decision" | "task" | "rationale"
                )
            })
        });
    let rationale_tag = metadata_str(&node.metadata, "tag").map(|value| value.to_lowercase());
    let rationale_body = metadata_str(&node.metadata, "body").map(|value| value.to_lowercase());
    let is_file_or_doc = node.node_type == "file" || node.node_type == "document";
    let is_symbol = node.node_type == "symbol";
    let looks_like_path = looks_like_path_query(search);
    let looks_like_identifier = looks_like_identifier_query(search);
    let mut score = 0.0_f64;

    if is_file_or_doc {
        if id_lower == format!("file:{search}")
            || source_id_lower == search
            || full_path.as_deref() == Some(search)
            || source_file.as_deref() == Some(search)
        {
            score += 260.0;
        }
        if basename.as_deref() == Some(search) {
            score += 220.0;
        }
        if full_path
            .as_deref()
            .is_some_and(|path| path.ends_with(search))
        {
            score += 170.0;
        }
        if basename
            .as_deref()
            .is_some_and(|value| value.starts_with(search))
        {
            score += 140.0;
        }
        if full_path
            .as_deref()
            .is_some_and(|path| path.contains(search))
        {
            score += 110.0;
        }
        if extension
            .as_deref()
            .is_some_and(|value| value == search.trim_start_matches('.'))
        {
            score += 40.0;
        }
    } else if looks_like_path {
        score -= 48.0;
    }

    if is_symbol {
        if label_lower == search {
            score += 240.0;
        } else if label_lower.starts_with(search) {
            score += 180.0;
        } else if label_lower.contains(search) {
            score += 110.0;
        }
        if symbol_kind.as_deref() == Some(search) {
            score += 36.0;
        }
        if looks_like_identifier {
            score += 18.0;
        }
    }

    if node.node_type == "module" {
        if label_lower == search || import_source.as_deref() == Some(search) {
            score += 210.0;
        } else if label_lower.starts_with(search)
            || import_source
                .as_deref()
                .is_some_and(|value| value.starts_with(search))
        {
            score += 145.0;
        } else if label_lower.contains(search)
            || import_source
                .as_deref()
                .is_some_and(|value| value.contains(search))
        {
            score += 90.0;
        }
    }

    if matches!(
        node.node_type.as_str(),
        "section" | "decision" | "task" | "rationale" | "document"
    ) {
        if heading.as_deref() == Some(search) || label_lower == search {
            score += 190.0;
        } else if heading
            .as_deref()
            .is_some_and(|value| value.starts_with(search))
            || label_lower.starts_with(search)
        {
            score += 135.0;
        } else if heading
            .as_deref()
            .is_some_and(|value| value.contains(search))
            || label_lower.contains(search)
        {
            score += 92.0;
        }
    }

    if rationale_tag.as_deref() == Some(search) {
        score += 170.0;
    }
    if rationale_body
        .as_deref()
        .is_some_and(|value| value.contains(search))
    {
        score += 80.0;
    }

    for token in search_tokens {
        if basename.as_deref() == Some(token.as_str()) || label_lower == *token {
            score += 18.0;
        }
        if full_path
            .as_deref()
            .is_some_and(|value| value.contains(token))
        {
            score += 11.0;
        }
        if heading
            .as_deref()
            .is_some_and(|value| value.contains(token))
            || rationale_body
                .as_deref()
                .is_some_and(|value| value.contains(token))
        {
            score += 9.0;
        }
        if haystack.contains(token.as_str()) {
            score += if token.len() > 3 { 2.0 } else { 1.0 };
        }
    }

    if haystack == search {
        score += 12.0;
    } else if haystack.contains(search) {
        score += 4.0;
    }

    if looks_like_path && is_file_or_doc {
        score += 24.0;
    }
    if path_is_noisy(
        full_path
            .as_deref()
            .or(source_file.as_deref())
            .unwrap_or_default(),
    ) {
        score -= 24.0;
    }

    score
}

fn diversify_scored_nodes(scored: Vec<(f64, &KnowledgeNode)>, limit: usize) -> Vec<KnowledgeNode> {
    let mut selected = Vec::new();
    let mut per_source = HashMap::<String, usize>::new();

    for (_, node) in scored {
        let Some(key) = source_diversity_key(node) else {
            selected.push(node.clone());
            if selected.len() >= limit {
                break;
            }
            continue;
        };
        let counter = per_source.entry(key).or_default();
        if *counter >= 2 {
            continue;
        }
        *counter += 1;
        selected.push(node.clone());
        if selected.len() >= limit {
            break;
        }
    }

    selected
}

fn source_diversity_key(node: &KnowledgeNode) -> Option<String> {
    if node.node_type == "file" || node.node_type == "document" {
        return None;
    }
    metadata_str(&node.metadata, "sourceFile")
        .map(ToOwned::to_owned)
        .or_else(|| Some(node.source_id.clone()).filter(|value| !value.is_empty()))
}

fn looks_like_path_query(search: &str) -> bool {
    search.contains('/') || search.contains('\\') || search.contains('.')
}

fn looks_like_identifier_query(search: &str) -> bool {
    !search.contains(' ')
        && search
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | ':' | '-' | '.'))
}

fn path_is_noisy(path: &str) -> bool {
    let lowered = path.to_lowercase();
    [
        "node_modules/",
        "dist/",
        "build/",
        ".next/",
        "coverage/",
        "target/",
        "vendor/",
    ]
    .iter()
    .any(|segment| lowered.contains(segment))
}

fn metadata_str<'a>(metadata: &'a Value, key: &str) -> Option<&'a str> {
    metadata.get(key).and_then(Value::as_str)
}

pub fn to_persistable_memory_edge(
    edge: &KnowledgeEdge,
) -> Option<(Uuid, Uuid, String, String, f64, Value)> {
    let from = edge.source.strip_prefix("memory:")?;
    let to = edge.target.strip_prefix("memory:")?;
    let from_id = Uuid::parse_str(from).ok()?;
    let to_id = Uuid::parse_str(to).ok()?;
    let allowed = [
        "caused_by",
        "depends_on",
        "supersedes",
        "contradicts",
        "confirms",
        "derived_from",
        "related_to",
        "from_same_session",
    ];
    if !allowed.contains(&edge.relation.as_str()) {
        return None;
    }
    Some((
        from_id,
        to_id,
        edge.relation.clone(),
        edge.evidence.clone(),
        edge.weight,
        edge.metadata.clone(),
    ))
}

fn extract_structural(
    session_id: Uuid,
    events: &[SessionEventRecord],
    nodes: &mut Vec<KnowledgeNode>,
    edges: &mut Vec<KnowledgeEdge>,
    seen_nodes: &mut HashSet<String>,
) {
    let mut file_change_files: Vec<String> = Vec::new();

    for event in events {
        // Match Node.js: only extract file nodes for file_change events (switch case)
        match event.event_type {
            chum_mem_contracts::CanonicalEventType::FileChange => {
                if let Some(file_path) = event.payload.file_path.as_deref() {
                    let node_id = format!("file:{file_path}");
                    let mut file_meta = json!({ "fullPath": file_path });
                    if let Some(diff_stat) = &event.payload.diff_stat {
                        file_meta.as_object_mut().unwrap().insert(
                            "diffStat".to_string(),
                            json!({ "added": diff_stat.added, "deleted": diff_stat.deleted }),
                        );
                    }
                    add_node(
                        nodes,
                        seen_nodes,
                        KnowledgeNode {
                            id: node_id.clone(),
                            label: file_path
                                .rsplit('/')
                                .next()
                                .unwrap_or(file_path)
                                .to_string(),
                            node_type: "file".to_string(),
                            source_type: "session_event".to_string(),
                            source_id: event.id.to_string(),
                            metadata: file_meta,
                            community_id: None,
                        },
                    );
                    let mut edge_meta =
                        json!({ "eventId": event.id, "eventTime": event.created_at });
                    if let Some(diff_stat) = &event.payload.diff_stat {
                        edge_meta.as_object_mut().unwrap().insert(
                            "linesChanged".to_string(),
                            json!(diff_stat.added + diff_stat.deleted),
                        );
                    }
                    edges.push(KnowledgeEdge {
                        source: format!("session:{session_id}"),
                        target: node_id.clone(),
                        relation: "modifies".to_string(),
                        evidence: "extracted".to_string(),
                        weight: 1.0,
                        source_file: None,
                        metadata: edge_meta.clone(),
                    });
                    file_change_files.push(file_path.to_string());

                    // v2.2.2: Cross-layer edge — file → session (reverse of modifies)
                    edges.push(KnowledgeEdge {
                        source: node_id,
                        target: format!("session:{session_id}"),
                        relation: "touched_by".to_string(),
                        evidence: "extracted".to_string(),
                        weight: 1.0,
                        source_file: None,
                        metadata: edge_meta,
                    });
                }
                continue;
            }
            chum_mem_contracts::CanonicalEventType::TestResult => {
                // Match Node.js extractTestNode
                let node_id = format!("test:{}", event.id);
                let passed = event.payload.exit_code.map(|c| c == 0).unwrap_or(false);
                add_node(
                    nodes,
                    seen_nodes,
                    KnowledgeNode {
                        id: node_id.clone(),
                        label: event
                            .payload
                            .tool_name
                            .clone()
                            .unwrap_or_else(|| format!("Test {}", &event.id.to_string()[..8])),
                        node_type: "test".to_string(),
                        source_type: "session_event".to_string(),
                        source_id: event.id.to_string(),
                        metadata: json!({ "passed": passed, "exitCode": event.payload.exit_code }),
                        community_id: None,
                    },
                );
                edges.push(KnowledgeEdge {
                    source: format!("session:{session_id}"),
                    target: node_id,
                    relation: "produces".to_string(),
                    evidence: "extracted".to_string(),
                    weight: 1.0,
                    source_file: None,
                    metadata: json!({ "eventId": event.id, "eventTime": event.created_at }),
                });
                continue;
            }
            _ => {}
        }

        if let Some(tool_name) = event.payload.tool_name.as_deref() {
            let node_id = format!("tool:{tool_name}");
            add_node(
                nodes,
                seen_nodes,
                KnowledgeNode {
                    id: node_id.clone(),
                    label: tool_name.to_string(),
                    node_type: "tool".to_string(),
                    source_type: "session_event".to_string(),
                    source_id: event.id.to_string(),
                    metadata: json!({}),
                    community_id: None,
                },
            );
            edges.push(KnowledgeEdge {
                source: format!("session:{session_id}"),
                target: node_id.clone(),
                relation: if event.event_type == chum_mem_contracts::CanonicalEventType::ToolCall {
                    "calls".to_string()
                } else {
                    "produces".to_string()
                },
                evidence: "extracted".to_string(),
                weight: 1.0,
                source_file: None,
                metadata: json!({ "eventId": event.id, "eventTime": event.created_at }),
            });
            // Add tool -> file edge when filePath present (matching Node.js)
            if let Some(file_path) = event.payload.file_path.as_deref() {
                edges.push(KnowledgeEdge {
                    source: node_id,
                    target: format!("file:{file_path}"),
                    relation: "modifies".to_string(),
                    evidence: "extracted".to_string(),
                    weight: 1.0,
                    source_file: None,
                    metadata: json!({ "eventId": event.id, "eventTime": event.created_at }),
                });
            }
        }

        if let Some(command) = event.payload.command.as_deref() {
            // Match Node.js: take first 3 tokens, truncate to 60 chars, lowercase
            let normalized: String = command
                .split_whitespace()
                .take(3)
                .collect::<Vec<_>>()
                .join(" ")
                .to_lowercase()
                .chars()
                .take(60)
                .collect();
            let node_id = format!("cmd:{normalized}");
            add_node(
                nodes,
                seen_nodes,
                KnowledgeNode {
                    id: node_id.clone(),
                    label: normalized,
                    node_type: "command".to_string(),
                    source_type: "session_event".to_string(),
                    source_id: event.id.to_string(),
                    metadata: json!({ "fullCommand": command }),
                    community_id: None,
                },
            );
            edges.push(KnowledgeEdge {
                source: format!("session:{session_id}"),
                target: node_id,
                relation: "calls".to_string(),
                evidence: "extracted".to_string(),
                weight: 1.0,
                source_file: None,
                metadata: json!({ "eventId": event.id, "exitCode": event.payload.exit_code, "eventTime": event.created_at }),
            });
        }

        // Error detection: only for eventType === 'error' (matching Node.js structural-extractor)
        if event.event_type == chum_mem_contracts::CanonicalEventType::Error {
            let message = event_text(event);
            let signature = normalize_error_signature(&message.to_lowercase());
            let node_id = format!("error:{signature}");
            add_node(
                nodes,
                seen_nodes,
                KnowledgeNode {
                    id: node_id.clone(),
                    label: message.chars().take(80).collect(),
                    node_type: "error".to_string(),
                    source_type: "session_event".to_string(),
                    source_id: event.id.to_string(),
                    metadata: json!({
                        "signature": signature,
                        "fullMessage": message.chars().take(500).collect::<String>(),
                    }),
                    community_id: None,
                },
            );
            edges.push(KnowledgeEdge {
                source: node_id.clone(),
                target: format!("session:{session_id}"),
                relation: "caused_by".to_string(),
                evidence: "extracted".to_string(),
                weight: 1.0,
                source_file: None,
                metadata: json!({ "eventId": event.id, "eventTime": event.created_at }),
            });
            // Add error -> file edge when filePath present (matching Node.js)
            if let Some(file_path) = event.payload.file_path.as_deref() {
                edges.push(KnowledgeEdge {
                    source: node_id,
                    target: format!("file:{file_path}"),
                    relation: "references".to_string(),
                    evidence: "extracted".to_string(),
                    weight: 1.0,
                    source_file: None,
                    metadata: json!({ "eventId": event.id }),
                });
            }
        }
    }

    // Add co-occurrence edges for file_change files (matching Node.js)
    let unique_files: Vec<String> = {
        let mut seen = HashSet::new();
        file_change_files
            .into_iter()
            .filter(|f| seen.insert(f.clone()))
            .collect()
    };
    for i in 0..unique_files.len() {
        for j in (i + 1)..unique_files.len() {
            edges.push(KnowledgeEdge {
                source: format!("file:{}", unique_files[i]),
                target: format!("file:{}", unique_files[j]),
                relation: "co_occurs".to_string(),
                evidence: "extracted".to_string(),
                weight: 1.0,
                source_file: None,
                metadata: json!({ "context": "same_session" }),
            });
        }
    }
}

fn extract_semantic(
    session_id: Uuid,
    episodes: &[SessionEpisodeDraft],
    memories: &[MemoryNodeInput],
    prior_session_ids: &[Uuid],
    events: &[SessionEventRecord],
    nodes: &mut Vec<KnowledgeNode>,
    edges: &mut Vec<KnowledgeEdge>,
    seen_nodes: &mut HashSet<String>,
) {
    for episode in episodes {
        let node_id = format!("episode:{session_id}:{}", episode.episode_ordinal);
        add_node(
            nodes,
            seen_nodes,
            KnowledgeNode {
                id: node_id.clone(),
                label: episode.title.chars().take(80).collect(),
                node_type: "episode".to_string(),
                source_type: "episode".to_string(),
                source_id: format!("{session_id}:{}", episode.episode_ordinal),
                metadata: json!({
                    "episodeType": episode.episode_type,
                    "startedAt": episode.started_at,
                    "endedAt": episode.ended_at,
                    "eventCount": episode.provenance_event_ids.len(),
                }),
                community_id: None,
            },
        );
        edges.push(KnowledgeEdge {
            source: format!("session:{session_id}"),
            target: node_id.clone(),
            relation: "contains".to_string(),
            evidence: "extracted".to_string(),
            weight: 1.0,
            source_file: None,
            metadata: json!({}),
        });
        if episode.episode_ordinal > 1 {
            edges.push(KnowledgeEdge {
                source: format!("episode:{session_id}:{}", episode.episode_ordinal - 1),
                target: node_id,
                relation: "related_to".to_string(),
                evidence: "extracted".to_string(),
                weight: 0.9,
                source_file: None,
                metadata: json!({ "relation_subtype": "sequential" }),
            });
        }
    }

    for window in episodes.windows(2) {
        let [left, right] = match window {
            [left, right] => [left, right],
            _ => continue,
        };
        let reason = match (left.episode_type.as_str(), right.episode_type.as_str()) {
            ("implementation", "debugging") => Some("debugging_follows_implementation"),
            ("debugging", "implementation") => Some("fix_follows_debugging"),
            _ => None,
        };
        if let Some(reason) = reason {
            let confidence = if reason == "fix_follows_debugging" {
                0.7
            } else {
                0.75
            };
            edges.push(KnowledgeEdge {
                source: format!("episode:{session_id}:{}", left.episode_ordinal),
                target: format!("episode:{session_id}:{}", right.episode_ordinal),
                relation: "caused_by".to_string(),
                evidence: "inferred".to_string(),
                weight: 0.8,
                source_file: None,
                metadata: json!({ "reason": reason, "confidence": confidence }),
            });
        }
    }

    for memory in memories {
        let node_id = format!("memory:{}", memory.id);
        add_node(
            nodes,
            seen_nodes,
            KnowledgeNode {
                id: node_id.clone(),
                label: memory.title.chars().take(80).collect(),
                node_type: memory_node_type(&memory.memory_type).to_string(),
                source_type: "memory".to_string(),
                source_id: memory.id.to_string(),
                metadata: json!({
                    "memoryType": memory.memory_type,
                    "importanceScore": memory.importance_score,
                }),
                community_id: None,
            },
        );
        edges.push(KnowledgeEdge {
            source: format!("session:{session_id}"),
            target: node_id,
            relation: "produces".to_string(),
            evidence: "extracted".to_string(),
            weight: 1.0,
            source_file: None,
            metadata: json!({}),
        });
    }

    if !prior_session_ids.is_empty() {
        // Count unique file paths from events (matching Node.js)
        let file_paths: HashSet<&str> = events
            .iter()
            .filter_map(|e| e.payload.file_path.as_deref())
            .filter(|p| !p.trim().is_empty())
            .collect();
        if !file_paths.is_empty() {
            for prior_id in prior_session_ids {
                edges.push(KnowledgeEdge {
                    source: format!("session:{prior_id}"),
                    target: format!("session:{session_id}"),
                    relation: "continuity".to_string(),
                    evidence: "inferred".to_string(),
                    weight: 0.8,
                    source_file: None,
                    metadata: json!({
                        "reason": "shared_file_context",
                        "sharedFileCount": file_paths.len(),
                    }),
                });
            }
        }
    }

    infer_memory_similarity_edges(memories, edges);

    // v2.2.2: Symbol mention extraction — link memory claims to file nodes
    // Scan claim text for file paths that appear in file_change events
    let touched_files: HashSet<&str> = events
        .iter()
        .filter_map(|e| e.payload.file_path.as_deref())
        .filter(|p| !p.trim().is_empty())
        .collect();
    if !touched_files.is_empty() {
        for memory in memories {
            let text = format!("{} {}", memory.title, memory.content);
            for file_path in &touched_files {
                // Match file basename or full relative path in claim text
                let basename = file_path.rsplit('/').next().unwrap_or(file_path);
                if text.contains(basename) || text.contains(*file_path) {
                    edges.push(KnowledgeEdge {
                        source: format!("memory:{}", memory.id),
                        target: format!("file:{file_path}"),
                        relation: "references".to_string(),
                        evidence: "inferred".to_string(),
                        weight: 0.7,
                        source_file: None,
                        metadata: json!({ "matchType": "file_mention" }),
                    });
                }
            }
        }
    }
}

const MIN_SHARED_TOKENS: usize = 4;
const AMBIGUOUS_SIMILARITY_THRESHOLD: f64 = 0.28;
const INFERRED_SIMILARITY_THRESHOLD: f64 = 0.5;
const DISTINCTIVE_SHARED_TOKEN_MIN: usize = 2;

fn infer_memory_similarity_edges(memories: &[MemoryNodeInput], edges: &mut Vec<KnowledgeEdge>) {
    add_episode_companion_edges(memories, edges);
    let mut candidates = Vec::<(usize, usize, f64, Vec<String>)>::new();
    for left in 0..memories.len() {
        for right in (left + 1)..memories.len() {
            let a = &memories[left];
            let b = &memories[right];

            // Classify relationship (matching Node.js classifyMemoryRelationship)
            let policy = classify_memory_relationship(a, b);
            if policy == "skip" {
                continue;
            }

            // Use content-only for both shared tokens and similarity (matching Node.js)
            let analysis = analyze_token_similarity(&a.content, &b.content);

            if analysis.shared_tokens.len() < MIN_SHARED_TOKENS
                || analysis.similarity < AMBIGUOUS_SIMILARITY_THRESHOLD
            {
                continue;
            }

            if policy == "same_episode_only"
                && (analysis.similarity < INFERRED_SIMILARITY_THRESHOLD
                    || count_distinctive_tokens(&analysis.shared_tokens)
                        < DISTINCTIVE_SHARED_TOKEN_MIN)
            {
                continue;
            }

            if policy == "strong_only" && analysis.similarity < INFERRED_SIMILARITY_THRESHOLD {
                continue;
            }

            if analysis.similarity < INFERRED_SIMILARITY_THRESHOLD {
                continue;
            }

            candidates.push((left, right, analysis.similarity, analysis.shared_tokens));
        }
    }

    candidates.sort_by(|left, right| {
        right
            .2
            .partial_cmp(&left.2)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut degree = HashMap::<usize, usize>::new();
    for (left, right, similarity, shared) in candidates {
        if degree.get(&left).copied().unwrap_or_default() >= 2
            || degree.get(&right).copied().unwrap_or_default() >= 2
        {
            continue;
        }
        degree
            .entry(left)
            .and_modify(|value| *value += 1)
            .or_insert(1);
        degree
            .entry(right)
            .and_modify(|value| *value += 1)
            .or_insert(1);
        let weight = (0.6 + similarity * 0.8).min(1.0);
        edges.push(KnowledgeEdge {
            source: format!("memory:{}", memories[left].id),
            target: format!("memory:{}", memories[right].id),
            relation: "related_to".to_string(),
            evidence: "inferred".to_string(),
            weight: (weight * 1000.0).round() / 1000.0,
            source_file: None,
            metadata: json!({
                "reason": "content_similarity",
                "similarity": (similarity * 1000.0).round() / 1000.0,
                "sharedTokenCount": shared.len(),
                "sharedTokens": shared.into_iter().take(6).collect::<Vec<_>>(),
            }),
        });
    }
}

/// Classify memory relationship policy (matching Node.js classifyMemoryRelationship)
fn classify_memory_relationship(a: &MemoryNodeInput, b: &MemoryNodeInput) -> &'static str {
    let a_meta = memory_episode_metadata(&a.metadata);
    let b_meta = memory_episode_metadata(&b.metadata);
    let a_is_episode_derived = a_meta.0.is_some();
    let b_is_episode_derived = b_meta.0.is_some();

    // Same episode derived: both have sessionId+episodeOrdinal and they match
    if a_is_episode_derived && b_is_episode_derived && a_meta == b_meta {
        return "same_episode_only";
    }

    // Both episode-derived but different episodes: skip
    if a_is_episode_derived && b_is_episode_derived {
        return "skip";
    }

    // Bug memories need strong similarity
    if a.memory_type == "bug" || b.memory_type == "bug" {
        return "strong_only";
    }

    "allow_ambiguous"
}

struct TokenAnalysis {
    similarity: f64,
    shared_tokens: Vec<String>,
}

/// Analyze token similarity between two strings (matching Node.js analyzeTokenSimilarity)
fn analyze_token_similarity(a: &str, b: &str) -> TokenAnalysis {
    let tokens_a: HashSet<String> = tokenize(a).into_iter().collect();
    let tokens_b: HashSet<String> = tokenize(b).into_iter().collect();
    if tokens_a.is_empty() || tokens_b.is_empty() {
        return TokenAnalysis {
            similarity: 0.0,
            shared_tokens: Vec::new(),
        };
    }

    let mut shared_tokens: Vec<String> = tokens_a.intersection(&tokens_b).cloned().collect();
    shared_tokens.sort();

    let union = tokens_a.len() + tokens_b.len() - shared_tokens.len();
    let similarity = if union == 0 {
        0.0
    } else {
        shared_tokens.len() as f64 / union as f64
    };

    TokenAnalysis {
        similarity,
        shared_tokens,
    }
}

/// Count distinctive tokens (matching Node.js countDistinctiveTokens)
fn count_distinctive_tokens(tokens: &[String]) -> usize {
    tokens
        .iter()
        .filter(|t| t.len() >= 6 || t.chars().any(|c| c.is_ascii_digit()))
        .count()
}

fn add_episode_companion_edges(memories: &[MemoryNodeInput], edges: &mut Vec<KnowledgeEdge>) {
    for left in 0..memories.len() {
        for right in (left + 1)..memories.len() {
            let left_meta = memory_episode_metadata(&memories[left].metadata);
            let right_meta = memory_episode_metadata(&memories[right].metadata);
            if left_meta.0.is_none()
                || left_meta != right_meta
                || !is_companion_memory_pair(
                    &memories[left].memory_type,
                    &memories[right].memory_type,
                )
            {
                continue;
            }
            edges.push(KnowledgeEdge {
                source: format!("memory:{}", memories[left].id),
                target: format!("memory:{}", memories[right].id),
                relation: "related_to".to_string(),
                evidence: "inferred".to_string(),
                weight: 0.92,
                source_file: None,
                metadata: json!({
                    "reason": "same_episode_derivation",
                    "sessionId": left_meta.0,
                    "episodeOrdinal": left_meta.1,
                }),
            });
        }
    }
}

fn add_node(nodes: &mut Vec<KnowledgeNode>, seen: &mut HashSet<String>, node: KnowledgeNode) {
    if seen.insert(node.id.clone()) {
        nodes.push(node);
    }
}

fn dedupe_edges(edges: Vec<KnowledgeEdge>) -> Vec<KnowledgeEdge> {
    let mut map = indexmap::IndexMap::<(String, String, String), KnowledgeEdge>::new();
    for edge in edges {
        let key = (
            edge.source.clone(),
            edge.target.clone(),
            edge.relation.clone(),
        );
        match map.get_mut(&key) {
            Some(existing) => {
                let existing_rank = evidence_rank(&existing.evidence);
                let new_rank = evidence_rank(&edge.evidence);
                if new_rank > existing_rank {
                    // Higher evidence rank: take new edge but keep max weight, merge metadata
                    let max_weight = existing.weight.max(edge.weight);
                    let merged_meta = merge_metadata(&existing.metadata, &edge.metadata);
                    *existing = edge;
                    existing.weight = max_weight;
                    existing.metadata = merged_meta;
                } else if edge.weight > existing.weight {
                    // Same/lower evidence rank but higher weight: update weight and merge metadata
                    existing.weight = edge.weight;
                    existing.metadata = merge_metadata(&existing.metadata, &edge.metadata);
                }
            }
            None => {
                map.insert(key, edge);
            }
        }
    }
    map.into_values().collect()
}

fn evidence_rank(evidence: &str) -> u8 {
    match evidence {
        "extracted" => 3,
        "inferred" => 2,
        "ambiguous" => 1,
        _ => 0,
    }
}

fn merge_metadata(existing: &Value, incoming: &Value) -> Value {
    let mut result = match existing {
        Value::Object(map) => Value::Object(map.clone()),
        _ => json!({}),
    };
    if let (Value::Object(target), Value::Object(source)) = (&mut result, incoming) {
        for (key, value) in source {
            target.insert(key.clone(), value.clone());
        }
    }
    result
}

fn compute_statistics(
    nodes: &[KnowledgeNode],
    edges: &[KnowledgeEdge],
    communities: &[CommunityInfo],
) -> GraphStatistics {
    let mut degree = HashMap::<&str, usize>::new();
    for node in nodes {
        degree.insert(&node.id, 0);
    }
    let mut evidence_distribution = EvidenceDistribution::default();
    for edge in edges {
        *degree.entry(&edge.source).or_default() += 1;
        *degree.entry(&edge.target).or_default() += 1;
        match edge.evidence.as_str() {
            "extracted" => evidence_distribution.extracted += 1,
            "inferred" => evidence_distribution.inferred += 1,
            _ => evidence_distribution.ambiguous += 1,
        }
    }
    let node_count = nodes.len();
    let edge_count = edges.len();
    let total_degree = degree.values().sum::<usize>() as f64;
    let avg_degree = if node_count > 0 {
        (total_degree / node_count as f64 * 100.0).round() / 100.0
    } else {
        0.0
    };
    let max_possible = node_count.saturating_mul(node_count.saturating_sub(1)) as f64 / 2.0;
    let density = if max_possible > 0.0 {
        ((edge_count as f64 / max_possible) * 10_000.0).round() / 10_000.0
    } else {
        0.0
    };
    let isolated_nodes = degree.values().filter(|value| **value == 0).count();
    GraphStatistics {
        node_count,
        edge_count,
        community_count: communities.len(),
        evidence_distribution,
        avg_degree,
        density,
        isolated_nodes,
    }
}

fn empty_stats() -> GraphStatistics {
    GraphStatistics {
        node_count: 0,
        edge_count: 0,
        community_count: 0,
        evidence_distribution: EvidenceDistribution::default(),
        avg_degree: 0.0,
        density: 0.0,
        isolated_nodes: 0,
    }
}

fn build_adjacency(edges: &[KnowledgeEdge]) -> HashMap<String, Vec<String>> {
    let mut adjacency = HashMap::<String, Vec<String>>::new();
    for edge in edges {
        adjacency
            .entry(edge.source.clone())
            .or_default()
            .push(edge.target.clone());
        adjacency
            .entry(edge.target.clone())
            .or_default()
            .push(edge.source.clone());
    }
    adjacency
}

fn build_community(
    id: usize,
    members: &[String],
    edges: &[KnowledgeEdge],
    node_map: &HashMap<&str, &KnowledgeNode>,
) -> CommunityInfo {
    let member_set = members.iter().cloned().collect::<HashSet<_>>();
    let mut intra = 0usize;
    let mut bridge = HashMap::<String, usize>::new();
    let mut degree = HashMap::<String, usize>::new();
    for edge in edges {
        let source_in = member_set.contains(&edge.source);
        let target_in = member_set.contains(&edge.target);
        if source_in && target_in {
            intra += 1;
            *degree.entry(edge.source.clone()).or_default() += 1;
            *degree.entry(edge.target.clone()).or_default() += 1;
        } else if source_in && !target_in {
            *bridge.entry(edge.source.clone()).or_default() += 1;
        } else if target_in && !source_in {
            *bridge.entry(edge.target.clone()).or_default() += 1;
        }
    }
    let max_possible = members
        .len()
        .saturating_mul(members.len().saturating_sub(1)) as f64
        / 2.0;
    let cohesion = if max_possible > 0.0 {
        (intra as f64 / max_possible).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let mut representative_nodes = degree.into_iter().collect::<Vec<_>>();
    representative_nodes.sort_by_key(|(_, score)| std::cmp::Reverse(*score));
    let mut bridge_nodes = bridge.into_iter().collect::<Vec<_>>();
    bridge_nodes.sort_by_key(|(_, score)| std::cmp::Reverse(*score));
    let rep_nodes: Vec<String> = representative_nodes
        .into_iter()
        .take(5)
        .map(|(id, _)| id)
        .collect();

    // Generate community label based on dominant node type (matching Node.js)
    let label = generate_community_label(members, edges, node_map);

    CommunityInfo {
        community_id: id,
        label: Some(label),
        node_count: members.len(),
        cohesion_score: (cohesion * 10_000.0).round() / 10_000.0,
        representative_nodes: rep_nodes,
        bridge_nodes: bridge_nodes.into_iter().take(3).map(|(id, _)| id).collect(),
        community_path: None,
        level: 0,
    }
}

fn collect_subgraph(graph: &KnowledgeGraph, seeds: &[String], depth: usize) -> GraphQueryResponse {
    let adjacency = build_adjacency(&graph.edges);
    let mut visited = seeds.iter().cloned().collect::<HashSet<_>>();
    let mut queue = seeds
        .iter()
        .cloned()
        .map(|id| (id, 0usize))
        .collect::<VecDeque<_>>();
    while let Some((current, current_depth)) = queue.pop_front() {
        if current_depth >= depth {
            continue;
        }
        if let Some(neighbors) = adjacency.get(&current) {
            for neighbor in neighbors {
                if visited.insert(neighbor.clone()) {
                    queue.push_back((neighbor.clone(), current_depth + 1));
                }
            }
        }
    }
    let nodes = graph
        .nodes
        .iter()
        .filter(|node| visited.contains(&node.id))
        .cloned()
        .collect::<Vec<_>>();
    let edges = graph
        .edges
        .iter()
        .filter(|edge| visited.contains(&edge.source) && visited.contains(&edge.target))
        .cloned()
        .collect::<Vec<_>>();
    GraphQueryResponse {
        nodes,
        edges,
        metadata: json!({ "seedIds": seeds, "depth": depth }),
    }
}

fn shortest_path(graph: &KnowledgeGraph, source: &str, target: &str) -> GraphQueryResponse {
    if source == target {
        return collect_subgraph(graph, &[source.to_string()], 0);
    }
    let adjacency = build_adjacency(&graph.edges);
    let mut queue = VecDeque::from([source.to_string()]);
    let mut previous = HashMap::<String, Option<String>>::from([(source.to_string(), None)]);
    while let Some(current) = queue.pop_front() {
        if current == target {
            break;
        }
        if let Some(neighbors) = adjacency.get(&current) {
            for neighbor in neighbors {
                if previous.contains_key(neighbor) {
                    continue;
                }
                previous.insert(neighbor.clone(), Some(current.clone()));
                queue.push_back(neighbor.clone());
            }
        }
    }
    if !previous.contains_key(target) {
        return GraphQueryResponse {
            nodes: Vec::new(),
            edges: Vec::new(),
            metadata: json!({ "sourceId": source, "targetId": target, "found": false }),
        };
    }
    let mut ids = Vec::new();
    let mut cursor = Some(target.to_string());
    while let Some(current) = cursor {
        ids.push(current.clone());
        cursor = previous.get(&current).cloned().flatten();
    }
    ids.reverse();

    // Collect only edges that are specifically on the path (matching Node.js)
    let mut path_edges = Vec::new();
    for window in ids.windows(2) {
        let (hop_source, hop_target) = (&window[0], &window[1]);
        // Find the edge for this hop (check both directions since graph is undirected for BFS)
        if let Some(edge) = graph.edges.iter().find(|e| {
            (e.source == *hop_source && e.target == *hop_target)
                || (e.source == *hop_target && e.target == *hop_source)
        }) {
            path_edges.push(edge.clone());
        }
    }

    let id_set = ids.iter().cloned().collect::<HashSet<_>>();
    GraphQueryResponse {
        nodes: graph
            .nodes
            .iter()
            .filter(|node| id_set.contains(&node.id))
            .cloned()
            .collect(),
        edges: path_edges,
        metadata: json!({ "sourceId": source, "targetId": target, "found": true, "hopCount": ids.len().saturating_sub(1) }),
    }
}

// ---------------------------------------------------------------------------
// v2.2.2: NeuroPath-inspired goal-directed path pruning
// ---------------------------------------------------------------------------

/// Tokenize a query into lowercase sub-goal tokens (≥3 chars, non-stopword).
fn parse_sub_goals(text: &str) -> Vec<String> {
    use regex::Regex;
    use std::sync::LazyLock;
    static TOKEN_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[a-z0-9_./:+-]+").unwrap());
    static STOPWORDS: &[&str] = &[
        "the", "and", "for", "with", "from", "that", "this", "what", "which", "who", "how", "why",
        "was", "were", "has", "have", "had", "are", "can", "all", "any", "but", "not", "our",
        "your", "his", "her", "its", "ours",
    ];
    let lower = text.to_lowercase();
    let mut seen = HashSet::new();
    let mut goals = Vec::new();
    for m in TOKEN_RE.find_iter(&lower) {
        let tok = m.as_str();
        if tok.len() < 3 || STOPWORDS.contains(&tok) {
            continue;
        }
        if seen.insert(tok.to_string()) {
            goals.push(tok.to_string());
        }
    }
    goals
}

/// Return a bitmap indicating which sub-goals are "covered" by this node
/// (i.e., match its id, label, or metadata).
fn compute_coverage(node: &KnowledgeNode, sub_goals: &[String]) -> Vec<bool> {
    let haystack = format!(
        "{} {} {}",
        node.id.to_lowercase(),
        node.label.to_lowercase(),
        node.metadata.to_string().to_lowercase()
    );
    sub_goals.iter().map(|g| haystack.contains(g)).collect()
}

/// Merge two coverage bitmaps (OR).
fn merge_coverage(a: &[bool], b: &[bool]) -> Vec<bool> {
    a.iter().zip(b.iter()).map(|(x, y)| *x || *y).collect()
}

/// Score how much an edge advances toward uncovered sub-goals. Returns
/// fraction of uncovered goals that appear in the edge text.
fn score_advancement(
    from: &KnowledgeNode,
    to: &KnowledgeNode,
    edge: &KnowledgeEdge,
    sub_goals: &[String],
    covered: &[bool],
) -> f64 {
    let uncovered: Vec<&String> = sub_goals
        .iter()
        .zip(covered.iter())
        .filter_map(|(g, c)| if *c { None } else { Some(g) })
        .collect();
    if uncovered.is_empty() {
        return 1.0;
    }
    let edge_text = format!(
        "{} {} {} {} {}",
        from.label.to_lowercase(),
        from.id.to_lowercase(),
        edge.relation.to_lowercase(),
        to.label.to_lowercase(),
        to.id.to_lowercase()
    );
    let hits = uncovered
        .iter()
        .filter(|g| edge_text.contains(g.as_str()))
        .count();
    hits as f64 / uncovered.len() as f64
}

/// NeuroPath-inspired goal-directed BFS with semantic pruning.
///
/// Parses `text` into sub-goals, starts from `start` (or best-matching node
/// if absent), and explores the graph via BFS while pruning edges whose
/// advancement score falls below `PRUNE_THRESHOLD`.
fn goal_directed_search(
    graph: &KnowledgeGraph,
    start: Option<&str>,
    text: &str,
    max_hops: usize,
) -> GraphQueryResponse {
    const PRUNE_THRESHOLD: f64 = 0.15;
    const MAX_PATHS: usize = 5;

    let sub_goals = parse_sub_goals(text);
    if sub_goals.is_empty() {
        return GraphQueryResponse {
            nodes: Vec::new(),
            edges: Vec::new(),
            metadata: json!({ "error": "no sub-goals parsed from text", "text": text }),
        };
    }

    // Determine start node: use provided, else best-coverage match.
    let start_id: String = match start {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => graph
            .nodes
            .iter()
            .max_by_key(|n| {
                compute_coverage(n, &sub_goals)
                    .iter()
                    .filter(|c| **c)
                    .count()
            })
            .map(|n| n.id.clone())
            .unwrap_or_default(),
    };

    if start_id.is_empty() || !graph.nodes.iter().any(|n| n.id == start_id) {
        return GraphQueryResponse {
            nodes: Vec::new(),
            edges: Vec::new(),
            metadata: json!({
                "error": "start node not found",
                "startNodeId": start_id,
            }),
        };
    }

    let node_map: HashMap<&str, &KnowledgeNode> =
        graph.nodes.iter().map(|n| (n.id.as_str(), n)).collect();

    // Adjacency with edge indices (undirected traversal, directed edge data).
    let mut adjacency: HashMap<&str, Vec<(&str, usize)>> = HashMap::new();
    for (i, edge) in graph.edges.iter().enumerate() {
        adjacency
            .entry(edge.source.as_str())
            .or_default()
            .push((edge.target.as_str(), i));
        adjacency
            .entry(edge.target.as_str())
            .or_default()
            .push((edge.source.as_str(), i));
    }

    // BFS frontier: (current_node_id, path_edge_indices, covered_bitmap)
    let init_covered = node_map
        .get(start_id.as_str())
        .map(|n| compute_coverage(n, &sub_goals))
        .unwrap_or_else(|| vec![false; sub_goals.len()]);
    let mut frontier: Vec<(String, Vec<usize>, Vec<bool>)> =
        vec![(start_id.clone(), Vec::new(), init_covered.clone())];

    // (node_id, coverage_bitmap) dedup — different coverage states are distinct.
    let mut visited: HashSet<(String, Vec<bool>)> = HashSet::new();
    visited.insert((start_id.clone(), init_covered.clone()));

    let mut complete: Vec<(Vec<usize>, Vec<bool>)> = Vec::new();
    let mut partials: Vec<(Vec<usize>, Vec<bool>)> = Vec::new();
    let mut edges_explored: usize = 0;
    let mut edges_pruned: usize = 0;

    // Short-circuit: start node already covers all goals.
    if init_covered.iter().all(|c| *c) {
        complete.push((Vec::new(), init_covered.clone()));
    }

    for _hop in 0..max_hops {
        if frontier.is_empty() {
            break;
        }
        let mut next_frontier: Vec<(String, Vec<usize>, Vec<bool>)> = Vec::new();
        for (node_id, path, covered) in frontier.drain(..) {
            if covered.iter().all(|c| *c) {
                continue; // already complete — don't expand further
            }
            let Some(neighbors) = adjacency.get(node_id.as_str()) else {
                continue;
            };
            let Some(from_node) = node_map.get(node_id.as_str()) else {
                continue;
            };
            for &(neighbor_id, edge_idx) in neighbors {
                edges_explored += 1;
                if path.contains(&edge_idx) {
                    continue; // don't re-traverse same edge
                }
                let Some(to_node) = node_map.get(neighbor_id) else {
                    continue;
                };
                let edge = &graph.edges[edge_idx];
                let advancement = score_advancement(from_node, to_node, edge, &sub_goals, &covered);
                if advancement < PRUNE_THRESHOLD {
                    edges_pruned += 1;
                    continue;
                }
                let neighbor_cov = compute_coverage(to_node, &sub_goals);
                let new_covered = merge_coverage(&covered, &neighbor_cov);
                let key = (neighbor_id.to_string(), new_covered.clone());
                if !visited.insert(key) {
                    continue;
                }
                let mut new_path = path.clone();
                new_path.push(edge_idx);
                if new_covered.iter().all(|c| *c) {
                    complete.push((new_path.clone(), new_covered.clone()));
                    if complete.len() >= MAX_PATHS * 2 {
                        // Enough complete paths — stop expanding.
                        break;
                    }
                } else {
                    partials.push((new_path.clone(), new_covered.clone()));
                }
                next_frontier.push((neighbor_id.to_string(), new_path, new_covered));
            }
        }
        frontier = next_frontier;
    }

    // Select paths: prefer complete, fall back to best partial.
    let mut selected: Vec<(Vec<usize>, Vec<bool>)> = if !complete.is_empty() {
        complete.sort_by_key(|(p, _)| p.len());
        complete.into_iter().take(MAX_PATHS).collect()
    } else {
        partials.sort_by(|(pa, ca), (pb, cb)| {
            let ua = ca.iter().filter(|c| !**c).count();
            let ub = cb.iter().filter(|c| !**c).count();
            ua.cmp(&ub).then(pa.len().cmp(&pb.len()))
        });
        partials.into_iter().take(MAX_PATHS).collect()
    };
    // Keep the raw flag before consuming `selected`.
    let any_complete = selected.iter().any(|(_, c)| c.iter().all(|b| *b));
    if selected.is_empty() {
        // Surface the start node + its coverage as a degenerate result.
        selected.push((Vec::new(), init_covered.clone()));
    }

    // Collect nodes + edges on selected paths.
    let mut node_ids: HashSet<String> = HashSet::from([start_id.clone()]);
    let mut path_edges: Vec<KnowledgeEdge> = Vec::new();
    let mut seen_edge_idx: HashSet<usize> = HashSet::new();
    for (path, _) in &selected {
        for &ei in path {
            if seen_edge_idx.insert(ei) {
                let e = &graph.edges[ei];
                node_ids.insert(e.source.clone());
                node_ids.insert(e.target.clone());
                path_edges.push(e.clone());
            }
        }
    }
    let nodes: Vec<KnowledgeNode> = graph
        .nodes
        .iter()
        .filter(|n| node_ids.contains(&n.id))
        .cloned()
        .collect();

    let paths_meta: Vec<serde_json::Value> = selected
        .iter()
        .map(|(path, covered)| {
            let covered_goals: Vec<&String> = sub_goals
                .iter()
                .zip(covered.iter())
                .filter_map(|(g, c)| if *c { Some(g) } else { None })
                .collect();
            json!({
                "hops": path.len(),
                "coveredGoals": covered_goals,
                "complete": covered.iter().all(|c| *c),
            })
        })
        .collect();

    GraphQueryResponse {
        nodes,
        edges: path_edges,
        metadata: json!({
            "query": "goal_directed",
            "startNodeId": start_id,
            "subGoals": sub_goals,
            "maxHops": max_hops,
            "pruneThreshold": PRUNE_THRESHOLD,
            "edgesExplored": edges_explored,
            "edgesPruned": edges_pruned,
            "pruneRatio": if edges_explored > 0 {
                edges_pruned as f64 / edges_explored as f64
            } else {
                0.0
            },
            "anyComplete": any_complete,
            "paths": paths_meta,
        }),
    }
}

fn node_bucket(node_type: &str) -> &str {
    match node_type {
        "file" | "module" => "files",
        "document" | "section" | "rationale" => "docs",
        _ => "symbols",
    }
}

fn memory_node_type(memory_type: &str) -> &str {
    match memory_type {
        "decision" => "decision",
        "task" => "task",
        "constraint" => "constraint",
        "bug" => "bug",
        "fix" => "fix",
        "open_question" => "open_question",
        "fact" => "fact",
        "implementation_detail" => "implementation_detail",
        "summary" => "summary",
        "risk" => "error",
        "change_log" => "change_log",
        _ => "memory",
    }
}

fn memory_episode_metadata(metadata: &Value) -> (Option<String>, Option<i64>) {
    (
        metadata
            .get("sessionId")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        metadata.get("episodeOrdinal").and_then(Value::as_i64),
    )
}

fn is_companion_memory_pair(left: &str, right: &str) -> bool {
    // Match Node.js: pair.has('summary') && (pair.has('bug') || pair.has('implementation_detail'))
    let pair = [left, right];
    pair.contains(&"summary") && (pair.contains(&"bug") || pair.contains(&"implementation_detail"))
}

fn normalize_error_signature(input: &str) -> String {
    use regex::Regex;
    use std::sync::LazyLock;

    // Match Node.js: .toLowerCase().replace(/[0-9a-f]{7,}/g, '').replace(/\s+/g, '_').trim().slice(0, 80)
    static HEX_RUN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[0-9a-f]{7,}").unwrap());
    static WHITESPACE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+").unwrap());

    let lowered = input.to_lowercase();
    let without_hex = HEX_RUN.replace_all(&lowered, "");
    let normalized = WHITESPACE.replace_all(&without_hex, "_");
    let trimmed = normalized.trim();
    trimmed.chars().take(80).collect()
}

fn tokenize(input: &str) -> Vec<String> {
    use regex::Regex;
    use std::sync::LazyLock;

    static TOKEN_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[a-z0-9_]{3,}").unwrap());
    static STOPWORDS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
        [
            "also",
            "and",
            "about",
            "after",
            "agent",
            "agents",
            "around",
            "before",
            "build",
            "change",
            "changes",
            "code",
            "details",
            "debug",
            "debugging",
            "during",
            "error",
            "errors",
            "file",
            "files",
            "fix",
            "fixed",
            "from",
            "into",
            "issue",
            "issues",
            "memory",
            "need",
            "needs",
            "now",
            "output",
            "project",
            "request",
            "review",
            "reviewed",
            "reviewing",
            "session",
            "summary",
            "that",
            "the",
            "then",
            "this",
            "test",
            "tests",
            "update",
            "updated",
            "using",
            "with",
            "work",
        ]
        .into_iter()
        .collect()
    });

    let lowered = input.to_lowercase();
    TOKEN_RE
        .find_iter(&lowered)
        .map(|m| m.as_str())
        .filter(|token| !STOPWORDS.contains(token))
        .map(ToOwned::to_owned)
        .collect()
}

fn generate_community_label(
    members: &[String],
    _edges: &[KnowledgeEdge],
    node_map: &HashMap<&str, &KnowledgeNode>,
) -> String {
    // Count node types using actual node data (matching Node.js which reads node.type)
    let mut type_counts = HashMap::<String, usize>::new();
    for member in members {
        let node_type = node_map
            .get(member.as_str())
            .map(|n| {
                let t = &n.node_type;
                let mut chars = t.chars();
                match chars.next() {
                    None => String::new(),
                    Some(c) => c.to_uppercase().to_string() + chars.as_str(),
                }
            })
            .unwrap_or_else(|| "Mixed".to_string());
        *type_counts.entry(node_type).or_default() += 1;
    }
    let dominant = type_counts
        .iter()
        .max_by_key(|(_, count)| **count)
        .map(|(t, _)| t.clone())
        .unwrap_or_else(|| "Mixed".to_string());
    format!("{dominant} cluster ({} nodes)", members.len())
}

/// Find connected components using BFS (matching Node.js findConnectedComponents)
fn find_connected_components(
    nodes: &[KnowledgeNode],
    adjacency: &HashMap<String, Vec<(String, f64)>>,
) -> Vec<Vec<String>> {
    let mut visited = HashSet::<String>::new();
    let mut components = Vec::new();
    for node in nodes {
        if !visited.insert(node.id.clone()) {
            continue;
        }
        let mut queue = VecDeque::from([node.id.clone()]);
        let mut component = Vec::new();
        while let Some(current) = queue.pop_front() {
            component.push(current.clone());
            if let Some(neighbors) = adjacency.get(&current) {
                for (neighbor, _) in neighbors {
                    if visited.insert(neighbor.clone()) {
                        queue.push_back(neighbor.clone());
                    }
                }
            }
        }
        components.push(component);
    }
    components.sort_by_key(|component| std::cmp::Reverse(component.len()));
    components
}

fn build_adjacency_weighted(edges: &[KnowledgeEdge]) -> HashMap<String, Vec<(String, f64)>> {
    let mut adjacency = HashMap::<String, Vec<(String, f64)>>::new();
    for edge in edges {
        adjacency
            .entry(edge.source.clone())
            .or_default()
            .push((edge.target.clone(), edge.weight));
        adjacency
            .entry(edge.target.clone())
            .or_default()
            .push((edge.source.clone(), edge.weight));
    }
    adjacency
}

/// Greedy modularity clustering (Louvain-like), matching Node.js cluster.ts
/// v2.2.2: God Node damping — reduce edge weights for nodes above the 95th
/// percentile degree, using `1 / ln(degree)` so they don't force unrelated
/// nodes into the same community.
fn damp_hub_edges(nodes: &[KnowledgeNode], adjacency: &mut HashMap<String, Vec<(String, f64)>>) {
    // Compute degree for each node
    let mut degrees: Vec<usize> = nodes
        .iter()
        .map(|n| adjacency.get(&n.id).map_or(0, |v| v.len()))
        .collect();
    degrees.sort_unstable();

    if degrees.is_empty() {
        return;
    }

    // 95th percentile threshold
    let p95_idx = (degrees.len() as f64 * 0.95).floor() as usize;
    let threshold = degrees.get(p95_idx).copied().unwrap_or(usize::MAX);
    if threshold < 5 {
        return; // Don't damp in very small graphs
    }

    // Build set of hub node IDs and their node_type for classification
    let node_type_map: HashMap<&str, &str> = nodes
        .iter()
        .map(|n| (n.id.as_str(), n.node_type.as_str()))
        .collect();

    let hub_ids: HashSet<&str> = nodes
        .iter()
        .filter(|n| adjacency.get(&n.id).map_or(0, |v| v.len()) > threshold)
        .map(|n| n.id.as_str())
        .collect();

    // Pre-compute damping factors for hubs
    let hub_damping: HashMap<&str, f64> = hub_ids
        .iter()
        .map(|&id| {
            let degree = adjacency.get(id).map_or(1, |v| v.len()).max(2);
            (id, 1.0 / (degree as f64).ln().max(1.0))
        })
        .collect();

    // Apply damping: for each edge involving a hub, reduce its weight
    for (node_id, neighbors) in adjacency.iter_mut() {
        let is_hub = hub_ids.contains(node_id.as_str());
        let out_damping = if is_hub {
            hub_damping.get(node_id.as_str()).copied().unwrap_or(1.0)
        } else {
            1.0
        };
        for (target, weight) in neighbors.iter_mut() {
            let in_damping = hub_damping.get(target.as_str()).copied().unwrap_or(1.0);
            // Apply the stronger (smaller) damping factor
            *weight *= out_damping.min(in_damping);
        }
    }

    let _ = node_type_map; // used for classification in hub_nodes query
}

/// v2.2.2: Classify hub type for filtering in hub_nodes query.
fn classify_hub_type(node: &KnowledgeNode, degree: usize) -> &'static str {
    match node.node_type.as_str() {
        "session" => "session_hub",
        "module" => "import_hub",
        "file" | "document" if degree > 50 => "central_file",
        _ => "domain_hub",
    }
}

fn greedy_modularity_clustering(
    nodes: &[KnowledgeNode],
    adjacency: &HashMap<String, Vec<(String, f64)>>,
) -> HashMap<String, usize> {
    let mut community: HashMap<String, usize> = HashMap::new();
    for (i, node) in nodes.iter().enumerate() {
        community.insert(node.id.clone(), i);
    }

    // Total edge weight
    let total_weight: f64 = adjacency
        .values()
        .flat_map(|neighbors| neighbors.iter().map(|(_, w)| w))
        .sum::<f64>()
        / 2.0;

    if total_weight == 0.0 {
        return community;
    }

    let max_iterations = 20;
    for _ in 0..max_iterations {
        let mut changed = false;
        for node in nodes {
            let node_id = &node.id;
            let current_community = community[node_id];
            let neighbors = match adjacency.get(node_id) {
                Some(n) => n,
                None => continue,
            };

            // Calculate weight to each neighboring community
            let mut community_weights = HashMap::<usize, f64>::new();
            for (neighbor, weight) in neighbors {
                if let Some(&neighbor_community) = community.get(neighbor) {
                    *community_weights.entry(neighbor_community).or_default() += weight;
                }
            }

            // Find the community with the highest edge weight gain (matching Node.js: compare vs 0, skip current)
            let mut best_community = current_community;
            let mut best_gain: f64 = 0.0;
            for (&candidate_community, &weight) in &community_weights {
                if candidate_community == current_community {
                    continue;
                }
                if weight > best_gain {
                    best_gain = weight;
                    best_community = candidate_community;
                }
            }

            if best_community != current_community && best_gain > 0.0 {
                community.insert(node_id.clone(), best_community);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    community
}

/// Split oversized communities by re-running modularity clustering (matching Node.js splitOversizedCommunities)
fn split_oversized_communities(
    community: &mut HashMap<String, usize>,
    adjacency: &HashMap<String, Vec<(String, f64)>>,
    max_size: usize,
) {
    let mut next_id = community.values().copied().max().unwrap_or(0) + 1;

    // Collect communities that exceed max_size
    let mut community_members: HashMap<usize, Vec<String>> = HashMap::new();
    for (node_id, &comm_id) in community.iter() {
        community_members
            .entry(comm_id)
            .or_default()
            .push(node_id.clone());
    }

    for (_comm_id, members) in community_members {
        if members.len() <= max_size {
            continue;
        }

        // Build sub-nodes for the oversized community
        let sub_nodes: Vec<KnowledgeNode> = members
            .iter()
            .map(|id| KnowledgeNode {
                id: id.clone(),
                label: String::new(),
                node_type: String::new(),
                source_type: String::new(),
                source_id: String::new(),
                metadata: json!({}),
                community_id: None,
            })
            .collect();

        // Re-run greedy modularity on just these members
        let sub_assignments = greedy_modularity_clustering(&sub_nodes, adjacency);

        // Check if the sub-clustering actually produced more than 1 community
        let local_ids: HashSet<usize> = sub_assignments.values().copied().collect();
        if local_ids.len() <= 1 {
            continue;
        }

        // Remap local community IDs to new global IDs
        let mut remap = HashMap::new();
        for &local_id in &local_ids {
            remap.insert(local_id, next_id);
            next_id += 1;
        }

        for (node_id, local_id) in sub_assignments {
            if let Some(&new_id) = remap.get(&local_id) {
                community.insert(node_id, new_id);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// v2.2.2: Community-aware retrieval (DESIGN.md §3.4)
// ---------------------------------------------------------------------------

/// Score each community in `graph` by lexical overlap between `query` and the
/// community's label + representative node labels. Returns `community_id → score`
/// in `[0.0, 1.0]`. An empty query yields an empty map.
///
/// This is a lexical approximation of the "query × community summary embedding"
/// step from DESIGN.md §3.4. Using lexical overlap avoids a separate embedding
/// index; if/when community summary embeddings land, swap this implementation.
pub fn community_relevance_from_query(query: &str, graph: &KnowledgeGraph) -> HashMap<usize, f64> {
    let goals = parse_sub_goals(query);
    if goals.is_empty() {
        return HashMap::new();
    }
    // Pre-index nodes by id so we can resolve representative_nodes labels.
    let node_label: HashMap<&str, &str> = graph
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.label.as_str()))
        .collect();

    let mut out = HashMap::with_capacity(graph.communities.len());
    for c in &graph.communities {
        let label = c.label.clone().unwrap_or_default();
        let mut haystack = label.to_lowercase();
        for rep_id in &c.representative_nodes {
            if let Some(lbl) = node_label.get(rep_id.as_str()) {
                haystack.push(' ');
                haystack.push_str(&lbl.to_lowercase());
            }
            haystack.push(' ');
            haystack.push_str(&rep_id.to_lowercase());
        }
        let hits = goals
            .iter()
            .filter(|g| haystack.contains(g.as_str()))
            .count();
        let score = hits as f64 / goals.len() as f64;
        out.insert(c.community_id, score);
    }
    out
}

/// Build a `memory_uuid → community_id` map from graph nodes whose id has the
/// form `memory:<uuid>` and whose `community_id` is set. Nodes without a parsed
/// UUID or community assignment are skipped.
pub fn memory_community_map(graph: &KnowledgeGraph) -> HashMap<Uuid, usize> {
    let mut out = HashMap::new();
    for node in &graph.nodes {
        let Some(rest) = node.id.strip_prefix("memory:") else {
            continue;
        };
        let Ok(uuid) = Uuid::parse_str(rest) else {
            continue;
        };
        if let Some(cid) = node.community_id {
            out.insert(uuid, cid);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use chum_mem_contracts::{CanonicalEventType, SessionEventPayload};

    #[test]
    fn builds_memory_edges_from_same_episode_and_similarity() {
        let session_id = Uuid::nil();
        let project_id = Uuid::nil();
        let graph = build_knowledge_graph(
            project_id,
            session_id,
            &[SessionEventRecord {
                id: Uuid::new_v4(),
                event_type: CanonicalEventType::Command,
                payload: SessionEventPayload {
                    command: Some("cargo test".to_string()),
                    ..Default::default()
                },
                created_at: "2026-01-01T00:00:00Z".to_string(),
            }],
            &[SessionEpisodeDraft {
                episode_ordinal: 1,
                episode_type: "implementation".to_string(),
                title: "Implement tests".to_string(),
                summary: "added tests and fixed failures".to_string(),
                started_at: "2026-01-01T00:00:00Z".to_string(),
                ended_at: "2026-01-01T00:10:00Z".to_string(),
                provenance_event_ids: Vec::new(),
                metadata: json!({}),
            }],
            &[
                MemoryNodeInput {
                    id: Uuid::new_v4(),
                    memory_type: "summary".to_string(),
                    title: "Testing summary".to_string(),
                    content: "added integration tests for cargo test failures".to_string(),
                    summary: "added integration tests".to_string(),
                    importance_score: 0.8,
                    metadata: json!({ "sessionId": session_id, "episodeOrdinal": 1 }),
                },
                MemoryNodeInput {
                    id: Uuid::new_v4(),
                    memory_type: "implementation_detail".to_string(),
                    title: "Implementation detail".to_string(),
                    content: "cargo test failure fixed with integration tests and command updates"
                        .to_string(),
                    summary: "fixed cargo test failure".to_string(),
                    importance_score: 0.7,
                    metadata: json!({ "sessionId": session_id, "episodeOrdinal": 1 }),
                },
            ],
            &[],
        );
        assert!(
            graph.edges.iter().any(
                |edge| edge.source.starts_with("memory:") && edge.target.starts_with("memory:")
            )
        );
    }

    // ── v2.2.2: Goal-directed path pruning tests ──

    fn mk_node(id: &str, label: &str) -> KnowledgeNode {
        KnowledgeNode {
            id: id.to_string(),
            label: label.to_string(),
            node_type: "symbol".to_string(),
            source_type: "derived".to_string(),
            source_id: id.to_string(),
            metadata: json!({}),
            community_id: None,
        }
    }

    fn mk_edge(source: &str, target: &str, relation: &str) -> KnowledgeEdge {
        KnowledgeEdge {
            source: source.to_string(),
            target: target.to_string(),
            relation: relation.to_string(),
            evidence: "extracted".to_string(),
            weight: 1.0,
            source_file: None,
            metadata: json!({}),
        }
    }

    fn mk_test_graph() -> KnowledgeGraph {
        // Build a small graph:
        //   fileA (has "retrieval") → funcX (has "worker") → bugY (has "oom")
        //   fileA → noise1 → noise2 (unrelated)
        let nodes = vec![
            mk_node("file:fileA.rs", "retrieval module"),
            mk_node("symbol:funcX", "worker_process"),
            mk_node("bug:bugY", "oom crash in worker"),
            mk_node("file:noise1.rs", "unrelated docs"),
            mk_node("symbol:noise2", "random_helper"),
        ];
        let edges = vec![
            mk_edge("file:fileA.rs", "symbol:funcX", "defines"),
            mk_edge("symbol:funcX", "bug:bugY", "affected_by"),
            mk_edge("file:fileA.rs", "file:noise1.rs", "imports"),
            mk_edge("file:noise1.rs", "symbol:noise2", "defines"),
        ];
        let statistics = GraphStatistics {
            node_count: nodes.len(),
            edge_count: edges.len(),
            community_count: 0,
            evidence_distribution: EvidenceDistribution::default(),
            avg_degree: 0.0,
            density: 0.0,
            isolated_nodes: 0,
        };
        KnowledgeGraph {
            version: "test".to_string(),
            generated_at: "2026-04-16T00:00:00Z".to_string(),
            project_id: Uuid::nil(),
            nodes,
            edges,
            communities: Vec::new(),
            statistics,
        }
    }

    #[test]
    fn goal_directed_finds_complete_path() {
        let graph = mk_test_graph();
        let response = goal_directed_search(&graph, Some("file:fileA.rs"), "worker oom bug", 3);
        // Should discover path fileA → funcX → bugY covering both "worker" and "oom"
        assert!(
            response.edges.len() >= 2,
            "expected at least 2 edges on path: {:?}",
            response.edges
        );
        assert!(
            response.nodes.iter().any(|n| n.id == "bug:bugY"),
            "expected bugY in path nodes"
        );
        let any_complete = response
            .metadata
            .get("anyComplete")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        assert!(any_complete, "expected at least one complete path");
    }

    #[test]
    fn goal_directed_prunes_unrelated_edges() {
        let graph = mk_test_graph();
        let response = goal_directed_search(&graph, Some("file:fileA.rs"), "worker oom bug", 3);
        // noise2 should not appear — it doesn't advance toward worker/oom
        assert!(
            !response.nodes.iter().any(|n| n.id == "symbol:noise2"),
            "pruning should exclude unrelated noise2"
        );
        let pruned = response
            .metadata
            .get("edgesPruned")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        assert!(pruned > 0, "expected some edges to be pruned");
    }

    #[test]
    fn goal_directed_empty_text_returns_error() {
        let graph = mk_test_graph();
        let response = goal_directed_search(&graph, Some("file:fileA.rs"), "   ", 3);
        assert!(response.metadata.get("error").is_some());
    }

    #[test]
    fn goal_directed_picks_start_when_absent() {
        let graph = mk_test_graph();
        // No start node given — it should derive start from sub-goal coverage.
        let response = goal_directed_search(&graph, None, "worker oom bug", 3);
        let start = response
            .metadata
            .get("startNodeId")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        assert!(
            !start.is_empty(),
            "should derive a start node: {:?}",
            response.metadata
        );
    }

    #[test]
    fn goal_directed_subgoal_parsing() {
        let goals = parse_sub_goals("What functions return Result and was affected by bug X?");
        assert!(goals.contains(&"functions".to_string()));
        assert!(goals.contains(&"return".to_string()));
        assert!(goals.contains(&"result".to_string()));
        assert!(goals.contains(&"affected".to_string()));
        assert!(goals.contains(&"bug".to_string()));
        // Stopwords filtered
        assert!(!goals.contains(&"and".to_string()));
        assert!(!goals.contains(&"was".to_string()));
    }

    // ── v2.2.2: Community-aware retrieval tests ──

    fn mk_community(id: usize, label: &str, reps: &[&str]) -> CommunityInfo {
        CommunityInfo {
            community_id: id,
            label: Some(label.to_string()),
            node_count: reps.len(),
            cohesion_score: 1.0,
            representative_nodes: reps.iter().map(|s| s.to_string()).collect(),
            bridge_nodes: Vec::new(),
            community_path: Some(id.to_string()),
            level: 0,
        }
    }

    #[test]
    fn community_relevance_scores_matching_communities() {
        let mut graph = mk_test_graph();
        graph.communities = vec![
            mk_community(0, "worker oom module", &["symbol:funcX", "bug:bugY"]),
            mk_community(1, "docs", &["file:noise1.rs", "symbol:noise2"]),
        ];
        let scores = community_relevance_from_query("worker oom", &graph);
        let c0 = scores.get(&0).copied().unwrap_or(0.0);
        let c1 = scores.get(&1).copied().unwrap_or(0.0);
        assert!(
            c0 > c1,
            "worker/oom community should outrank docs: {c0} vs {c1}"
        );
        assert!((c0 - 1.0).abs() < 1e-9, "both goals present → score 1.0");
    }

    #[test]
    fn community_relevance_empty_query_returns_empty() {
        let graph = mk_test_graph();
        assert!(community_relevance_from_query("   ", &graph).is_empty());
    }

    #[test]
    fn memory_community_map_extracts_uuids() {
        let memory_a = Uuid::new_v4();
        let memory_b = Uuid::new_v4();
        let mut node_not_memory = mk_node("file:x.rs", "not a memory");
        node_not_memory.community_id = Some(99);
        let nodes = vec![
            KnowledgeNode {
                id: format!("memory:{memory_a}"),
                label: "mem a".to_string(),
                node_type: "memory".to_string(),
                source_type: "derived".to_string(),
                source_id: memory_a.to_string(),
                metadata: json!({}),
                community_id: Some(7),
            },
            KnowledgeNode {
                id: format!("memory:{memory_b}"),
                label: "mem b".to_string(),
                node_type: "memory".to_string(),
                source_type: "derived".to_string(),
                source_id: memory_b.to_string(),
                metadata: json!({}),
                community_id: None, // no assignment → skipped
            },
            node_not_memory, // non-memory node → skipped even with community_id
        ];
        let graph = KnowledgeGraph {
            version: "test".to_string(),
            generated_at: "2026-04-16T00:00:00Z".to_string(),
            project_id: Uuid::nil(),
            nodes,
            edges: Vec::new(),
            communities: Vec::new(),
            statistics: GraphStatistics {
                node_count: 3,
                edge_count: 0,
                community_count: 0,
                evidence_distribution: EvidenceDistribution::default(),
                avg_degree: 0.0,
                density: 0.0,
                isolated_nodes: 0,
            },
        };
        let map = memory_community_map(&graph);
        assert_eq!(map.get(&memory_a).copied(), Some(7));
        assert!(!map.contains_key(&memory_b), "unassigned memory filtered");
        assert_eq!(map.len(), 1, "non-memory nodes excluded");
    }
}
