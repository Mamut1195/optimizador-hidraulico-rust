//! Steiner tree approximation — Mehlhorn 2-approximation algorithm.
//!
//! Mirrors `nx.approximation.steiner_tree(G, terminal_nodes, weight="weight")`.
//!
//! ## Algorithm (Mehlhorn 1988)
//!
//! 1. **Multi-source Voronoi partition**: run a single multi-source Dijkstra from
//!    all terminals simultaneously. Each non-terminal vertex is assigned to its
//!    nearest terminal (`voronoi_terminal[v]`) and the shortest distance to it
//!    (`voronoi_dist[v]`).
//!
//! 2. **Metric closure on terminals**: for each edge `(u, v, w)` in the original
//!    graph where `voronoi_terminal[u] != voronoi_terminal[v]`, add a closure edge
//!    between the two terminals with weight `voronoi_dist[u] + w + voronoi_dist[v]`.
//!
//! 3. **MST of closure** (Kruskal on terminal pairs).
//!
//! 4. **Expand closure MST edges** back to their underlying shortest paths in the
//!    original graph; union all path edges into a subgraph.
//!
//! 5. **MST of the path-subgraph** (Kruskal again).
//!
//! 6. **Prune non-terminal leaves** iteratively until none remain.
//!
//! ## Parity guarantees
//!
//! - Connectivity (all terminals reachable): **EXACT**.
//! - Tree structure (`edge_count == node_count - 1`): **EXACT**.
//! - Total weight vs Python oracle: **STATISTICAL** (≤ 1.10× oracle).
//! - `variant > 0` weight: **connectivity EXACT only** (cross-language RNG differs).

use std::collections::{BinaryHeap, HashMap, HashSet};

use crate::error::TerrainError;
use crate::graph::TerrainGraph;

// ── SteinerTree result type ───────────────────────────────────────────────

/// The result of a Steiner tree computation.
///
/// Provides connectivity queries and weight aggregation used by tests.
#[derive(Debug, Clone)]
pub struct SteinerTree {
    /// Nodes (node ID strings) in the Steiner tree.
    nodes: Vec<String>,
    /// Edges as (node_a, node_b, weight) triples. Both orderings are stored
    /// for O(1) lookup in `is_connected`.
    edges: Vec<(String, String, f64)>,
    /// Adjacency map for BFS/DFS reachability checks.
    adj: HashMap<String, Vec<(String, f64)>>,
}

impl SteinerTree {
    fn new(nodes: Vec<String>, edges: Vec<(String, String, f64)>) -> Self {
        let mut adj: HashMap<String, Vec<(String, f64)>> = HashMap::new();
        for n in &nodes {
            adj.entry(n.clone()).or_default();
        }
        for (a, b, w) in &edges {
            adj.entry(a.clone()).or_default().push((b.clone(), *w));
            adj.entry(b.clone()).or_default().push((a.clone(), *w));
        }
        Self { nodes, edges, adj }
    }

    /// Return `true` if `node_id` is present in the Steiner tree.
    pub fn contains_node(&self, node_id: &str) -> bool {
        self.adj.contains_key(node_id)
    }

    /// Return `true` if `src` and `tgt` are in the same connected component
    /// of the Steiner tree (BFS).
    pub fn is_connected(&self, src: &str, tgt: &str) -> bool {
        if !self.adj.contains_key(src) || !self.adj.contains_key(tgt) {
            return false;
        }
        if src == tgt {
            return true;
        }
        // BFS
        let mut visited = HashSet::new();
        let mut queue = std::collections::VecDeque::new();
        queue.push_back(src.to_string());
        visited.insert(src.to_string());
        while let Some(cur) = queue.pop_front() {
            if cur == tgt {
                return true;
            }
            if let Some(neighbors) = self.adj.get(&cur) {
                for (nb, _) in neighbors {
                    if !visited.contains(nb.as_str()) {
                        visited.insert(nb.clone());
                        queue.push_back(nb.clone());
                    }
                }
            }
        }
        false
    }

    /// Total edge weight of the Steiner tree.
    pub fn total_weight(&self) -> f64 {
        self.edges.iter().map(|(_, _, w)| w).sum()
    }

