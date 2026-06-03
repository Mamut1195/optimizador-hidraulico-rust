//! SolverGraph — crate-private DiGraph wrapper for hydraulic solver traversals.
//!
//! Provides a directed petgraph `DiGraph<String, EdgeWeight, u32>` with:
//! - O(1) `String → NodeIndex` lookup via `HashMap`
//! - Deterministic, String-sorted traversal helpers (`sorted_neighbors`,
//!   `sorted_kahn_topo_sort`, `bfs_oriented`)
//!
//! # Determinism contract
//!
//! All helpers that return ordered sequences sort by the underlying `String`
//! node id (lexicographic ascending). This replicates the hand-rolled Kahn
//! algorithm in `sewer.rs:1089-1136` (the pre-existing `queue_vec.sort()`
//! behaviour) and must be preserved bit-for-bit for oracle-parity.
//!
//! REQ-007: do NOT use `petgraph::algo::toposort`; use `sorted_kahn_topo_sort`
//! instead.
//!
//! # Ready-set ordering
//!
//! `sorted_kahn_topo_sort` uses `BTreeSet<(String, NodeIndex<u32>)>` as the
//! ready set. This gives ascending-by-String pop order in O(log n) per
//! insert/pop without re-sorting a Vec on each iteration. The `(String,
//! NodeIndex)` tuple gives a total order: primary key is the node id string,
//! secondary is the raw `NodeIndex` index value (a `u32` under the default
//! petgraph `Ix = u32`). In practice the String key alone determines order
//! because node ids are unique; the NodeIndex tiebreak prevents any theoretical
//! BTreeSet collision on identical Strings (which cannot happen in a valid
//! SolverGraph).

use std::collections::{BTreeSet, HashMap, VecDeque};

use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::visit::EdgeRef;

use crate::error::SolverError;

/// Edge weight type used by SolverGraph.
///
/// Solvers use `f64` for length / cost / capacity uniformly today; using a
/// concrete type alias keeps the door open for richer tuple types without
/// churning solver call sites.
pub(crate) type EdgeWeight = f64;

/// Crate-private DiGraph wrapper with `String ↔ NodeIndex` lookup and
/// deterministic, String-sorted traversal helpers.
///
/// Lifecycle:
///   build phase  → `add_node` / `add_edge` (mutating)
///   query phase  → `node_index` / `node_id` / `sorted_neighbors` /
///                  `sorted_kahn_topo_sort` / `bfs_oriented`
pub(crate) struct SolverGraph {
    graph: DiGraph<String, EdgeWeight, u32>,
    id_to_idx: HashMap<String, NodeIndex<u32>>,
}

impl SolverGraph {
    /// Create a new, empty `SolverGraph`.
    pub(crate) fn new() -> Self {
        Self {
            graph: DiGraph::new(),
            id_to_idx: HashMap::new(),
        }
    }

    /// Add a node by its `String` id. Idempotent: returns the existing
    /// `NodeIndex` if `id` is already in the graph.
    pub(crate) fn add_node(&mut self, id: &str) -> NodeIndex<u32> {
        if let Some(&existing) = self.id_to_idx.get(id) {
            return existing;
        }
        let idx = self.graph.add_node(id.to_owned());
        self.id_to_idx.insert(id.to_owned(), idx);
        idx
    }

    /// Add a directed edge `from → to` with weight `w`. Both endpoints are
    /// inserted (idempotently) if missing.
    pub(crate) fn add_edge(&mut self, from: &str, to: &str, w: EdgeWeight) {
        let from_idx = self.add_node(from);
        let to_idx = self.add_node(to);
        self.graph.add_edge(from_idx, to_idx, w);
    }

    /// Look up `NodeIndex` for a `String` id. Returns `None` for unknown ids.
    pub(crate) fn node_index(&self, id: &str) -> Option<NodeIndex<u32>> {
        self.id_to_idx.get(id).copied()
    }

    /// Reverse lookup — borrow the `String` stored at `idx`.
    ///
    /// Panics if `idx` is not in this graph (should not happen in valid use).
    pub(crate) fn node_id(&self, idx: NodeIndex<u32>) -> &str {
        self.graph
            .node_weight(idx)
            .expect("node_id: NodeIndex not in graph")
            .as_str()
    }

