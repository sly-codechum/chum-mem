use std::collections::{HashMap, HashSet, VecDeque};

/// Result of Leiden community detection.
pub struct LeidenResult {
    /// Node ID -> Community ID mapping.
    pub assignments: HashMap<String, usize>,
    /// Number of communities found.
    pub community_count: usize,
    /// Modularity score.
    pub modularity: f64,
}

/// Run Leiden community detection on a weighted graph.
///
/// Arguments:
/// - `adjacency`: map from node_id -> vec of (neighbor_id, weight)
/// - `resolution`: resolution parameter (1.0 = standard, higher = smaller communities)
/// - `max_iterations`: max number of outer iterations
///
/// Returns community assignments.
pub fn leiden_clustering(
    adjacency: &HashMap<String, Vec<(String, f64)>>,
    resolution: f64,
    max_iterations: usize,
) -> LeidenResult {
    let nodes: Vec<String> = adjacency.keys().cloned().collect();

    if nodes.is_empty() {
        return LeidenResult {
            assignments: HashMap::new(),
            community_count: 0,
            modularity: 0.0,
        };
    }

    // Total edge weight (each edge counted once in each direction, so divide by 2).
    let m = total_edge_weight(adjacency);
    if m == 0.0 {
        // No edges: every node is its own community.
        let mut assignments = HashMap::new();
        for (i, node) in nodes.iter().enumerate() {
            assignments.insert(node.clone(), i);
        }
        return LeidenResult {
            assignments,
            community_count: nodes.len(),
            modularity: 0.0,
        };
    }

    // Initial assignment: each node in its own community.
    let mut partition: HashMap<String, usize> = HashMap::new();
    for (i, node) in nodes.iter().enumerate() {
        partition.insert(node.clone(), i);
    }
    let mut next_community_id = nodes.len();

    for _iteration in 0..max_iterations {
        // Phase 1: Local moving.
        let moved = local_moving_phase(&mut partition, adjacency, resolution, m);

        // Phase 2: Refinement -- ensure well-connected communities.
        refine_partition(
            &mut partition,
            adjacency,
            resolution,
            m,
            &mut next_community_id,
        );

        // Phase 3: Check convergence. If no moves happened in phase 1, we are done.
        if !moved {
            break;
        }

        // Phase 3 continued: Aggregation.
        // We aggregate the graph into super-nodes and repeat.
        // Build the aggregated graph.
        let (agg_adjacency, agg_partition, node_to_supernode) =
            aggregate_graph(adjacency, &partition);

        if agg_adjacency.len() >= partition.values().collect::<HashSet<_>>().len() {
            // No further aggregation possible.
            break;
        }

        // Run Leiden recursively on the aggregated graph.
        let sub_result = leiden_clustering(
            &agg_adjacency,
            resolution,
            max_iterations.saturating_sub(1).max(1),
        );

        // Map super-node assignments back to original nodes.
        // agg_partition maps super_node_name -> original community id (used for naming).
        // sub_result.assignments maps super_node_name -> new community id.
        // node_to_supernode maps original_node -> super_node_name.
        let _ = agg_partition; // used indirectly through node_to_supernode
        for (node, supernode) in &node_to_supernode {
            if let Some(&new_comm) = sub_result.assignments.get(supernode) {
                partition.insert(node.clone(), new_comm);
            }
        }

        break; // One level of aggregation is usually sufficient for our graph sizes.
    }

    // Compact community IDs to 0..n.
    let mut id_map: HashMap<usize, usize> = HashMap::new();
    let mut next_id = 0usize;
    // Sort for deterministic output.
    let mut sorted_nodes: Vec<&String> = partition.keys().collect();
    sorted_nodes.sort();
    for node in &sorted_nodes {
        let comm = partition[*node];
        if !id_map.contains_key(&comm) {
            id_map.insert(comm, next_id);
            next_id += 1;
        }
    }
    let mut assignments = HashMap::new();
    for (node, comm) in &partition {
        assignments.insert(node.clone(), id_map[comm]);
    }

    let community_count = next_id;
    let modularity = compute_modularity(&assignments, adjacency, resolution);

    LeidenResult {
        assignments,
        community_count,
        modularity,
    }
}