    /// Number of nodes in the Steiner tree.
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Number of edges in the Steiner tree.
    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }

    /// Return all edges as `(node_a, node_b, weight)` triples.
    ///
    /// Each edge appears once (as inserted, not deduplicated). Used by downstream
    /// solvers (e.g., SewerSolver) to convert the Steiner tree into a directed graph.
    pub fn edges(&self) -> &[(String, String, f64)] {
        &self.edges
    }

    /// Return all node IDs in the Steiner tree.
    pub fn node_ids(&self) -> &[String] {
        &self.nodes
    }

    /// Return the adjacency map (undirected): node → [(neighbor, weight)].
    pub fn adjacency(&self) -> &HashMap<String, Vec<(String, f64)>> {
        &self.adj
    }

    /// Return sorted (lexicographic) edge pairs for snapshot tests.
    ///
    /// Each pair is `(min(a,b), max(a,b))`.
    pub fn sorted_edges(&self) -> Vec<(String, String)> {
        let mut pairs: Vec<(String, String)> = self
            .edges
            .iter()
            .map(|(a, b, _)| {
                if a <= b {
                    (a.clone(), b.clone())
                } else {
                    (b.clone(), a.clone())
                }
            })
            .collect();
        pairs.sort();
        pairs.dedup();
        pairs
    }
}

// ── Internal graph helper types ───────────────────────────────────────────

/// A compact undirected adjacency representation for Steiner internal use.
///
/// Node IDs are represented as `usize` indices into `node_names`.
#[derive(Debug)]
pub(crate) struct CompactGraph {
    /// Node ID string → compact index.
    name_to_idx: HashMap<String, usize>,
    /// compact index → node ID string.
    node_names: Vec<String>,
    /// Adjacency list: `adj[u]` = Vec of `(v, weight)`.
    adj: Vec<Vec<(usize, f64)>>,
}

impl CompactGraph {
    fn new(node_names: Vec<String>) -> Self {
        let n = node_names.len();
        let name_to_idx: HashMap<String, usize> = node_names
            .iter()
            .enumerate()
            .map(|(i, s)| (s.clone(), i))
            .collect();
        Self {
            name_to_idx,
            node_names,
            adj: vec![Vec::new(); n],
        }
    }

    fn node_count(&self) -> usize {
        self.node_names.len()
    }
}

// ── Priority queue entry for Dijkstra ────────────────────────────────────

/// Min-heap entry: `(neg_cost, node_idx)` — negate to turn BinaryHeap into min-heap.
#[derive(Debug, PartialEq)]
struct HeapEntry(f64, usize);

impl Eq for HeapEntry {}

impl Ord for HeapEntry {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Reverse for min-heap: smaller distance has higher priority
        other
            .0
            .partial_cmp(&self.0)
            .unwrap_or(std::cmp::Ordering::Equal)
    }
}