    /// Outgoing neighbors of `idx` as `(NodeIndex, EdgeWeight)` pairs, sorted
    /// ascending by the destination node's `String` id.
    ///
    /// Replaces the `.iter().map(...).collect::<Vec<(String, f64)>>()` patterns
    /// in the three solvers' BFS/DFS chains.
    pub(crate) fn sorted_neighbors(
        &self,
        idx: NodeIndex<u32>,
    ) -> Vec<(NodeIndex<u32>, EdgeWeight)> {
        let mut neighbors: Vec<(NodeIndex<u32>, EdgeWeight)> = self
            .graph
            .edges(idx)
            .map(|e| (e.target(), *e.weight()))
            .collect();
        neighbors.sort_by(|&(a, _), &(b, _)| self.node_id(a).cmp(self.node_id(b)));
        neighbors
    }

    /// Hand-rolled Kahn topological sort over the entire `DiGraph`, tie-breaking
    /// by `String` node id (ascending).
    ///
    /// Uses `BTreeSet<(String, NodeIndex<u32>)>` as the ready set (see module
    /// doc for the ordering rationale).
    ///
    /// Returns `Err(SolverError::BuildFailed(...))` if a cycle is detected
    /// (output shorter than `node_count()`). The existing sewer.rs implementation
    /// silently drops cycle nodes; this function makes cycles explicit.
    pub(crate) fn sorted_kahn_topo_sort(&self) -> Result<Vec<NodeIndex<u32>>, SolverError> {
        // Step 1: compute in-degree for each node.
        let mut in_degree: HashMap<NodeIndex<u32>, usize> =
            self.graph.node_indices().map(|n| (n, 0usize)).collect();

        for edge in self.graph.edge_references() {
            *in_degree.entry(edge.target()).or_insert(0) += 1;
        }

        // Step 2: seed the ready set with zero-in-degree nodes.
        // BTreeSet key = (node_id_string, NodeIndex.index()) gives
        // ascending-by-String tie-break in O(log n).
        let mut ready: BTreeSet<(String, u32)> = BTreeSet::new();
        for (&n, &deg) in &in_degree {
            if deg == 0 {
                ready.insert((self.node_id(n).to_owned(), n.index() as u32));
            }
        }

        // Step 3: Kahn's drain loop.
        let mut order: Vec<NodeIndex<u32>> = Vec::with_capacity(self.graph.node_count());

        while let Some((_, raw)) = ready.pop_first() {
            let idx = NodeIndex::new(raw as usize);
            order.push(idx);

            for (succ, _w) in self.sorted_neighbors(idx) {
                let deg = in_degree.entry(succ).or_insert(0);
                *deg = deg.saturating_sub(1);
                if *deg == 0 {
                    ready.insert((self.node_id(succ).to_owned(), succ.index() as u32));
                }
            }
        }

        // Step 4: cycle check.
        if order.len() < self.graph.node_count() {
            return Err(SolverError::BuildFailed(
                "cycle detected in SolverGraph — sorted_kahn_topo_sort cannot produce a total order"
                    .to_owned(),
            ));
        }

        Ok(order)
    }

    /// BFS from `root` over outgoing edges. Visit order at each level is
    /// determined by `sorted_neighbors`, giving deterministic orientation.
    ///
    /// Returns the visit order (including `root` as the first element).
    pub(crate) fn bfs_oriented(&self, root: NodeIndex<u32>) -> Vec<NodeIndex<u32>> {
        let mut visited: HashMap<NodeIndex<u32>, bool> = HashMap::new();
        let mut queue: VecDeque<NodeIndex<u32>> = VecDeque::new();
        let mut order: Vec<NodeIndex<u32>> = Vec::new();

        visited.insert(root, true);
        queue.push_back(root);

        while let Some(cur) = queue.pop_front() {
            order.push(cur);
            for (nb, _w) in self.sorted_neighbors(cur) {
                if !visited.contains_key(&nb) {
                    visited.insert(nb, true);
                    queue.push_back(nb);
                }
            }
        }

        order
    }

