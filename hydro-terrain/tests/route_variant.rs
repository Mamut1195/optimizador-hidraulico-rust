//! Integration tests for route_variant (T-4.4).
//!
//! RED phase: written before production wiring exists.
//! Acceptance criteria:
//!   AC-1: variant==0 → insta snapshot of Steiner edge set (Rust-internal reproducibility).
//!   AC-2: variant>0 → all 5 terminals still reachable after perturb_weights (connectivity EXACT).
//!   AC-3: variant>0 → NO cross-language weight comparison (RNG differs; topology unpredictable).

use oracle::terrain::load_steiner_golden;

/// Build graph and run Steiner for a given variant.
fn build_and_run_steiner(variant: u32) -> hydro_terrain::SteinerTree {
    use hydro_terrain::steiner_tree;
    use hydro_terrain::{CostFunction, TerrainGraph, TerrainModel};

    let fixture = load_steiner_golden();
    let mut terrain = TerrainModel::from_xyz_list(&fixture.points).unwrap();
    terrain.build_grid(fixture.build_grid_res).unwrap();

    let mut graph = TerrainGraph::new(CostFunction::default());
    graph.from_grid(fixture.graph_resolution, &terrain);

    if variant > 0 {
        graph.perturb_weights(variant);
    }

    steiner_tree(&graph, &fixture.terminals).expect("steiner_tree must succeed")
}

/// T-4.4 AC-1a: variant==0 Steiner produces deterministic results across two calls.
///
/// We verify Rust-internal reproducibility: two separate graph builds with the
/// same parameters (variant=0) must produce identical edge sets.
#[test]
fn variant_zero_is_deterministic() {
    let tree1 = build_and_run_steiner(0);
    let tree2 = build_and_run_steiner(0);

    // Same total weight (exact, no RNG involved)
    let w1 = tree1.total_weight();
    let w2 = tree2.total_weight();
    assert!(
        (w1 - w2).abs() < 1e-9,
        "variant=0 must be deterministic: first call={w1}, second call={w2}"
    );

    // Same edge count
    assert_eq!(
        tree1.edge_count(),
        tree2.edge_count(),
        "variant=0 must produce same edge count"
    );

    // Same sorted edge set
    let edges1 = tree1.sorted_edges();
    let edges2 = tree2.sorted_edges();
    assert_eq!(
        edges1, edges2,
        "variant=0 must produce identical edge set across two calls"
    );
}

/// T-4.4 AC-1b: variant==0 insta snapshot of Steiner edge set (Rust-internal reproducibility).
///
/// Snapshot captures: sorted edge list + weight rounded to 3 decimal places.
/// The weight is rounded to avoid f64 Display precision differences across platforms
/// while still catching meaningful weight changes.
#[test]
fn variant_zero_insta_snapshot() {
    let tree = build_and_run_steiner(0);

    let edges = tree.sorted_edges();
    let edge_str: Vec<String> = edges.iter().map(|(a, b)| format!("{a}--{b}")).collect();
    let weight_rounded = format!("{:.3}", tree.total_weight());

    // Snapshot the edge list + rounded weight for Rust-side reproducibility
    insta::assert_debug_snapshot!("steiner_variant0", (edge_str, weight_rounded));
}

/// T-4.4 AC-2: variant>0 — all terminals still reachable (connectivity EXACT).
/// NO weight gate for variant>0 (RNG differs cross-language per phase4-decisions).
#[test]
fn variant_positive_connectivity_exact() {
    use oracle::terrain::load_steiner_golden;

    let fixture = load_steiner_golden();

    for variant in [1u32, 2, 5] {
        let tree = build_and_run_steiner(variant);

        for t in &fixture.terminals {
            assert!(
                tree.contains_node(t),
                "variant={variant}: terminal '{t}' missing from Steiner tree"
            );
        }

        for src in &fixture.terminals {
            for tgt in &fixture.terminals {
                if src != tgt {
                    assert!(
                        tree.is_connected(src, tgt),
                        "variant={variant}: terminal '{src}' not connected to '{tgt}'"
                    );
                }
            }
        }

        // Tree structure invariant still holds
        let n = tree.node_count();
        let e = tree.edge_count();
        assert_eq!(
            e,
            n - 1,
            "variant={variant}: tree invariant violated: e={e}, n={n}"
        );

        println!(
            "variant={variant}: connectivity OK, weight={:.6}",
            tree.total_weight()
        );
    }
}

/// T-4.4 AC-3: variant==0 weight is within statistical gate of oracle.
/// (Redundant with steiner.rs but confirms variant=0 path via route_variant wiring.)
#[test]
fn variant_zero_weight_statistical_gate() {
    let fixture = load_steiner_golden();
    let tree = build_and_run_steiner(0);

    let rust_weight = tree.total_weight();
    let oracle_weight = fixture.oracle_steiner_weight;
    let ratio = rust_weight / oracle_weight;

    println!("variant=0: Rust={rust_weight:.6}, Oracle={oracle_weight:.6}, ratio={ratio:.6}");

    assert!(
        ratio <= 1.10,
        "variant=0 weight gate FAILED: ratio={ratio:.6} > 1.10 \
         (Rust={rust_weight:.6}, Oracle={oracle_weight:.6})"
    );
}