impl PartialOrd for HeapEntry {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

// ── Steiner tree public API ───────────────────────────────────────────────

/// Compute the Mehlhorn 2-approximation Steiner tree connecting `terminals`
/// in the given `TerrainGraph`.
///
/// # Parity
///
/// - Connectivity: EXACT (all terminals reachable from each other).
/// - Tree structure: EXACT (`edge_count == node_count - 1`).
/// - Total weight: STATISTICAL (≤ 1.10 × Python oracle for unperturbed graphs).
///
/// # Errors
///
/// Returns [`TerrainError::InsufficientTerminals`] if `terminals.len() < 2`.
pub fn steiner_tree(
    graph: &TerrainGraph,
    terminals: &[impl AsRef<str>],
) -> Result<SteinerTree, TerrainError> {
    let terminal_ids: Vec<String> = terminals.iter().map(|t| t.as_ref().to_string()).collect();

    if terminal_ids.len() < 2 {
        return Err(TerrainError::InsufficientTerminals {
            got: terminal_ids.len(),
        });
    }

    // Build compact graph representation
    let (compact, _) = graph.build_compact_inner()?;

    // Resolve terminal names to compact indices
    let mut terminal_indices: Vec<usize> = Vec::with_capacity(terminal_ids.len());
    for tid in &terminal_ids {
        let idx = compact
            .name_to_idx
            .get(tid)
            .copied()
            .ok_or_else(|| TerrainError::NodeNotFound { id: tid.clone() })?;
        terminal_indices.push(idx);
    }

    // Step 1: Multi-source Dijkstra → Voronoi partition
    let (voronoi_terminal, voronoi_dist) = multi_source_dijkstra(&compact, &terminal_indices);

    // Step 2: Build metric closure on terminals
    let closure = build_metric_closure(
        &compact,
        &terminal_indices,
        &voronoi_terminal,
        &voronoi_dist,
    );

    // Step 3: MST of closure
    let closure_mst = kruskal_mst_with_n(&closure, compact.node_count());

    // Step 4: Expand closure MST edges → path subgraph
    let path_subgraph = expand_closure_to_paths(
        &compact,
        &voronoi_terminal,
        &voronoi_dist,
        &closure_mst,
        &terminal_indices,
    );

    // Step 5: MST of path subgraph
    let final_tree_edges = kruskal_mst_with_n(&path_subgraph, compact.node_count());

    // Step 6: Build SteinerTree from final_tree_edges, then prune non-terminal leaves
    let terminal_set: HashSet<usize> = terminal_indices.iter().copied().collect();
    let result = build_steiner_result(
        &compact,
        &terminal_set,
        &final_tree_edges,
        compact.node_count(),
    );

    Ok(result)
}

// ── Step 1: Multi-source Dijkstra (Voronoi) ──────────────────────────────

/// Run multi-source Dijkstra from all terminal sources simultaneously.
///
/// Returns `(voronoi_terminal[v], voronoi_dist[v])` for every node `v`:
/// - `voronoi_terminal[v]` = index of the nearest terminal.
/// - `voronoi_dist[v]` = shortest distance from `v` to that terminal.
fn multi_source_dijkstra(
    graph: &CompactGraph,
    terminal_indices: &[usize],
) -> (Vec<usize>, Vec<f64>) {
    let n = graph.node_count();
    let mut dist = vec![f64::INFINITY; n];
    let mut voronoi = vec![usize::MAX; n];
    let mut heap = BinaryHeap::new();

    for &t in terminal_indices {
        dist[t] = 0.0;
        voronoi[t] = t;
        heap.push(HeapEntry(0.0, t));
    }

    while let Some(HeapEntry(d, u)) = heap.pop() {
        if d > dist[u] {
            continue; // stale entry
        }
        for &(v, w) in &graph.adj[u] {
            let nd = d + w;
            if nd < dist[v] {
                dist[v] = nd;
                voronoi[v] = voronoi[u];
                heap.push(HeapEntry(nd, v));
            }
        }
    }

    (voronoi, dist)
}

// ── Step 2: Metric closure on terminals ──────────────────────────────────

/// A single edge in the metric closure (or path subgraph).
#[derive(Debug, Clone)]
struct ClosureEdge {
    /// Index of terminal (or graph node) u.
    u: usize,
    /// Index of terminal (or graph node) v.
    v: usize,
    /// Metric distance (voronoi_dist[u] + edge_weight + voronoi_dist[v]).
    weight: f64,
}

/// Build the metric closure on the terminal set.
///
/// For each original edge `(u_orig, v_orig, w)` where the two Voronoi regions differ,
/// add closure edge `(voronoi[u_orig], voronoi[v_orig])` with weight
/// `voronoi_dist[u_orig] + w + voronoi_dist[v_orig]`.
///
/// Deduplicates: keep the minimum-weight edge between each terminal pair.
fn build_metric_closure(
    graph: &CompactGraph,
    terminal_indices: &[usize],
    voronoi_terminal: &[usize],
    voronoi_dist: &[f64],
) -> Vec<ClosureEdge> {
    // Remap terminal indices to local 0..T-1 space
    let t_count = terminal_indices.len();
    let mut term_local: HashMap<usize, usize> = HashMap::new();
    for (local, &t) in terminal_indices.iter().enumerate() {
        term_local.insert(t, local);
    }

    // best_edge[(local_u, local_v)] = min weight seen
    let mut best: HashMap<(usize, usize), f64> = HashMap::new();

    for u in 0..graph.node_count() {
        let tu = voronoi_terminal[u];
        if tu == usize::MAX {
            continue;
        }
        for &(v, w) in &graph.adj[u] {
            if v <= u {
                continue; // process each undirected edge once
            }
            let tv = voronoi_terminal[v];
            if tv == usize::MAX || tu == tv {
                continue; // same Voronoi region — skip
            }
            let d = voronoi_dist[u] + w + voronoi_dist[v];
            let key = if tu < tv { (tu, tv) } else { (tv, tu) };
            let entry = best.entry(key).or_insert(f64::INFINITY);
            if d < *entry {
                *entry = d;
            }
        }
    }

    let _ = t_count; // silence warning

    best.into_iter()
        .map(|((u, v), w)| ClosureEdge { u, v, weight: w })
        .collect()
}

// ── Step 3: Kruskal MST ───────────────────────────────────────────────────

/// Union-Find for Kruskal.
struct UnionFind {
    parent: Vec<usize>,
    rank: Vec<usize>,
}

impl UnionFind {
    fn new(n: usize) -> Self {
        Self {
            parent: (0..n).collect(),
            rank: vec![0; n],
        }
    }