    /// Number of nodes in the graph.
    pub(crate) fn node_count(&self) -> usize {
        self.graph.node_count()
    }

    /// Number of edges in the graph.
    pub(crate) fn edge_count(&self) -> usize {
        self.graph.edge_count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── sorted_kahn_topo_sort — diamond graph determinism ─────────────────────

    /// Diamond graph: a→b, a→c, b→d, c→d.
    /// Nodes b and c both have in-degree 1 after popping a.
    /// Lex order: "b" < "c" → b must come before c in the output.
    /// Expected topo order: [a, b, c, d].
    #[test]
    fn sorted_kahn_topo_sort_deterministic_on_diamond_graph() {
        let mut sg = SolverGraph::new();
        sg.add_edge("a", "b", 1.0);
        sg.add_edge("a", "c", 1.0);
        sg.add_edge("b", "d", 1.0);
        sg.add_edge("c", "d", 1.0);

        let order = sg
            .sorted_kahn_topo_sort()
            .expect("acyclic graph must not fail");
        assert_eq!(order.len(), 4, "all 4 nodes must appear in the output");

        let ids: Vec<&str> = order.iter().map(|&idx| sg.node_id(idx)).collect();
        assert_eq!(
            ids,
            vec!["a", "b", "c", "d"],
            "lex tie-break: b before c; expected [a, b, c, d], got {:?}",
            ids
        );
    }

    // ── sorted_kahn_topo_sort — cycle detection ───────────────────────────────

    /// 3-node cycle: x→y→z→x. Kahn cannot drain all nodes → must return Err.
    #[test]
    fn sorted_kahn_topo_sort_detects_cycle() {
        let mut sg = SolverGraph::new();
        sg.add_edge("x", "y", 1.0);
        sg.add_edge("y", "z", 1.0);
        sg.add_edge("z", "x", 1.0);

        let result = sg.sorted_kahn_topo_sort();
        assert!(result.is_err(), "cycle must produce Err, got {:?}", result);
    }

    // ── sorted_neighbors — lex order ──────────────────────────────────────────

    /// Node "root" has outgoing edges to "g10", "g2", "g1" (inserted in that
    /// order). sorted_neighbors must return them in lex String order:
    /// "g1" < "g10" < "g2".
    #[test]
    fn sorted_neighbors_returns_lex_sorted_by_node_id() {
        let mut sg = SolverGraph::new();
        sg.add_edge("root", "g10", 3.0);
        sg.add_edge("root", "g2", 2.0);
        sg.add_edge("root", "g1", 1.0);

        let root_idx = sg.node_index("root").expect("root must exist");
        let neighbors = sg.sorted_neighbors(root_idx);
        assert_eq!(neighbors.len(), 3);

        let ids: Vec<&str> = neighbors.iter().map(|&(idx, _)| sg.node_id(idx)).collect();
        assert_eq!(
            ids,
            vec!["g1", "g10", "g2"],
            "lex order expected [g1, g10, g2], got {:?}",
            ids
        );
    }

    // ── add_edge — creates missing nodes ─────────────────────────────────────

    /// add_edge on previously-unknown nodes must insert both endpoints.
    #[test]
    fn add_edge_creates_missing_nodes() {
        let mut sg = SolverGraph::new();
        assert_eq!(sg.node_count(), 0);
        assert_eq!(sg.edge_count(), 0);

        sg.add_edge("alpha", "beta", 5.0);

        assert_eq!(sg.node_count(), 2, "two nodes must be auto-created");
        assert_eq!(sg.edge_count(), 1, "one directed edge must exist");
        assert!(sg.node_index("alpha").is_some());
        assert!(sg.node_index("beta").is_some());
    }

    // ── node_id / node_index — round-trip ────────────────────────────────────

    /// NodeIndex → &str → node_index must round-trip to the same NodeIndex.
    #[test]
    fn node_id_and_node_index_round_trip() {
        let mut sg = SolverGraph::new();
        let idx = sg.add_node("g42");

        let id_str = sg.node_id(idx);
        assert_eq!(id_str, "g42");

        let back = sg.node_index("g42").expect("must find g42");
        assert_eq!(back, idx, "round-trip must yield the same NodeIndex");
    }
}