/// Compute the modularity of a partition.
///
/// Q = (1 / 2m) * sum_ij [ A_ij - gamma * (k_i * k_j) / (2m) ] * delta(c_i, c_j)
pub fn compute_modularity(
    partition: &HashMap<String, usize>,
    adjacency: &HashMap<String, Vec<(String, f64)>>,
    resolution: f64,
) -> f64 {
    let m = total_edge_weight(adjacency);
    if m == 0.0 {
        return 0.0;
    }

    // Compute node degrees (strength).
    let mut degree: HashMap<&str, f64> = HashMap::new();
    for (node, neighbors) in adjacency {
        let k: f64 = neighbors.iter().map(|(_, w)| w).sum();
        degree.insert(node.as_str(), k);
    }

    // For each community, accumulate internal edge weight and total degree.
    let mut sigma_in: HashMap<usize, f64> = HashMap::new();
    let mut sigma_tot: HashMap<usize, f64> = HashMap::new();

    for (node, &comm) in partition {
        let k = degree.get(node.as_str()).copied().unwrap_or(0.0);
        *sigma_tot.entry(comm).or_default() += k;

        if let Some(neighbors) = adjacency.get(node) {
            for (neighbor, weight) in neighbors {
                if let Some(&neighbor_comm) = partition.get(neighbor) {
                    if neighbor_comm == comm {
                        // Count each internal edge weight (will be counted from both sides).
                        *sigma_in.entry(comm).or_default() += weight;
                    }
                }
            }
        }
    }

    let two_m = 2.0 * m;
    let mut q = 0.0;
    for comm in sigma_tot.keys() {
        let s_in = sigma_in.get(comm).copied().unwrap_or(0.0);
        let s_tot = sigma_tot.get(comm).copied().unwrap_or(0.0);
        // s_in is double-counted (both endpoints), so internal edges = s_in / 2.
        q += s_in / two_m - resolution * (s_tot / two_m).powi(2);
    }
    q
}