    fn find(&mut self, x: usize) -> usize {
        if self.parent[x] != x {
            self.parent[x] = self.find(self.parent[x]); // path compression
        }
        self.parent[x]
    }

    fn union(&mut self, x: usize, y: usize) -> bool {
        let rx = self.find(x);
        let ry = self.find(y);
        if rx == ry {
            return false;
        }
        if self.rank[rx] < self.rank[ry] {
            self.parent[rx] = ry;
        } else if self.rank[rx] > self.rank[ry] {
            self.parent[ry] = rx;
        } else {
            self.parent[ry] = rx;
            self.rank[rx] += 1;
        }
        true
    }
}

/// Kruskal MST over a list of edges (any node index space).
///
/// Returns the MST edge list (subset of the input).
/// Node indices must be in `0..n_nodes`.
fn kruskal_mst_with_n(edges: &[ClosureEdge], n_nodes: usize) -> Vec<ClosureEdge> {
    let mut sorted = edges.to_vec();
    // Sort by weight, then by node-pair as a deterministic tie-breaker. Without
    // the secondary key, equal-weight edges would order by HashMap iteration
    // order, which is non-deterministic across runs/compiler versions and would
    // break reproducibility (this port's core invariant).
    sorted.sort_by(|a, b| {
        a.weight
            .partial_cmp(&b.weight)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| (a.u.min(a.v), a.u.max(a.v)).cmp(&(b.u.min(b.v), b.u.max(b.v))))
    });

    let mut uf = UnionFind::new(n_nodes);
    let mut mst = Vec::new();

    for e in sorted {
        if uf.union(e.u, e.v) {
            mst.push(e);
        }
    }
    mst
}

// ── Step 4: Expand closure MST to path subgraph ──────────────────────────

/// For each closure MST edge `(T_u, T_v)`, reconstruct the underlying shortest path
/// through the Voronoi regions and add all path edges to a subgraph.
///
/// The path goes:  T_u → ... → crossing edge → ... → T_v
///
/// We find the original edge `(u_orig, v_orig)` that achieves the minimum
/// `voronoi_dist[u_orig] + w + voronoi_dist[v_orig]` for the given terminal pair,
/// then trace back the Voronoi paths from u_orig and v_orig to their terminals.
///
/// Returns a flat list of `ClosureEdge` representing the path subgraph (in original
/// graph node indices).
fn expand_closure_to_paths(
    graph: &CompactGraph,
    voronoi_terminal: &[usize],
    voronoi_dist: &[f64],
    closure_mst: &[ClosureEdge],
    _terminal_indices: &[usize],
) -> Vec<ClosureEdge> {
    // Build a reverse-path map: for each node v, which neighbor is on the shortest
    // path toward its voronoi terminal?
    // We reconstruct by re-running Dijkstra per closure MST edge.
    // Efficient enough for the 11×11 fixture (~121 nodes) and in general for small
    // Steiner instances (the Mehlhorn algorithm is designed for sparse terminals).

    let mut path_edges: Vec<ClosureEdge> = Vec::new();

    for closure_edge in closure_mst {
        let tu = closure_edge.u;
        let tv = closure_edge.v;

        // Find the original crossing edge (u_orig, v_orig) that bridges tu→tv regions
        let (u_cross, v_cross, w_cross) =
            find_best_crossing_edge(graph, voronoi_terminal, voronoi_dist, tu, tv);

        // Trace path from tu to u_cross (Voronoi path)
        let path_tu = trace_voronoi_path(graph, voronoi_terminal, voronoi_dist, u_cross, tu);

        // Trace path from tv to v_cross (Voronoi path)
        let path_tv = trace_voronoi_path(graph, voronoi_terminal, voronoi_dist, v_cross, tv);

        // Add all path edges
        for &(a, b, w) in &path_tu {
            path_edges.push(ClosureEdge {
                u: a,
                v: b,
                weight: w,
            });
        }
        // Add crossing edge
        path_edges.push(ClosureEdge {
            u: u_cross,
            v: v_cross,
            weight: w_cross,
        });
        for &(a, b, w) in &path_tv {
            path_edges.push(ClosureEdge {
                u: a,
                v: b,
                weight: w,
            });
        }
    }

    // Deduplicate by (min(u,v), max(u,v)) — keep minimum weight
    dedup_path_edges(path_edges)
}

