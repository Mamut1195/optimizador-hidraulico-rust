//! Integration tests for Steiner tree (Mehlhorn 2-approx) — oracle parity (T-4.3).
//!
//! RED phase: written before any production steiner.rs exists.
//! Acceptance criteria:
//!   AC-1: All 5 terminals reachable from each other via tree edges (EXACT connectivity).
//!   AC-2: Tree is acyclic and edge_count == node_count - 1 (tree invariant EXACT).
//!   AC-3: Total tree weight ≤ 1.10 × Python oracle weight (STATISTICAL).
//!   AC-4: InsufficientTerminals error when terminals.len() < 2.
//!   AC-5: proptest — connectivity invariant holds on random 3-terminal subsets of the fixture.

use oracle::terrain::load_steiner_golden;

/// Build the Steiner fixture graph (11×11 grid, 5 terminals, CostFunction::default()).
fn build_steiner_graph() -> (hydro_terrain::TerrainGraph, Vec<String>) {
    use hydro_terrain::{CostFunction, TerrainGraph, TerrainModel};

    let fixture = load_steiner_golden();

    // Build terrain from fixture points
    let mut terrain = TerrainModel::from_xyz_list(&fixture.points).unwrap();
    terrain.build_grid(fixture.build_grid_res).unwrap();

    // Build graph
    let mut graph = TerrainGraph::new(CostFunction::default());
    graph.from_grid(fixture.graph_resolution, &terrain);

    (graph, fixture.terminals.clone())
}

/// T-4.3 AC-1: all 5 terminals must be reachable from each other via tree edges.
#[test]
fn steiner_connectivity_exact() {
    use hydro_terrain::steiner_tree;

    let (graph, terminals) = build_steiner_graph();
    let tree = steiner_tree(&graph, &terminals).expect("steiner_tree must succeed");

    // All terminals must be in the tree
    for t in &terminals {
        assert!(
            tree.contains_node(t),
            "terminal '{t}' missing from Steiner tree"
        );
    }

    // All terminal pairs must be connected (BFS/DFS reachability)
    for src in &terminals {
        for tgt in &terminals {
            if src != tgt {
                assert!(
                    tree.is_connected(src, tgt),
                    "terminal '{src}' not connected to '{tgt}' in Steiner tree"
                );
            }
        }
    }
}

/// T-4.3 AC-2: tree must be acyclic and edge_count == node_count - 1.
#[test]
fn steiner_no_cycles() {
    use hydro_terrain::steiner_tree;

    let (graph, terminals) = build_steiner_graph();
    let tree = steiner_tree(&graph, &terminals).expect("steiner_tree must succeed");

    let n = tree.node_count();
    let e = tree.edge_count();
    assert!(n >= 2, "Steiner tree must have at least 2 nodes, got {n}");
    assert_eq!(
        e,
        n - 1,
        "Steiner tree must satisfy edge_count == node_count - 1 (got e={e}, n={n})"
    );
}

/// T-4.3 AC-3: total weight ≤ 1.10 × Python oracle weight (STATISTICAL).
///
/// The oracle weight comes from `nx.approximation.steiner_tree` on the same 11×11
/// fixture with the same CostFunction. Both use the Mehlhorn 2-approx so the ratio
/// is expected to be ≤ 1.0 + small floating-point noise.
///
/// Gate: 1.10 (10% slack for Mehlhorn expansion path differences).
/// NEVER relax this gate. If exceeded, report the ratio and stop.
#[test]
fn steiner_weight_within_10pct_of_oracle() {
    use hydro_terrain::steiner_tree;

    let fixture = oracle::terrain::load_steiner_golden();
    let (graph, terminals) = build_steiner_graph();
    let tree = steiner_tree(&graph, &terminals).expect("steiner_tree must succeed");

    let rust_weight = tree.total_weight();
    let oracle_weight = fixture.oracle_steiner_weight;

    let ratio = rust_weight / oracle_weight;
    println!("Steiner weight: Rust={rust_weight:.6}, Oracle={oracle_weight:.6}, ratio={ratio:.6}");

    assert!(
        ratio <= 1.10,
        "GATE FAILED: Steiner weight ratio {ratio:.6} > 1.10 \
         (Rust={rust_weight:.6}, Oracle={oracle_weight:.6}). \
         This indicates Mehlhorn port diverges more than expected. \
         Do NOT relax this gate — report the ratio and diagnose."
    );
}

/// T-4.3 AC-4: InsufficientTerminals error when fewer than 2 terminals given.
#[test]
fn steiner_insufficient_terminals_error() {
    use hydro_terrain::steiner_tree;
    use hydro_terrain::TerrainError;

    let (graph, terminals) = build_steiner_graph();

    // 0 terminals: use an explicit typed empty slice
    let empty: &[String] = &[];
    let result = steiner_tree(&graph, empty);
    assert!(
        matches!(result, Err(TerrainError::InsufficientTerminals { .. })),
        "0 terminals must return InsufficientTerminals, got: {result:?}"
    );

    // 1 terminal
    let result = steiner_tree(&graph, &terminals[..1]);
    assert!(
        matches!(result, Err(TerrainError::InsufficientTerminals { .. })),
        "1 terminal must return InsufficientTerminals, got: {result:?}"
    );
}

/// T-4.3 AC-5: proptest — connectivity invariant holds on any valid terminal subset.
#[cfg(test)]
mod proptest_steiner {
    use proptest::prelude::*;
    use rand::SeedableRng;

    proptest! {
        #![proptest_config(proptest::test_runner::Config {
            cases: 20,
            ..Default::default()
        })]

        #[test]
        fn steiner_always_connects_terminals(
            // Pick 2–5 terminal indices from the 5 available
            n_terminals in 2usize..=5usize,
            seed in 0u64..1000u64,
        ) {
            use hydro_terrain::steiner_tree;
            use hydro_terrain::{CostFunction, TerrainGraph, TerrainModel};

            let fixture = oracle::terrain::load_steiner_golden();
            let mut terrain = TerrainModel::from_xyz_list(&fixture.points).unwrap();
            terrain.build_grid(fixture.build_grid_res).unwrap();
            let mut graph = TerrainGraph::new(CostFunction::default());
            graph.from_grid(fixture.graph_resolution, &terrain);

            // Select `n_terminals` from the fixture's 5 terminals using the seed
            let all = &fixture.terminals;
            let mut rng = rand::rngs::SmallRng::seed_from_u64(seed);
            use rand::seq::SliceRandom;
            let mut chosen: Vec<String> = all.to_vec();
            chosen.shuffle(&mut rng);
            let chosen = &chosen[..n_terminals];

            let tree = steiner_tree(&graph, chosen).expect("steiner_tree must succeed");

            for t in chosen {
                prop_assert!(
                    tree.contains_node(t),
                    "terminal '{}' missing from Steiner tree", t
                );
            }
            for src in chosen {
                for tgt in chosen {
                    if src != tgt {
                        prop_assert!(
                            tree.is_connected(src, tgt),
                            "terminal '{}' not connected to '{}'", src, tgt
                        );
                    }
                }
            }
            // Tree invariant
            let n = tree.node_count();
            let e = tree.edge_count();
            prop_assert_eq!(e, n - 1, "tree invariant failed: e={}, n={}", e, n);
        }
    }
}
