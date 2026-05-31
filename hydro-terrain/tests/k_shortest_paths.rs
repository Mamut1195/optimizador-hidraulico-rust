//! Oracle-parity integration test for k_shortest_paths (Yen's algorithm).
//!
//! Asserts that `TerrainGraph::k_shortest_paths` returns EXACTLY the ordered
//! Vec<Vec<String>> from the Python oracle (`nx.shortest_simple_paths`) for
//! each case in `terrain_golden.json::graph_fixture::k_paths_cases`.
//!
//! Anti-tie-break guarantee: the generator only emits cases where all k path
//! costs are pairwise STRICTLY distinct, so the ordered result is immune to
//! internal tie-break divergence.

use oracle::terrain::load_terrain_golden;

/// Helper: build a TerrainGraph from the golden fixture terrain points.
fn build_graph() -> hydro_terrain::TerrainGraph {
    use hydro_terrain::{CostFunction, TerrainGraph};
    let fixture = load_terrain_golden();
    let mut model = hydro_terrain::TerrainModel::from_xyz_list(&fixture.points).unwrap();
    model.build_grid(fixture.build_grid_res).unwrap();
    let terrain = model;
    let mut graph = TerrainGraph::new(CostFunction::default());
    graph.from_grid(fixture.graph_fixture.resolution, &terrain);
    graph
}

/// Compute total weight of a path by summing edge weights in the graph.
fn path_weight(graph: &hydro_terrain::TerrainGraph, path: &[String]) -> f64 {
    path.windows(2)
        .map(|w| {
            graph
                .edge_weight(&w[0], &w[1])
                .unwrap_or_else(|| panic!("edge not found: {} -> {}", w[0], w[1]))
        })
        .sum()
}

/// T-k-paths-1: k_shortest_paths must return EXACTLY the oracle ordered paths,
/// and each path's summed weight must match oracle cost within 1e-9 abs.
#[test]
fn k_shortest_paths_oracle_parity() {
    let fixture = load_terrain_golden();
    let graph = build_graph();

    for case in &fixture.graph_fixture.k_paths_cases {
        let got = graph
            .k_shortest_paths(&case.source_id, &case.target_id, case.k)
            .unwrap_or_else(|e| {
                panic!(
                    "k_shortest_paths({} -> {}, k={}) failed: {}",
                    case.source_id, case.target_id, case.k, e
                )
            });

        assert_eq!(
            got.len(),
            case.paths.len(),
            "k_shortest_paths({} -> {}, k={}): returned {} paths, oracle has {}",
            case.source_id,
            case.target_id,
            case.k,
            got.len(),
            case.paths.len()
        );

        for (i, (got_path, oracle_path)) in got.iter().zip(case.paths.iter()).enumerate() {
            assert_eq!(
                got_path,
                oracle_path,
                "k_shortest_paths({} -> {}, k={}): path[{}] mismatch\n  got:    {:?}\n  oracle: {:?}",
                case.source_id,
                case.target_id,
                case.k,
                i,
                got_path,
                oracle_path
            );

            // Also verify summed weight matches oracle cost
            let weight = path_weight(&graph, got_path);
            assert!(
                (weight - case.costs[i]).abs() < 1e-9,
                "{} -> {} path[{}] weight mismatch: got={:.12}, oracle={:.12}",
                case.source_id,
                case.target_id,
                i,
                weight,
                case.costs[i]
            );
        }
    }
}

/// T-k-paths-2: k_shortest_paths with k=1 must equal shortest_path.
#[test]
fn k_shortest_paths_k1_equals_dijkstra() {
    let fixture = load_terrain_golden();
    let graph = build_graph();

    for case in &fixture.graph_fixture.dijkstra_cases {
        let kpaths = graph
            .k_shortest_paths(&case.source_id, &case.target_id, 1)
            .unwrap_or_else(|e| panic!("k_shortest_paths k=1 failed: {}", e));

        let dijkstra = graph
            .shortest_path(&case.source_id, &case.target_id)
            .unwrap_or_else(|e| panic!("shortest_path failed: {}", e));

        assert_eq!(
            kpaths.len(),
            1,
            "k=1 must return exactly 1 path, got {}",
            kpaths.len()
        );
        assert_eq!(
            kpaths[0], dijkstra,
            "k_shortest_paths k=1 must equal shortest_path for {} -> {}",
            case.source_id, case.target_id
        );
    }
}

/// T-k-paths-3: k_shortest_paths with unknown source returns NodeNotFound.
#[test]
fn k_shortest_paths_unknown_node_returns_error() {
    let graph = build_graph();
    let result = graph.k_shortest_paths("nonexistent", "g0", 3);
    assert!(result.is_err(), "unknown source must return Err");
}
