//! Integration tests for TerrainModel — oracle parity (T-4.1).
//!
//! RED phase: these tests are written first, before any production code exists.
//! They define the acceptance criteria for T-4.1.

use approx::assert_abs_diff_eq;
use oracle::terrain::load_terrain_golden;

/// T-4.1 AC-2: on-sample elevation queries must match oracle exactly (abs ≤ 1e-9).
#[test]
fn on_sample_elevation_exact() {
    use hydro_terrain::TerrainModel;

    let fixture = load_terrain_golden();
    let mut model = TerrainModel::from_xyz_list(&fixture.points).unwrap();
    model.build_grid(fixture.build_grid_res).unwrap();

    for q in &fixture.on_sample_queries {
        let got = model.elevation_at(q.x, q.y);
        assert_abs_diff_eq!(got, q.expected_z, epsilon = 1e-9);
    }
}

/// T-4.1 AC-3: nearest_idx on distinct grid points must match oracle index exactly.
#[test]
fn nearest_node_index_exact() {
    use hydro_terrain::TerrainModel;

    let fixture = load_terrain_golden();
    let model = TerrainModel::from_xyz_list(&fixture.points).unwrap();

    for q in &fixture.on_sample_queries {
        let idx = model.nearest_idx(q.x, q.y);
        assert_eq!(
            idx, q.nearest_idx,
            "nearest_idx mismatch at ({}, {}): got {}, expected {}",
            q.x, q.y, idx, q.nearest_idx
        );
    }
}

/// T-4.1 AC-4: off-grid elevation statistical gate — p95 abs error < 0.1 m.
#[test]
fn off_grid_elevation_statistical_p95() {
    use hydro_terrain::TerrainModel;

    let fixture = load_terrain_golden();
    let mut model = TerrainModel::from_xyz_list(&fixture.points).unwrap();
    model.build_grid(fixture.build_grid_res).unwrap();

    let mut abs_errors: Vec<f64> = fixture
        .off_grid_queries
        .iter()
        .map(|q| (model.elevation_at(q.x, q.y) - q.oracle_z).abs())
        .collect();

    abs_errors.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let p95_idx = ((abs_errors.len() as f64) * 0.95) as usize;
    let p95 = abs_errors[p95_idx.min(abs_errors.len() - 1)];

    assert!(
        p95 < 0.1,
        "p95 off-grid abs error {p95:.6} m >= 0.1 m threshold (200 queries, p95_idx={p95_idx})"
    );

    // Also report the measured value for the apply-progress record.
    println!("Measured off-grid p95 abs error: {p95:.6} m");
}

/// T-4.1 AC-1: compilation + basic structural invariants.
#[test]
fn terrain_model_structural() {
    use hydro_terrain::TerrainModel;

    let fixture = load_terrain_golden();
    let model = TerrainModel::from_xyz_list(&fixture.points).unwrap();

    assert_eq!(model.point_count(), fixture.points.len());
    assert!(model.min_elevation() < model.max_elevation());

    let (xmin, xmax, ymin, ymax) = model.bounds();
    assert_abs_diff_eq!(xmin, fixture.x_min, epsilon = 1e-9);
    assert_abs_diff_eq!(xmax, fixture.x_max, epsilon = 1e-9);
    assert_abs_diff_eq!(ymin, fixture.y_min, epsilon = 1e-9);
    assert_abs_diff_eq!(ymax, fixture.y_max, epsilon = 1e-9);
    // suppress unused variable warnings from destructuring
    let _ = xmin;
    let _ = xmax;
    let _ = ymin;
    let _ = ymax;
}

/// T-4.1: elevation_at without grid (NN fallback) — within bounds test.
#[test]
fn elevation_at_nn_fallback_no_grid() {
    use hydro_terrain::TerrainModel;

    let fixture = load_terrain_golden();
    let model = TerrainModel::from_xyz_list(&fixture.points).unwrap();
    // Without grid, elevation_at falls back to NN — on a sample point this should be exact.
    for q in fixture.on_sample_queries.iter().take(5) {
        let got = model.elevation_at(q.x, q.y);
        assert_abs_diff_eq!(got, q.expected_z, epsilon = 1e-9);
    }
}

/// T-4.1: from_xyz_list error path — empty input.
#[test]
fn from_xyz_list_empty_returns_error() {
    use hydro_terrain::TerrainModel;

    let result = TerrainModel::from_xyz_list(&[]);
    assert!(result.is_err(), "empty points must return Err");
}