/// Find the original graph edge `(u, v, w)` with the minimum
/// `voronoi_dist[u] + w + voronoi_dist[v]` where `voronoi[u] == tu` and `voronoi[v] == tv`.
fn find_best_crossing_edge(
    graph: &CompactGraph,
    voronoi_terminal: &[usize],
    voronoi_dist: &[f64],
    tu: usize,
    tv: usize,
) -> (usize, usize, f64) {
    let mut best = (0, 0, f64::INFINITY);
    for u in 0..graph.node_count() {
        if voronoi_terminal[u] != tu {
            continue;
        }
        for &(v, w) in &graph.adj[u] {
            if voronoi_terminal[v] != tv {
                continue;
            }
            let d = voronoi_dist[u] + w + voronoi_dist[v];
            if d < best.2 {
                best = (u, v, w);
            }
        }
    }
    best
}

/// Trace the shortest path from `node` back to `target_terminal` using Dijkstra
/// over the original graph, restricted to the node's Voronoi region.
///
/// Returns list of `(u, v, w)` tuples forming the path edges.
fn trace_voronoi_path(
    graph: &CompactGraph,
    voronoi_terminal: &[usize],
    _voronoi_dist: &[f64],
    start: usize,
    target_terminal: usize,
) -> Vec<(usize, usize, f64)> {
    if start == target_terminal {
        return Vec::new();
    }

    // Dijkstra from target_terminal, find path to start
    let n = graph.node_count();
    let mut dist = vec![f64::INFINITY; n];
    let mut prev: Vec<Option<(usize, f64)>> = vec![None; n]; // (prev_node, edge_weight)
    let mut heap = BinaryHeap::new();

    dist[target_terminal] = 0.0;
    heap.push(HeapEntry(0.0, target_terminal));

    while let Some(HeapEntry(d, u)) = heap.pop() {
        if d > dist[u] {
            continue;
        }
        if u == start {
            break;
        }
        for &(v, w) in &graph.adj[u] {
            // Stay within the same Voronoi region (both voronoi_terminal == target_terminal)
            // OR allow crossing through the target terminal itself
            if voronoi_terminal[v] != target_terminal && v != target_terminal {
                continue;
            }
            let nd = d + w;
            if nd < dist[v] {
                dist[v] = nd;
                prev[v] = Some((u, w));
                heap.push(HeapEntry(nd, v));
            }
        }
    }

    // If we didn't reach start via Voronoi-constrained path, fall back to
    // full Dijkstra (handles edge cases where start is on boundary)
    if dist[start] == f64::INFINITY {
        return trace_full_dijkstra(graph, start, target_terminal);
    }

    // Reconstruct path
    let mut edges = Vec::new();
    let mut cur = start;
    while let Some((p, w)) = prev[cur] {
        edges.push((p, cur, w));
        cur = p;
    }
    edges
}

/// Full Dijkstra path from `start` to `target` (fallback — no Voronoi constraint).
fn trace_full_dijkstra(
    graph: &CompactGraph,
    start: usize,
    target: usize,
) -> Vec<(usize, usize, f64)> {
    let n = graph.node_count();
    let mut dist = vec![f64::INFINITY; n];
    let mut prev: Vec<Option<(usize, f64)>> = vec![None; n];
    let mut heap = BinaryHeap::new();

    dist[target] = 0.0;
    heap.push(HeapEntry(0.0, target));

    while let Some(HeapEntry(d, u)) = heap.pop() {
        if d > dist[u] {
            continue;
        }
        if u == start {
            break;
        }
        for &(v, w) in &graph.adj[u] {
            let nd = d + w;
            if nd < dist[v] {
                dist[v] = nd;
                prev[v] = Some((u, w));
                heap.push(HeapEntry(nd, v));
            }
        }
    }

    let mut edges = Vec::new();
    let mut cur = start;
    while let Some((p, w)) = prev[cur] {
        edges.push((p, cur, w));
        cur = p;
    }
    edges
}

