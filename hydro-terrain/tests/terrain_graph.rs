//! Integration tests for TerrainGraph — oracle parity (T-4.2).
//!
//! RED phase written first; GREEN when production code passes all assertions.

use approx::assert_abs_diff_eq;
use oracle::terrain::load_terrain_golden;

/// Helper: build a TerrainModel from the fixture.
fn build_terrain_model() -> hydro_terrain::TerrainModel {
    let fixture = load_terrain_golden();
    let mut model = hydro_terrain::TerrainModel::from_xyz_list(&fixture.points).unwrap();
    model.build_grid(fixture.build_grid_res).unwrap();
    model
}

/// Helper: build a TerrainGraph from the fixture.
fn build_graph() -> hydro_terrain::TerrainGraph {
    use hydro_terrain::{CostFunction, TerrainGraph};
    let fixture = load_terrain_golden();
    let terrain = build_terrain_model();
    let mut graph = TerrainGraph::new(CostFunction::default());
    graph.from_grid(fixture.graph_fixture.resolution, &terrain);
    graph
}

/// T-4.2 AC-4: node_count and edge_count must match oracle exactly.
#[test]
fn graph_node_edge_counts() {
    let fixture = load_terrain_golden();
    let graph = build_graph();

    assert_eq!(
        graph.node_count(),
        fixture.graph_fixture.node_count,
        "node_count mismatch: got {}, expected {}",
        graph.node_count(),
        fixture.graph_fixture.node_count
    );
    assert_eq!(
        graph.edge_count(),
        fixture.graph_fixture.edge_count,
        "edge_count mismatch: got {}, expected {}",
        graph.edge_count(),
        fixture.graph_fixture.edge_count
    );
}

/// T-4.2 AC-3: nearest_node must return the exact oracle node ID for 5 cases.
#[test]
fn nearest_node_exact() {
    let fixture = load_terrain_golden();
    let graph = build_graph();

    for case in &fixture.graph_fixture.nearest_node_cases {
        let got = graph.nearest_node(case.query_x, case.query_y).unwrap();
        assert_eq!(
            got, case.expected_node_id,
            "nearest_node mismatch at ({}, {}): got '{}', expected '{}'",
            case.query_x, case.query_y, got, case.expected_node_id
        );
    }
}

/// T-4.2 AC-2: Dijkstra path list must equal oracle AND total weight abs diff ≤ 1e-9.
#[test]
fn dijkstra_path_exact() {
    let fixture = load_terrain_golden();
    let graph = build_graph();

    for case in &fixture.graph_fixture.dijkstra_cases {
        let path = graph
            .shortest_path(&case.source_id, &case.target_id)
            .unwrap_or_else(|e| {
                panic!(
                    "shortest_path({} -> {}) failed: {}",
                    case.source_id, case.target_id, e
                )
            });

        assert_eq!(
            path,
            case.path,
            "Dijkstra path mismatch ({} -> {}): got {} nodes, expected {} nodes",
            case.source_id,
            case.target_id,
            path.len(),
            case.path.len()
        );

        let weight = graph
            .shortest_path_cost(&case.source_id, &case.target_id)
            .unwrap_or_else(|e| {
                panic!(
                    "shortest_path_cost({} -> {}) failed: {}",
                    case.source_id, case.target_id, e
                )
            });

        assert_abs_diff_eq!(weight, case.total_weight, epsilon = 1e-9);
    }
}

/// T-4.2: perturb_weights stub compiles and runs (correctness tested in T-4.3 / PR-5).
#[test]
fn perturb_weights_stub_compiles() {
    let mut graph = build_graph();
    // Verify it runs without panic for variant > 0
    graph.perturb_weights(1);
    assert!(
        graph.node_count() > 0,
        "graph must still have nodes after perturb"
    );
}

/// T-4.2: shortest_path returns Err for unknown node IDs.
#[test]
fn shortest_path_unknown_node_returns_error() {
    let graph = build_graph();
    let result = graph.shortest_path("nonexistent_src", "nonexistent_tgt");
    assert!(result.is_err(), "unknown node IDs must return Err");
}
