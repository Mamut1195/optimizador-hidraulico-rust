"""Generate terrain golden fixtures for hydro-terrain oracle parity tests.

Emits:
  tests/oracle/fixtures/terrain_golden.json
  tests/oracle/fixtures/terrain_irregular_golden.json

terrain_golden.json contains:
  - 10000 on-sample grid points (100x100 at 10m spacing)
  - 50 on-sample queries with expected_z and nearest_idx
  - 200 off-grid queries with oracle_z (from Python elevation_at)
  - measured off-grid p95 abs error (oracle vs oracle = 0; documents Rust tolerance gate)
  - graph_fixture: TerrainGraph built at resolution=20m, Dijkstra cases, nearest_node cases

terrain_irregular_golden.json contains (W-01 fix — genuine ADR-5 validation):
  - ~1000 scatter points at random (x, y) positions (not on a regular grid)
  - z from the same sin/cos analytical surface (ground truth)
  - ~300 off-grid query points with oracle_z from Python scipy Delaunay-linear
  - This fixture exercises the real IDW (Rust) vs Delaunay (Python) divergence path
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

import numpy as np

# ---------------------------------------------------------------------------
# Ensure the oracle Python path is set so we can import hydro_engine
# ---------------------------------------------------------------------------
ENGINE_ROOT = Path(__file__).resolve().parents[4] / "motor-optimizacion-hidraulico" / "src"
if str(ENGINE_ROOT) not in sys.path:
    sys.path.insert(0, str(ENGINE_ROOT))

from hydro_engine.core.terrain import TerrainModel
from hydro_engine.core.graph import TerrainGraph
from hydro_engine.core.cost import CostFunction, CostWeights

# ---------------------------------------------------------------------------
# Fixture parameters (locked — changing breaks golden vectors)
# ---------------------------------------------------------------------------
GRID_RES = 10.0          # terrain sample spacing (m)
GRAPH_RES = 20.0         # TerrainGraph resolution (m)
X_MIN, Y_MIN = 0.0, 0.0
X_MAX, Y_MAX = 990.0, 990.0  # 100 steps * 10m = 990 (arange(0, 1000, 10) -> 0..990)
N_ON_SAMPLE = 50         # on-sample queries
N_OFF_GRID = 200         # off-grid interior queries
OFF_GRID_SEED = 1234     # fixed RNG seed for off-grid points
INTERIOR_MARGIN = 5.0    # off-grid points stay at least 5m from boundary
SCHEMA_VERSION = 1


def elevation(x: float, y: float) -> float:
    """Smooth synthetic DEM — no NaN risk anywhere on the grid."""
    return np.sin(x / 200.0) * np.cos(y / 200.0) * 50.0 + 100.0


def _generate_regular_grid_fixture(out_dir: Path) -> None:
    """Original regular-grid fixture generation (terrain_golden.json)."""
    out_path = out_dir / "terrain_golden.json"

    # -----------------------------------------------------------------------
    # Build the 100x100 synthetic DEM
    # -----------------------------------------------------------------------
    xs = np.arange(X_MIN, X_MAX + GRID_RES * 0.5, GRID_RES)   # 100 values: 0..990
    ys = np.arange(Y_MIN, Y_MAX + GRID_RES * 0.5, GRID_RES)   # 100 values: 0..990
    print(f"Grid shape: {len(xs)} x {len(ys)} = {len(xs)*len(ys)} points")

    xyz_list: list[tuple[float, float, float]] = []
    for x in xs:
        for y in ys:
            xyz_list.append((float(x), float(y), float(elevation(x, y))))

    print(f"Total points: {len(xyz_list)}")

    # Build TerrainModel and the regular grid (scipy griddata)
    terrain = TerrainModel.from_xyz_list(xyz_list)
    terrain.build_grid(resolution=GRID_RES)

    # -----------------------------------------------------------------------
    # On-sample queries: 50 points from the grid
    # -----------------------------------------------------------------------
    rng_on = np.random.RandomState(seed=42)
    on_sample_indices = rng_on.choice(len(xyz_list), size=N_ON_SAMPLE, replace=False)

    on_sample_queries = []
    for flat_idx in on_sample_indices:
        x, y, expected_z = xyz_list[flat_idx]
        # nearest_idx: the KDTree index of this point (should return flat_idx itself)
        _, nearest_idx = terrain._kdtree.query([x, y])
        got_z = terrain.elevation_at(x, y)
        on_sample_queries.append({
            "x": float(x),
            "y": float(y),
            "expected_z": float(expected_z),
            "nearest_idx": int(nearest_idx),
            "elevation_at_z": float(got_z),
        })

    # -----------------------------------------------------------------------
    # Off-grid queries: 200 random interior points
    # -----------------------------------------------------------------------
    rng_off = np.random.RandomState(seed=OFF_GRID_SEED)
    off_xs = rng_off.uniform(X_MIN + INTERIOR_MARGIN, X_MAX - INTERIOR_MARGIN, N_OFF_GRID)
    off_ys = rng_off.uniform(Y_MIN + INTERIOR_MARGIN, Y_MAX - INTERIOR_MARGIN, N_OFF_GRID)

    off_grid_queries = []
    for ox, oy in zip(off_xs, off_ys):
        oracle_z = terrain.elevation_at(float(ox), float(oy))
        off_grid_queries.append({
            "x": float(ox),
            "y": float(oy),
            "oracle_z": float(oracle_z),
        })

    # Oracle vs oracle p95 error is 0 by definition (oracle returns same value both times)
    # This field documents the fact; Rust vs oracle error is measured at test time.
    off_grid_p95_oracle_vs_oracle = 0.0

    # -----------------------------------------------------------------------
    # Build TerrainGraph at resolution=20m for graph fixture
    # -----------------------------------------------------------------------
    cost_fn = CostFunction(
        weights=CostWeights(
            w_length=1.0,
            w_excavation=2.0,
            w_slope_penalty=5.0,
            w_pump_penalty=50.0,
            w_crossing=10.0,
        ),
        min_slope=0.005,
        max_slope=0.05,
        min_cover=1.2,
    )
    graph = TerrainGraph(terrain, cost_fn)
    graph.from_grid(resolution=GRAPH_RES)

    node_count = graph.node_count
    edge_count = graph.edge_count
    print(f"TerrainGraph at res={GRAPH_RES}: {node_count} nodes, {edge_count} edges")

    # Dijkstra cases: 3 source→target pairs
    # For 100x100 terrain with 20m grid: xs = arange(0, 990+20, 20) = 0,20,...,980 = 50 values
    # ys = arange(0, 990+20, 20) = 0,20,...,980 = 50 values → 50x50 = 2500 nodes
    # Node IDs: counter increments i-outer, j-inner → g0=(0,0), g1=(0,20), ..., g49=(0,980)
    #           g50=(20,0), ...

    dijkstra_cases = []
    # Choose cases where the shortest path is unique (2nd path has strictly greater cost).
    # This ensures EXACT path-list parity between Python (networkx) and Rust (petgraph A*).
    import itertools

    def is_path_unique(g, src, tgt, atol=1e-9):
        """Return True if the shortest path cost is strictly less than the second-best."""
        try:
            paths = list(itertools.islice(
                nx.shortest_simple_paths(g.nx_graph, src, tgt, weight="weight"), 2
            ))
        except Exception:
            return False
        if len(paths) < 2:
            return True
        cost1 = nx.path_weight(g.nx_graph, paths[0], weight="weight")
        cost2 = nx.path_weight(g.nx_graph, paths[1], weight="weight")
        return (cost2 - cost1) > atol

    import networkx as nx
    candidate_pairs = [
        ("g0", "g10"),        # short, along Y axis
        ("g5", "g55"),        # mixed path
        ("g0", "g50"),        # across X axis
        ("g100", "g200"),     # mid-range
        ("g25", "g75"),       # short
    ]
    for (src, tgt) in candidate_pairs:
        if len(dijkstra_cases) >= 3:
            break
        if src not in graph.nx_graph or tgt not in graph.nx_graph:
            continue
        try:
            if not is_path_unique(graph, src, tgt):
                print(f"Skipping {src}->{tgt}: path not unique (tie-breaking sensitive)")
                continue
            path = graph.shortest_path(src, tgt)
            cost = graph.shortest_path_cost(src, tgt)
            dijkstra_cases.append({
                "source_id": src,
                "target_id": tgt,
                "path": list(path),
                "total_weight": float(cost),
            })
            print(f"  Dijkstra case {src}->{tgt}: {len(path)} nodes, cost={cost:.6f} (unique)")
        except Exception as e:
            print(f"Warning: Dijkstra {src}->{tgt} failed: {e}")

    if len(dijkstra_cases) < 1:
        # Fallback: use weight-only cases (g0 → g{node_count-1})
        try:
            src, tgt = "g0", f"g{node_count-1}"
            path = graph.shortest_path(src, tgt)
            cost = graph.shortest_path_cost(src, tgt)
            dijkstra_cases.append({
                "source_id": src,
                "target_id": tgt,
                "path": list(path),
                "total_weight": float(cost),
            })
        except Exception as e:
            print(f"Warning: fallback Dijkstra failed: {e}")

    # Nearest-node cases: 5 query points
    nearest_node_cases = []
    test_coords = [
        (5.0, 5.0),       # near g0
        (25.0, 25.0),     # between nodes
        (500.0, 500.0),   # center area
        (980.0, 980.0),   # far corner area
        (100.0, 300.0),   # arbitrary
    ]
    for qx, qy in test_coords:
        nid = graph.nearest_node(qx, qy)
        nearest_node_cases.append({
            "query_x": float(qx),
            "query_y": float(qy),
            "expected_node_id": str(nid),
        })

    # -----------------------------------------------------------------------
    # K-shortest-paths cases (Yen's / nx.shortest_simple_paths oracle parity)
    # Anti-tie-break discipline: only accept cases where the first k path costs
    # are pairwise STRICTLY distinct and the gap to any (k+1)-th path is also strict.
    # This ensures ordered k-path result is immune to tie-break divergence.
    # -----------------------------------------------------------------------
    K_PATHS_K = 3
    K_PATHS_ATOL = 1e-6

    def k_paths_costs_strictly_distinct(g_nx, src, tgt, k, atol=K_PATHS_ATOL):
        """Return (paths, costs) if anti-tie-break qualifies, else None."""
        try:
            # Fetch k+1 paths to check the gap to the discarded one
            paths_plus = list(itertools.islice(
                nx.shortest_simple_paths(g_nx, src, tgt, weight="weight"), k + 1
            ))
        except Exception:
            return None
        if len(paths_plus) < k:
            return None  # fewer than k paths exist
        paths = paths_plus[:k]
        costs = [nx.path_weight(g_nx, p, weight="weight") for p in paths]
        # (b) pairwise strictly distinct
        for i in range(len(costs) - 1):
            if costs[i + 1] - costs[i] <= atol:
                return None
        # (c) gap to (k+1)-th if it exists
        if len(paths_plus) > k:
            cost_kp1 = nx.path_weight(g_nx, paths_plus[k], weight="weight")
            if cost_kp1 - costs[-1] <= atol:
                return None
        return paths, costs

    k_paths_cases = []
    # Candidate pairs: diverse source-target pairs over the 51x51 grid
    kp_candidate_pairs = [
        ("g0", "g10"),
        ("g0", "g50"),
        ("g0", "g110"),
        ("g5", "g55"),
        ("g5", "g60"),
        ("g10", "g60"),
        ("g50", "g110"),
        ("g100", "g200"),
        ("g150", "g250"),
        ("g200", "g300"),
        ("g100", "g160"),
        ("g50", "g160"),
        ("g0", "g160"),
        ("g25", "g125"),
        ("g50", "g200"),
    ]
    for (src, tgt) in kp_candidate_pairs:
        if len(k_paths_cases) >= 3:
            break
        if src not in graph.nx_graph or tgt not in graph.nx_graph:
            continue
        result = k_paths_costs_strictly_distinct(graph.nx_graph, src, tgt, K_PATHS_K)
        if result is None:
            print(f"  k-paths: Skipping {src}->{tgt}: costs not strictly distinct or too few paths")
            continue
        paths, costs = result
        k_paths_cases.append({
            "source_id": src,
            "target_id": tgt,
            "k": K_PATHS_K,
            "paths": [list(p) for p in paths],
            "costs": [float(c) for c in costs],
        })
        gaps = [costs[i + 1] - costs[i] for i in range(len(costs) - 1)]
        print(
            f"  k-paths: Accepted {src}->{tgt}: k={K_PATHS_K}, "
            f"costs={[f'{c:.4f}' for c in costs]}, gaps={[f'{g:.4f}' for g in gaps]}"
        )

    print(f"  k_paths_cases: {len(k_paths_cases)}")

    graph_fixture = {
        "resolution": float(GRAPH_RES),
        "node_count": int(node_count),
        "edge_count": int(edge_count),
        "dijkstra_cases": dijkstra_cases,
        "nearest_node_cases": nearest_node_cases,
        "k_paths_cases": k_paths_cases,
    }

    # -----------------------------------------------------------------------
    # Assemble and write JSON
    # -----------------------------------------------------------------------
    fixture = {
        "schema_version": SCHEMA_VERSION,
        "fixture_name": "synthetic_10m_100x100",
        "grid_res": float(GRID_RES),
        "x_min": float(X_MIN),
        "x_max": float(X_MAX),
        "y_min": float(Y_MIN),
        "y_max": float(Y_MAX),
        "points": [[p[0], p[1], p[2]] for p in xyz_list],
        "on_sample_queries": on_sample_queries,
        "off_grid_queries": off_grid_queries,
        "off_grid_p95_abs_error": off_grid_p95_oracle_vs_oracle,
        "build_grid_res": float(GRID_RES),
        "graph_fixture": graph_fixture,
    }

    with open(out_path, "w", encoding="utf-8") as f:
        json.dump(fixture, f, indent=2)

    print(f"Written: {out_path}")
    print(f"  on_sample_queries: {len(on_sample_queries)}")
    print(f"  off_grid_queries: {len(off_grid_queries)}")
    print(f"  off_grid_p95 (oracle vs oracle): {off_grid_p95_oracle_vs_oracle}")
    print(f"  graph nodes: {node_count}, edges: {edge_count}")
    print(f"  dijkstra_cases: {len(dijkstra_cases)}")
    print(f"  nearest_node_cases: {len(nearest_node_cases)}")
    print(f"  k_paths_cases: {len(k_paths_cases)}")


def generate_irregular_scatter_fixture(out_dir: Path) -> None:
    """Generate terrain_irregular_golden.json — genuine ADR-5 / W-01 validation.

    Uses ~1000 randomly scattered survey points (NOT a regular grid) so that
    scipy.griddata Delaunay-linear and Rust IDW-bilinear genuinely diverge.
    Records oracle_z (Python side) for ~300 off-grid query points.

    Fixed parameters (locked — changing breaks golden vectors):
      - Domain: 0..990 x 0..990 (same as regular fixture)
      - Scatter seed: 7777
      - n_scatter: 1000
      - n_off_grid_queries: 300
      - Query seed: 8888
      - build_grid_res: 10.0 m
      - Surface: same sin(x/200)*cos(y/200)*50 + 100
    """
    # Locked parameters
    SCATTER_SEED = 7777
    QUERY_SEED = 8888
    N_SCATTER = 1000
    N_QUERIES = 300
    BUILD_GRID_RES = 10.0
    DOMAIN_MIN = 0.0
    DOMAIN_MAX = 990.0
    INTERIOR_MARGIN = 15.0  # query points stay away from boundary

    out_path = out_dir / "terrain_irregular_golden.json"

    # -----------------------------------------------------------------------
    # Generate ~1000 random scatter points (uniform x,y, NOT on a grid)
    # -----------------------------------------------------------------------
    rng_scatter = np.random.RandomState(seed=SCATTER_SEED)
    xs = rng_scatter.uniform(DOMAIN_MIN, DOMAIN_MAX, N_SCATTER)
    ys = rng_scatter.uniform(DOMAIN_MIN, DOMAIN_MAX, N_SCATTER)
    zs = np.array([elevation(float(x), float(y)) for x, y in zip(xs, ys)])

    xyz_list = [
        [float(x), float(y), float(z)] for x, y, z in zip(xs, ys, zs)
    ]

    print(f"\n=== Irregular scatter fixture ===")
    print(f"  Scatter points: {len(xyz_list)}")
    print(f"  x range: [{xs.min():.2f}, {xs.max():.2f}]")
    print(f"  y range: [{ys.min():.2f}, {ys.max():.2f}]")
    print(f"  z range: [{zs.min():.2f}, {zs.max():.2f}]")

    # -----------------------------------------------------------------------
    # Build Python TerrainModel from IRREGULAR scatter points
    # This triggers scipy.griddata Delaunay-linear for the scatter->grid step.
    # -----------------------------------------------------------------------
    terrain = TerrainModel.from_xyz_list(xyz_list)
    terrain.build_grid(resolution=BUILD_GRID_RES)

    # -----------------------------------------------------------------------
    # Generate ~300 off-grid query points (interior of domain)
    # -----------------------------------------------------------------------
    rng_query = np.random.RandomState(seed=QUERY_SEED)
    qxs = rng_query.uniform(DOMAIN_MIN + INTERIOR_MARGIN, DOMAIN_MAX - INTERIOR_MARGIN, N_QUERIES)
    qys = rng_query.uniform(DOMAIN_MIN + INTERIOR_MARGIN, DOMAIN_MAX - INTERIOR_MARGIN, N_QUERIES)

    off_grid_queries = []
    oracle_errors = []  # oracle vs analytical ground truth (to sanity-check oracle quality)
    for qx, qy in zip(qxs, qys):
        oracle_z = terrain.elevation_at(float(qx), float(qy))
        analytical_z = elevation(float(qx), float(qy))
        off_grid_queries.append({
            "x": float(qx),
            "y": float(qy),
            "oracle_z": float(oracle_z),
        })
        oracle_errors.append(abs(oracle_z - analytical_z))

    oracle_errors_arr = np.array(oracle_errors)
    oracle_errors_arr.sort()
    p95_idx = int(len(oracle_errors_arr) * 0.95)
    oracle_p95 = float(oracle_errors_arr[p95_idx])
    oracle_max = float(oracle_errors_arr[-1])
    print(f"  Python oracle vs analytical ground truth:")
    print(f"    p95 abs error: {oracle_p95:.6f} m (oracle fidelity check — NOT the Rust gate)")
    print(f"    max abs error: {oracle_max:.6f} m")

    # -----------------------------------------------------------------------
    # Assemble and write fixture
    # -----------------------------------------------------------------------
    fixture = {
        "schema_version": 1,
        "fixture_name": "irregular_scatter_1000pts",
        "build_grid_res": float(BUILD_GRID_RES),
        "x_min": float(DOMAIN_MIN),
        "x_max": float(DOMAIN_MAX),
        "y_min": float(DOMAIN_MIN),
        "y_max": float(DOMAIN_MAX),
        "scatter_seed": int(SCATTER_SEED),
        "n_scatter": len(xyz_list),
        "scatter_points": xyz_list,
        "off_grid_queries": off_grid_queries,
        "off_grid_p95_oracle_vs_oracle": 0.0,
    }

    with open(out_path, "w", encoding="utf-8") as f:
        json.dump(fixture, f, indent=2)

    print(f"  Written: {out_path}")
    print(f"  off_grid_queries: {len(off_grid_queries)}")


def main() -> None:
    out_dir = Path(__file__).resolve().parents[1] / "fixtures"
    out_dir.mkdir(parents=True, exist_ok=True)

    # Generate the regular-grid fixture (terrain_golden.json)
    _generate_regular_grid_fixture(out_dir)

    # Generate the irregular-scatter fixture (terrain_irregular_golden.json)
    # This is the W-01 fix — genuine ADR-5 / R-04 validation on irregular terrain.
    generate_irregular_scatter_fixture(out_dir)


if __name__ == "__main__":
    main()
