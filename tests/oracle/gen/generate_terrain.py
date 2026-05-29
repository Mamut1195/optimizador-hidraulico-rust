"""Generate terrain golden fixtures for hydro-terrain oracle parity tests.

Emits:
  tests/oracle/fixtures/terrain_golden.json

Fixture contains:
  - 10000 on-sample grid points (100x100 at 10m spacing)
  - 50 on-sample queries with expected_z and nearest_idx
  - 200 off-grid queries with oracle_z (from Python elevation_at)
  - measured off-grid p95 abs error (oracle vs oracle = 0; documents Rust tolerance gate)
  - graph_fixture: TerrainGraph built at resolution=20m, Dijkstra cases, nearest_node cases
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


def main() -> None:
    out_dir = Path(__file__).resolve().parents[1] / "fixtures"
    out_dir.mkdir(parents=True, exist_ok=True)
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

    graph_fixture = {
        "resolution": float(GRAPH_RES),
        "node_count": int(node_count),
        "edge_count": int(edge_count),
        "dijkstra_cases": dijkstra_cases,
        "nearest_node_cases": nearest_node_cases,
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


if __name__ == "__main__":
    main()
