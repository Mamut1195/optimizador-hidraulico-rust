"""Generate Steiner oracle fixture for hydro-terrain T-4.3 oracle parity tests.

Emits:
  tests/oracle/fixtures/steiner_golden.json

Fixture parameters (locked — changing breaks golden vectors):
  - Grid: 11×11 at resolution=20m (0..200 in both axes, step=20 → 11 values)
  - Surface: elevation = sin(x/200)*cos(y/200)*50 + 100 (same as terrain_golden)
  - build_grid_res: 20m (same as graph_resolution for exact on-grid queries)
  - graph_resolution: 20m
  - Terminals: g0, g10, g55, g99, g110
    (g0=(0,0), g10=(0,200), g55=(100,100), g99=(180,180), g110=(200,200) in i-outer j-inner order)
  - Oracle: nx.approximation.steiner_tree with weight="weight"

Terminal layout on 11×11 grid (i-outer, j-inner, counter = i*11 + j):
  g0   → i=0, j=0   → (x=0,   y=0)
  g10  → i=0, j=10  → (x=0,   y=200)
  g55  → i=5, j=0   → (x=100, y=0)
  g99  → i=9, j=0   → (x=180, y=0)
  g110 → i=10, j=0  → (x=200, y=0)

Wait, let's recalculate: counter increments i-outer, j-inner:
  i=0, j=0..10 → g0..g10
  i=1, j=0..10 → g11..g21
  ...
  i=5, j=0..10 → g55..g65
  g55 = i=5, j=0  → x=5*20=100, y=0
  g99 = counter 99 → i=9, j=0  → x=9*20=180, y=0
  g110 = counter 110 → i=10, j=0 → x=10*20=200, y=0

So all 5 terminals: g0=(0,0), g10=(0,200), g55=(100,0), g99=(180,0), g110=(200,0).
These span different quadrants so the Steiner tree must traverse non-trivial paths.
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

import numpy as np
import networkx as nx

# ---------------------------------------------------------------------------
# Ensure engine is on path for CostFunction import
# ---------------------------------------------------------------------------
ENGINE_ROOT = Path(__file__).resolve().parents[4] / "motor-optimizacion-hidraulico" / "src"
if str(ENGINE_ROOT) not in sys.path:
    sys.path.insert(0, str(ENGINE_ROOT))

from hydro_engine.core.terrain import TerrainModel
from hydro_engine.core.graph import TerrainGraph
from hydro_engine.core.cost import CostFunction, CostWeights

# ---------------------------------------------------------------------------
# Fixture parameters (locked)
# ---------------------------------------------------------------------------
GRAPH_RESOLUTION = 20.0        # grid step (m) for TerrainGraph
BUILD_GRID_RES = 20.0          # same for TerrainModel.build_grid
X_MIN, Y_MIN = 0.0, 0.0
X_MAX, Y_MAX = 200.0, 200.0    # 11 steps × 20m = 0..200 → 11 values via arange
SCHEMA_VERSION = 1

# Terminals: 5 node IDs on the 11×11 grid
# counter = i * 11 + j (i-outer, j-inner)
TERMINALS = ["g0", "g10", "g55", "g99", "g110"]


def elevation(x: float, y: float) -> float:
    """Same smooth DEM as terrain_golden."""
    return np.sin(x / 200.0) * np.cos(y / 200.0) * 50.0 + 100.0


def main() -> None:
    out_dir = Path(__file__).resolve().parents[1] / "fixtures"
    out_dir.mkdir(parents=True, exist_ok=True)
    out_path = out_dir / "steiner_golden.json"

    # -----------------------------------------------------------------------
    # Build the 11×11 synthetic DEM (0..200 at step=20 → 11 values)
    # -----------------------------------------------------------------------
    xs = np.arange(X_MIN, X_MAX + BUILD_GRID_RES * 0.5, BUILD_GRID_RES)
    ys = np.arange(Y_MIN, Y_MAX + BUILD_GRID_RES * 0.5, BUILD_GRID_RES)
    print(f"Grid: {len(xs)} x {len(ys)} = {len(xs)*len(ys)} nodes")
    assert len(xs) == 11, f"Expected 11 x-values, got {len(xs)}"
    assert len(ys) == 11, f"Expected 11 y-values, got {len(ys)}"

    xyz_list: list[list[float]] = []
    for x in xs:
        for y in ys:
            xyz_list.append([float(x), float(y), float(elevation(x, y))])

    assert len(xyz_list) == 121, f"Expected 121 DEM points, got {len(xyz_list)}"
    print(f"DEM points: {len(xyz_list)}")

    # -----------------------------------------------------------------------
    # Build TerrainModel + TerrainGraph
    # -----------------------------------------------------------------------
    terrain = TerrainModel.from_xyz_list(xyz_list)
    terrain.build_grid(resolution=BUILD_GRID_RES)

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
    graph.from_grid(resolution=GRAPH_RESOLUTION)

    print(f"TerrainGraph: {graph.node_count} nodes, {graph.edge_count} edges")
    assert graph.node_count == 121, f"Expected 121 nodes (11×11), got {graph.node_count}"

    # Verify terminal IDs exist in the graph
    nx_g = graph.nx_graph
    for t in TERMINALS:
        assert t in nx_g, f"Terminal '{t}' not found in graph"
    print(f"Terminals: {TERMINALS}")
    # Print terminal coordinates
    for t in TERMINALS:
        n = nx_g.nodes[t]
        print(f"  {t}: x={n['x']:.1f}, y={n['y']:.1f}, z={n['z']:.4f}")

    # -----------------------------------------------------------------------
    # Run Python Steiner tree (Mehlhorn 2-approx via networkx)
    # -----------------------------------------------------------------------
    steiner = nx.approximation.steiner_tree(nx_g, TERMINALS, weight="weight")

    oracle_weight = sum(d["weight"] for _, _, d in steiner.edges(data=True))
    oracle_node_count = steiner.number_of_nodes()
    oracle_edge_count = steiner.number_of_edges()

    print(f"Oracle Steiner tree: {oracle_node_count} nodes, {oracle_edge_count} edges")
    print(f"Oracle total weight: {oracle_weight:.6f}")

    # Verify all terminals are in the oracle tree
    for t in TERMINALS:
        assert t in steiner, f"Terminal '{t}' missing from oracle Steiner tree"
    print("All 5 terminals present in oracle Steiner tree OK")

    # Verify oracle tree is actually a tree (connected, acyclic)
    assert nx.is_connected(steiner), "Oracle Steiner tree is not connected!"
    assert oracle_edge_count == oracle_node_count - 1, (
        f"Oracle Steiner tree not a tree: {oracle_edge_count} edges, {oracle_node_count} nodes"
    )
    print("Oracle Steiner tree is a valid tree OK")

    # Collect edges as sorted pairs
    oracle_edges = []
    for u, v in steiner.edges():
        pair = sorted([str(u), str(v)])
        oracle_edges.append(pair)
    oracle_edges.sort()

    # -----------------------------------------------------------------------
    # Assemble and write fixture
    # -----------------------------------------------------------------------
    fixture = {
        "schema_version": SCHEMA_VERSION,
        "fixture_name": "steiner_11x11_5terminals",
        "points": xyz_list,
        "build_grid_res": float(BUILD_GRID_RES),
        "graph_resolution": float(GRAPH_RESOLUTION),
        "terminals": list(TERMINALS),
        "oracle_steiner_weight": float(oracle_weight),
        "oracle_edges": oracle_edges,
        "oracle_node_count": int(oracle_node_count),
        "oracle_edge_count": int(oracle_edge_count),
    }

    with open(out_path, "w", encoding="utf-8") as f:
        json.dump(fixture, f, indent=2)

    print(f"\nWritten: {out_path}")
    print(f"  fixture_name: {fixture['fixture_name']}")
    print(f"  points: {len(xyz_list)}")
    print(f"  terminals: {TERMINALS}")
    print(f"  oracle_steiner_weight: {oracle_weight:.6f}")
    print(f"  oracle_node_count: {oracle_node_count}")
    print(f"  oracle_edge_count: {oracle_edge_count}")


if __name__ == "__main__":
    main()