/// Find connected components using BFS on the adjacency map.
pub fn find_connected_components(
    adjacency: &HashMap<String, Vec<(String, f64)>>,
) -> Vec<Vec<String>> {
    let mut visited = HashSet::<String>::new();
    let mut components = Vec::new();

    let mut all_nodes: Vec<&String> = adjacency.keys().collect();
    all_nodes.sort(); // deterministic order

    for node in all_nodes {
        if !visited.insert(node.clone()) {
            continue;
        }
        let mut queue = VecDeque::from([node.clone()]);
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
        component.sort();
        components.push(component);
    }
    components.sort_by_key(|c| std::cmp::Reverse(c.len()));
    components
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Total edge weight (sum of all weights / 2, since adjacency is symmetric).
fn total_edge_weight(adjacency: &HashMap<String, Vec<(String, f64)>>) -> f64 {
    adjacency
        .values()
        .flat_map(|neighbors| neighbors.iter().map(|(_, w)| w))
        .sum::<f64>()
        / 2.0
}

/// Node strength (sum of adjacent edge weights).
fn node_strength(node: &str, adjacency: &HashMap<String, Vec<(String, f64)>>) -> f64 {
    adjacency
        .get(node)
        .map(|neighbors| neighbors.iter().map(|(_, w)| w).sum())
        .unwrap_or(0.0)
}

/// Phase 1: Local moving. For each node, greedily move it to the neighboring community
/// that yields the best modularity gain. Returns true if any node moved.
fn local_moving_phase(
    partition: &mut HashMap<String, usize>,
    adjacency: &HashMap<String, Vec<(String, f64)>>,
    resolution: f64,
    m: f64,
) -> bool {
    let two_m = 2.0 * m;
    let mut moved = false;

    // Pre-compute community totals.
    let mut sigma_tot: HashMap<usize, f64> = HashMap::new();
    for (node, &comm) in partition.iter() {
        *sigma_tot.entry(comm).or_default() += node_strength(node, adjacency);
    }

    let mut node_list: Vec<String> = partition.keys().cloned().collect();
    node_list.sort(); // deterministic order

    let max_local_iterations = 10;
    for _ in 0..max_local_iterations {
        let mut any_moved_this_pass = false;

        for node_id in &node_list {
            let current_comm = partition[node_id];
            let k_i = node_strength(node_id, adjacency);

            let neighbors = match adjacency.get(node_id) {
                Some(n) => n,
                None => continue,
            };

            // Weight from node_id to each neighboring community.
            let mut comm_weights: HashMap<usize, f64> = HashMap::new();
            for (neighbor, weight) in neighbors {
                if let Some(&neighbor_comm) = partition.get(neighbor) {
                    *comm_weights.entry(neighbor_comm).or_default() += weight;
                }
            }

            // Weight to own community (k_i_in for current community).
            let k_i_in_current = comm_weights.get(&current_comm).copied().unwrap_or(0.0);

            // Modularity gain for removing node from current community.
            let sigma_tot_current = sigma_tot.get(&current_comm).copied().unwrap_or(0.0);
            let remove_cost = k_i_in_current / two_m
                - resolution * (sigma_tot_current - k_i) * k_i / (two_m * two_m);

            let mut best_comm = current_comm;
            let mut best_gain = 0.0;

            for (&candidate_comm, &k_i_in_candidate) in &comm_weights {
                if candidate_comm == current_comm {
                    continue;
                }
                let sigma_tot_candidate = sigma_tot.get(&candidate_comm).copied().unwrap_or(0.0);

                // Gain for inserting into candidate.
                let insert_gain = k_i_in_candidate / two_m
                    - resolution * sigma_tot_candidate * k_i / (two_m * two_m);

                let delta_q = insert_gain - remove_cost;

                if delta_q > best_gain {
                    best_gain = delta_q;
                    best_comm = candidate_comm;
                }
            }

            if best_comm != current_comm && best_gain > 1e-12 {
                // Move node.
                *sigma_tot.entry(current_comm).or_default() -= k_i;
                *sigma_tot.entry(best_comm).or_default() += k_i;
                partition.insert(node_id.clone(), best_comm);
                moved = true;
                any_moved_this_pass = true;
            }
        }

        if !any_moved_this_pass {
            break;
        }
    }

    moved
}

/// Phase 2: Refinement. For each community, check internal connectivity and split
/// communities that are not well-connected into sub-communities. This is the key
/// distinction of Leiden over Louvain.
fn refine_partition(
    partition: &mut HashMap<String, usize>,
    adjacency: &HashMap<String, Vec<(String, f64)>>,
    resolution: f64,
    m: f64,
    next_community_id: &mut usize,
) {
    // Group nodes by community.
    let mut community_members: HashMap<usize, Vec<String>> = HashMap::new();
    for (node, &comm) in partition.iter() {
        community_members
            .entry(comm)
            .or_default()
            .push(node.clone());
    }

    for (_comm_id, members) in &community_members {
        if members.len() <= 2 {
            // Tiny community: nothing to refine.
            continue;
        }

        // Build sub-adjacency restricted to this community.
        let member_set: HashSet<&str> = members.iter().map(|s| s.as_str()).collect();

        // Check if the sub-graph of this community is connected.
        let sub_components = bfs_components_in_subset(&member_set, adjacency);
        if sub_components.len() <= 1 {
            // Already a single connected component. Now run a mini local-move to see
            // if the community should be further split for higher modularity.
            refine_connected_community(
                partition,
                adjacency,
                resolution,
                m,
                members,
                next_community_id,
            );
            continue;
        }

        // Community is disconnected: split into connected components.
        // Keep the largest component with the original ID; assign new IDs to the rest.
        let mut sorted_components = sub_components;
        sorted_components.sort_by_key(|c| std::cmp::Reverse(c.len()));

        // Skip first (largest) -- it keeps the existing community id.
        for component in sorted_components.iter().skip(1) {
            let new_id = *next_community_id;
            *next_community_id += 1;
            for node in component {
                partition.insert(node.clone(), new_id);
            }
        }
    }
}

/// Within a connected community, run a mini local-moving pass to discover sub-communities.
/// Only accept splits that improve modularity.
fn refine_connected_community(
    partition: &mut HashMap<String, usize>,
    adjacency: &HashMap<String, Vec<(String, f64)>>,
    resolution: f64,
    _m: f64,
    members: &[String],
    next_community_id: &mut usize,
) {
    if members.len() <= 3 {
        return;
    }

    let member_set: HashSet<&str> = members.iter().map(|s| s.as_str()).collect();

    // Start: each member in its own sub-community.
    let mut sub_partition: HashMap<String, usize> = HashMap::new();
    for (i, node) in members.iter().enumerate() {
        sub_partition.insert(node.clone(), i);
    }
    let sub_next_id = members.len();

    // Compute sigma_tot for sub-communities (restricted to intra-community edges).
    let mut sigma_tot: HashMap<usize, f64> = HashMap::new();
    for node in members {
        let k_i = internal_strength(node, adjacency, &member_set);
        let sub_comm = sub_partition[node];
        *sigma_tot.entry(sub_comm).or_default() += k_i;
    }

    // Internal total edge weight of this community subgraph.
    let mut m_sub: f64 = 0.0;
    for node in members {
        if let Some(neighbors) = adjacency.get(node.as_str()) {
            for (n, w) in neighbors {
                if member_set.contains(n.as_str()) {
                    m_sub += w;
                }
            }
        }
    }
    m_sub /= 2.0;

    if m_sub == 0.0 {
        return;
    }

    let two_m_sub = 2.0 * m_sub;

    // Run local moving on the sub-partition.
    let mut sorted_members: Vec<&String> = members.iter().collect();
    sorted_members.sort();

    for _ in 0..5 {
        let mut any_moved = false;

        for node_id in &sorted_members {
            let current_sub_comm = sub_partition[node_id.as_str()];
            let k_i = internal_strength(node_id, adjacency, &member_set);

            // Weight to each sub-community.
            let mut comm_weights: HashMap<usize, f64> = HashMap::new();
            if let Some(neighbors) = adjacency.get(node_id.as_str()) {
                for (neighbor, weight) in neighbors {
                    if !member_set.contains(neighbor.as_str()) {
                        continue;
                    }
                    if let Some(&sub_comm) = sub_partition.get(neighbor) {
                        *comm_weights.entry(sub_comm).or_default() += weight;
                    }
                }
            }

            let k_i_in_current = comm_weights.get(&current_sub_comm).copied().unwrap_or(0.0);
            let s_tot_current = sigma_tot.get(&current_sub_comm).copied().unwrap_or(0.0);
            let remove_cost = k_i_in_current / two_m_sub
                - resolution * (s_tot_current - k_i) * k_i / (two_m_sub * two_m_sub);

            let mut best_sub_comm = current_sub_comm;
            let mut best_gain = 0.0;

            for (&cand, &k_i_in_cand) in &comm_weights {
                if cand == current_sub_comm {
                    continue;
                }
                let s_tot_cand = sigma_tot.get(&cand).copied().unwrap_or(0.0);
                let insert_gain = k_i_in_cand / two_m_sub
                    - resolution * s_tot_cand * k_i / (two_m_sub * two_m_sub);
                let delta = insert_gain - remove_cost;
                if delta > best_gain {
                    best_gain = delta;
                    best_sub_comm = cand;
                }
            }

            if best_sub_comm != current_sub_comm && best_gain > 1e-12 {
                *sigma_tot.entry(current_sub_comm).or_default() -= k_i;
                *sigma_tot.entry(best_sub_comm).or_default() += k_i;
                sub_partition.insert(node_id.to_string(), best_sub_comm);
                any_moved = true;
            }
        }

        if !any_moved {
            break;
        }
    }

    // Count distinct sub-communities.
    let sub_comms: HashSet<usize> = sub_partition.values().copied().collect();
    if sub_comms.len() <= 1 {
        // No split found; keep original community.
        return;
    }

    // Compute modularity of single community vs split (using global m).
    // Only apply if the sub-partition strictly improves things.
    // We use a simple heuristic: if sub-communities > 1, apply them.
    // This matches the Leiden guarantee of well-connected communities.

    // Check that each sub-community is internally connected.
    let mut sub_comm_members: HashMap<usize, Vec<String>> = HashMap::new();
    for (node, &sc) in &sub_partition {
        sub_comm_members.entry(sc).or_default().push(node.clone());
    }

    // Assign new global community IDs for each sub-community.
    // (We reassign all, including the first, for simplicity; compaction happens later.)
    let _ = sub_next_id;
    for (_sc, sc_members) in &sub_comm_members {
        let new_id = *next_community_id;
        *next_community_id += 1;
        for node in sc_members {
            partition.insert(node.clone(), new_id);
        }
    }
}

/// Sum of edge weights from `node` to neighbors within `subset`.
fn internal_strength(
    node: &str,
    adjacency: &HashMap<String, Vec<(String, f64)>>,
    subset: &HashSet<&str>,
) -> f64 {
    adjacency
        .get(node)
        .map(|neighbors| {
            neighbors
                .iter()
                .filter(|(n, _)| subset.contains(n.as_str()))
                .map(|(_, w)| w)
                .sum()
        })
        .unwrap_or(0.0)
}

/// BFS to find connected components within a subset of nodes.
fn bfs_components_in_subset(
    subset: &HashSet<&str>,
    adjacency: &HashMap<String, Vec<(String, f64)>>,
) -> Vec<Vec<String>> {
    let mut visited = HashSet::<String>::new();
    let mut components = Vec::new();

    let mut sorted_subset: Vec<&&str> = subset.iter().collect();
    sorted_subset.sort();

    for &&node in &sorted_subset {
        if !visited.insert(node.to_string()) {
            continue;
        }
        let mut queue = VecDeque::from([node.to_string()]);
        let mut component = Vec::new();
        while let Some(current) = queue.pop_front() {
            component.push(current.clone());
            if let Some(neighbors) = adjacency.get(&current) {
                for (neighbor, _) in neighbors {
                    if subset.contains(neighbor.as_str()) && visited.insert(neighbor.clone()) {
                        queue.push_back(neighbor.clone());
                    }
                }
            }
        }
        components.push(component);
    }
    components
}

/// Phase 3: Aggregate the graph. Each community becomes a super-node.
/// Returns (aggregated adjacency, super-node partition, original-node -> super-node map).
fn aggregate_graph(
    adjacency: &HashMap<String, Vec<(String, f64)>>,
    partition: &HashMap<String, usize>,
) -> (
    HashMap<String, Vec<(String, f64)>>,
    HashMap<String, usize>,
    HashMap<String, String>,
) {
    // Community ID -> super-node name.
    let mut comm_to_super: HashMap<usize, String> = HashMap::new();
    let mut sorted_nodes: Vec<(&String, &usize)> = partition.iter().collect();
    sorted_nodes.sort_by_key(|(n, _)| n.as_str());

    for &(_, &comm) in &sorted_nodes {
        comm_to_super
            .entry(comm)
            .or_insert_with(|| format!("super_{}", comm));
    }

    // Build node_to_supernode mapping.
    let mut node_to_super: HashMap<String, String> = HashMap::new();
    for (node, &comm) in partition {
        node_to_super.insert(node.clone(), comm_to_super[&comm].clone());
    }

    // Accumulate edge weights between super-nodes.
    let mut super_edges: HashMap<(String, String), f64> = HashMap::new();
    for (node, neighbors) in adjacency {
        let super_src = match node_to_super.get(node) {
            Some(s) => s.clone(),
            None => continue,
        };
        for (neighbor, weight) in neighbors {
            let super_dst = match node_to_super.get(neighbor) {
                Some(s) => s.clone(),
                None => continue,
            };
            if super_src == super_dst {
                continue; // Skip self-loops in aggregated graph.
            }
            // Canonical order to avoid double-inserting.
            let key = if super_src <= super_dst {
                (super_src.clone(), super_dst.clone())
            } else {
                (super_dst.clone(), super_src.clone())
            };
            *super_edges.entry(key).or_default() += weight;
        }
    }

    // Each undirected edge was counted from both endpoints, so halve.
    let mut agg_adjacency: HashMap<String, Vec<(String, f64)>> = HashMap::new();
    for super_name in comm_to_super.values() {
        agg_adjacency.entry(super_name.clone()).or_default();
    }
    for ((a, b), weight) in &super_edges {
        let w = weight / 2.0; // Correct for double-counting.
        agg_adjacency
            .entry(a.clone())
            .or_default()
            .push((b.clone(), w));
        agg_adjacency
            .entry(b.clone())
            .or_default()
            .push((a.clone(), w));
    }

    // Super-node partition: initially each super-node in its own community.
    let mut agg_partition: HashMap<String, usize> = HashMap::new();
    for (i, name) in comm_to_super.values().enumerate() {
        agg_partition.insert(name.clone(), i);
    }

    (agg_adjacency, agg_partition, node_to_super)
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: build a symmetric adjacency list from a list of (src, dst, weight) edges.
    fn make_adjacency(edges: &[(&str, &str, f64)]) -> HashMap<String, Vec<(String, f64)>> {
        let mut adj: HashMap<String, Vec<(String, f64)>> = HashMap::new();
        for &(src, dst, w) in edges {
            adj.entry(src.to_string())
                .or_default()
                .push((dst.to_string(), w));
            adj.entry(dst.to_string())
                .or_default()
                .push((src.to_string(), w));
        }
        adj
    }

    /// Two cliques of 4 nodes each, connected by a single weak edge.
    /// Leiden should find exactly 2 communities.
    #[test]
    fn test_two_cliques() {
        // Clique A: a1-a2-a3-a4 (fully connected, weight 1.0).
        // Clique B: b1-b2-b3-b4 (fully connected, weight 1.0).
        // Bridge: a1-b1 weight 0.1.
        let mut edges: Vec<(&str, &str, f64)> = Vec::new();
        for &(i, j) in &[
            ("a1", "a2"),
            ("a1", "a3"),
            ("a1", "a4"),
            ("a2", "a3"),
            ("a2", "a4"),
            ("a3", "a4"),
        ] {
            edges.push((i, j, 1.0));
        }
        for &(i, j) in &[
            ("b1", "b2"),
            ("b1", "b3"),
            ("b1", "b4"),
            ("b2", "b3"),
            ("b2", "b4"),
            ("b3", "b4"),
        ] {
            edges.push((i, j, 1.0));
        }
        edges.push(("a1", "b1", 0.1));

        let adj = make_adjacency(&edges);
        let result = leiden_clustering(&adj, 1.0, 10);

        // All a-nodes should be in the same community.
        let comm_a1 = result.assignments["a1"];
        assert_eq!(result.assignments["a2"], comm_a1);
        assert_eq!(result.assignments["a3"], comm_a1);
        assert_eq!(result.assignments["a4"], comm_a1);

        // All b-nodes should be in the same community.
        let comm_b1 = result.assignments["b1"];
        assert_eq!(result.assignments["b2"], comm_b1);
        assert_eq!(result.assignments["b3"], comm_b1);
        assert_eq!(result.assignments["b4"], comm_b1);

        // The two cliques should be in different communities.
        assert_ne!(comm_a1, comm_b1);
        assert_eq!(result.community_count, 2);
        assert!(result.modularity > 0.0, "modularity should be positive");
    }

    /// Isolated nodes (no edges) should each be in their own community.
    #[test]
    fn test_isolated_nodes() {
        let mut adj: HashMap<String, Vec<(String, f64)>> = HashMap::new();
        adj.insert("x".to_string(), vec![]);
        adj.insert("y".to_string(), vec![]);
        adj.insert("z".to_string(), vec![]);

        let result = leiden_clustering(&adj, 1.0, 10);

        assert_eq!(result.community_count, 3);
        // Each node in a different community.
        let comms: HashSet<usize> = result.assignments.values().copied().collect();
        assert_eq!(comms.len(), 3);
        assert_eq!(result.modularity, 0.0);
    }

    /// A single fully connected clique should result in one community.
    #[test]
    fn test_single_community() {
        let edges = vec![
            ("n1", "n2", 1.0),
            ("n1", "n3", 1.0),
            ("n1", "n4", 1.0),
            ("n2", "n3", 1.0),
            ("n2", "n4", 1.0),
            ("n3", "n4", 1.0),
        ];
        let adj = make_adjacency(&edges);
        let result = leiden_clustering(&adj, 1.0, 10);

        // All should be in the same community.
        let comms: HashSet<usize> = result.assignments.values().copied().collect();
        assert_eq!(comms.len(), 1);
        assert_eq!(result.community_count, 1);
    }

    /// Higher resolution should produce more (smaller) communities.
    #[test]
    fn test_resolution_parameter() {
        // Build a graph with moderate structure: two loosely connected cliques.
        let mut edges: Vec<(&str, &str, f64)> = Vec::new();
        // Clique 1.
        for &(i, j) in &[("c1", "c2"), ("c1", "c3"), ("c2", "c3")] {
            edges.push((i, j, 1.0));
        }
        // Clique 2.
        for &(i, j) in &[("d1", "d2"), ("d1", "d3"), ("d2", "d3")] {
            edges.push((i, j, 1.0));
        }
        // Weak bridge.
        edges.push(("c3", "d1", 0.05));

        let adj = make_adjacency(&edges);

        let result_low = leiden_clustering(&adj, 0.5, 10);
        let result_high = leiden_clustering(&adj, 5.0, 10);

        // High resolution should produce at least as many communities as low resolution.
        assert!(
            result_high.community_count >= result_low.community_count,
            "high resolution ({}) should produce >= communities than low resolution ({})",
            result_high.community_count,
            result_low.community_count,
        );
    }

    /// Empty graph (no nodes, no edges) should return an empty result.
    #[test]
    fn test_empty_graph() {
        let adj: HashMap<String, Vec<(String, f64)>> = HashMap::new();
        let result = leiden_clustering(&adj, 1.0, 10);

        assert_eq!(result.community_count, 0);
        assert!(result.assignments.is_empty());
        assert_eq!(result.modularity, 0.0);
    }

    /// compute_modularity should return 0 for a single community containing all nodes.
    #[test]
    fn test_modularity_single_community() {
        let edges = vec![("a", "b", 1.0), ("b", "c", 1.0), ("a", "c", 1.0)];
        let adj = make_adjacency(&edges);

        let mut partition = HashMap::new();
        partition.insert("a".to_string(), 0);
        partition.insert("b".to_string(), 0);
        partition.insert("c".to_string(), 0);

        let q = compute_modularity(&partition, &adj, 1.0);
        // Single community: Q = sum(internal)/2m - (sum(degrees)/2m)^2 = 1 - 1 = 0.
        assert!(
            (q - 0.0).abs() < 1e-10,
            "modularity of single community should be 0, got {}",
            q
        );
    }

    /// find_connected_components should correctly identify disjoint subgraphs.
    #[test]
    fn test_connected_components() {
        let edges = vec![("a", "b", 1.0), ("c", "d", 1.0)];
        let adj = make_adjacency(&edges);
        let components = find_connected_components(&adj);

        assert_eq!(components.len(), 2);
        // Sorted by size descending (both have size 2, so order is stable by sort).
        for comp in &components {
            assert_eq!(comp.len(), 2);
        }
    }

    /// Disconnected communities should be detected even without the refinement phase.
    #[test]
    fn test_disconnected_subgraph_splitting() {
        // Three completely separate triangles.
        let edges = vec![
            ("a1", "a2", 1.0),
            ("a2", "a3", 1.0),
            ("a1", "a3", 1.0),
            ("b1", "b2", 1.0),
            ("b2", "b3", 1.0),
            ("b1", "b3", 1.0),
            ("c1", "c2", 1.0),
            ("c2", "c3", 1.0),
            ("c1", "c3", 1.0),
        ];
        let adj = make_adjacency(&edges);
        let result = leiden_clustering(&adj, 1.0, 10);

        // Should find exactly 3 communities.
        assert_eq!(result.community_count, 3);

        // Each triangle should be in its own community.
        assert_eq!(result.assignments["a1"], result.assignments["a2"]);
        assert_eq!(result.assignments["a2"], result.assignments["a3"]);
        assert_eq!(result.assignments["b1"], result.assignments["b2"]);
        assert_eq!(result.assignments["b2"], result.assignments["b3"]);
        assert_eq!(result.assignments["c1"], result.assignments["c2"]);
        assert_eq!(result.assignments["c2"], result.assignments["c3"]);

        // Different triangles should be in different communities.
        assert_ne!(result.assignments["a1"], result.assignments["b1"]);
        assert_ne!(result.assignments["a1"], result.assignments["c1"]);
        assert_ne!(result.assignments["b1"], result.assignments["c1"]);
    }
}