/// Deduplicate path edges: for each undirected pair `(min(u,v), max(u,v))`,
/// keep the entry with the minimum weight.
fn dedup_path_edges(edges: Vec<ClosureEdge>) -> Vec<ClosureEdge> {
    let mut best: HashMap<(usize, usize), f64> = HashMap::new();
    for e in edges {
        let key = if e.u < e.v { (e.u, e.v) } else { (e.v, e.u) };
        let entry = best.entry(key).or_insert(f64::INFINITY);
        if e.weight < *entry {
            *entry = e.weight;
        }
    }
    best.into_iter()
        .map(|((u, v), w)| ClosureEdge { u, v, weight: w })
        .collect()
}

// ── Step 6: Build final SteinerTree + prune non-terminal leaves ──────────

/// Build the final `SteinerTree` from the MST edge list, then prune non-terminal
/// leaves until no non-terminal leaves remain.
fn build_steiner_result(
    compact: &CompactGraph,
    terminal_set: &HashSet<usize>,
    mst_edges: &[ClosureEdge],
    _n: usize,
) -> SteinerTree {
    // Collect all node indices present in the MST
    let mut node_set: HashSet<usize> = HashSet::new();
    let mut adj_map: HashMap<usize, Vec<(usize, f64)>> = HashMap::new();

    for e in mst_edges {
        node_set.insert(e.u);
        node_set.insert(e.v);
        adj_map.entry(e.u).or_default().push((e.v, e.weight));
        adj_map.entry(e.v).or_default().push((e.u, e.weight));
    }
    for n in &node_set {
        adj_map.entry(*n).or_default();
    }

    // Iteratively prune non-terminal leaves
    loop {
        let leaves: Vec<usize> = node_set
            .iter()
            .filter(|&&n| !terminal_set.contains(&n) && adj_map.get(&n).map_or(0, |v| v.len()) == 1)
            .copied()
            .collect();

        if leaves.is_empty() {
            break;
        }

        for leaf in leaves {
            node_set.remove(&leaf);
            if let Some(neighbors) = adj_map.remove(&leaf) {
                for (nb, _) in neighbors {
                    if let Some(nb_adj) = adj_map.get_mut(&nb) {
                        nb_adj.retain(|(v, _)| *v != leaf);
                    }
                }
            }
        }
    }

    // Rebuild clean edge list
    let mut seen_edges: HashSet<(usize, usize)> = HashSet::new();
    let mut final_edges: Vec<(String, String, f64)> = Vec::new();
    let mut final_nodes: Vec<String> = Vec::new();

    for &n in &node_set {
        final_nodes.push(compact.node_names[n].clone());
    }
    final_nodes.sort();

    for &u in &node_set {
        if let Some(neighbors) = adj_map.get(&u) {
            for &(v, w) in neighbors {
                if node_set.contains(&v) {
                    let key = if u < v { (u, v) } else { (v, u) };
                    if seen_edges.insert(key) {
                        final_edges.push((
                            compact.node_names[u].clone(),
                            compact.node_names[v].clone(),
                            w,
                        ));
                    }
                }
            }
        }
    }

    SteinerTree::new(final_nodes, final_edges)
}

// ── Internal TerrainGraph methods needed by steiner ───────────────────────

impl TerrainGraph {
    /// Build a `CompactGraph` from the petgraph inner graph.
    ///
    /// Returns `(CompactGraph, _)` — the second element is unused here; terminal
    /// indices are resolved by the caller via `node_id_to_compact`.
    pub(crate) fn build_compact_inner(&self) -> Result<(CompactGraph, Vec<usize>), TerrainError> {
        let node_ids: Vec<String> = self.raw_node_ids().into_iter().collect();

        let mut compact = CompactGraph::new(node_ids);

        for (u_name, v_name, w) in self.raw_edges() {
            if let (Some(&ui), Some(&vi)) = (
                compact.name_to_idx.get(&u_name),
                compact.name_to_idx.get(&v_name),
            ) {
                compact.adj[ui].push((vi, w));
                compact.adj[vi].push((ui, w));
            }
        }

        Ok((compact, Vec::new()))
    }

    /// Return all node ID strings in the graph.
    pub fn raw_node_ids(&self) -> Vec<String> {
        self.inner_graph()
            .node_weights()
            .map(|n| n.id.clone())
            .collect()
    }

    /// Return all edges as `(u_id, v_id, weight)` triples.
    pub fn raw_edges(&self) -> Vec<(String, String, f64)> {
        let g = self.inner_graph();
        g.edge_indices()
            .map(|ei| {
                let (u, v) = g.edge_endpoints(ei).unwrap();
                let w = g[ei].weight;
                (g[u].id.clone(), g[v].id.clone(), w)
            })
            .collect()
    }
}
