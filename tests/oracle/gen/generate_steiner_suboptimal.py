"""Generate a suboptimal Steiner oracle fixture for strict parity testing (T-4.3 ext).

Emits:
  tests/oracle/fixtures/steiner_suboptimal_golden.json

Purpose:
  The existing steiner_golden.json fixture is VACUOUS for parity testing: both
  Python and Rust use the Mehlhorn 2-approximation, so the ratio is always ~1.0
  regardless of whether the Rust port is faithful. This generator produces a NEW
  fixture where:

    oracle_mehlhorn_weight > exact_optimal_weight * (1 + 1e-6)

  proving that the fixture exercises a genuinely suboptimal regime. The Rust test
  then asserts that Rust reproduces the EXACT oracle (Mehlhorn) edge-set AND weight
  (ratio == 1.0 within floating-point tolerance), demonstrating faithful parity of
  the Mehlhorn implementation rather than mere correctness of any Steiner heuristic.

Geometry (3x3 grid at step=20m):
  - Grid: 3x3 at resolution=20m (x,y in {0,20,40}), 9 nodes.
  - Node IDs: g{i*3+j} with i-outer (x-axis), j-inner (y-axis).
  - Elevation: terminals at z=100m, non-terminals at z=97m.
  - Terminals: g0=(0,0), g2=(0,40), g6=(40,0).
  - Non-terminal Steiner hub: g4=(20,20).

Suboptimality proof:
  - Mehlhorn uses g0 as the branching point: g0-g1-g2 + g0-g3-g6 = 608.
  - Optimal (Dreyfus-Wagner exact) routes through g4: g0-g1-g4 + g4-g5-g2 + g4-g7-g6 = 463.5.
  - approximation_gap = 608 / 463.5 = 1.3118 (> 1.0, proven suboptimal).

Why Mehlhorn fails here:
  - Pairwise shortest paths: d(g0,g2)=304, d(g0,g6)=304 (both via 2-hop direct routes).
  - Terminal MST picks g0-g2 and g0-g6, expanding to edges g0-g1, g1-g2, g0-g3, g3-g6.
  - The hub g4 is NOT on any terminal-to-terminal shortest path, so Mehlhorn misses it.
  - The optimal tree through g4 costs 3*154.5=463.5, saving 144.5 over Mehlhorn.
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

import numpy as np
import networkx as nx

# ---------------------------------------------------------------------------
# Ensure engine is on path for TerrainModel/TerrainGraph/CostFunction imports.
# ENGINE_ROOT resolves to:
#   <this_file>/../../../../../../../motor-optimizacion-hidraulico/src
# i.e. from tests/oracle/gen/ go up 4 levels to the workspace root,
# then into the sibling motor-optimizacion-hidraulico/src.
# ---------------------------------------------------------------------------
ENGINE_ROOT = Path(__file__).resolve().parents[4] / "motor-optimizacion-hidraulico" / "src"
if str(ENGINE_ROOT) not in sys.path:
    sys.path.insert(0, str(ENGINE_ROOT))

from hydro_engine.core.terrain import TerrainModel
from hydro_engine.core.graph import TerrainGraph
from hydro_engine.core.cost import CostFunction, CostWeights

# ---------------------------------------------------------------------------
# Fixture parameters (locked — do NOT change without updating the Rust test)
# ---------------------------------------------------------------------------
BUILD_GRID_RES = 20.0     # meters
GRAPH_RESOLUTION = 20.0   # meters
SCHEMA_VERSION = 1

# 3x3 grid: x in {0, 20, 40}, y in {0, 20, 40}
# Node ID: g{i*3+j}, i-outer (x), j-inner (y)
# Terminals at z=100m (high), non-terminals at z=97m (low valley)
TERMINALS = ["g0", "g2", "g6"]

# Hand-computed DEM: [x, y, z]
POINTS: list[list[float]] = [
    [0.0,   0.0,  100.0],   # g0 = terminal T1
    [0.0,  20.0,   97.0],   # g1 = intermediate
    [0.0,  40.0,  100.0],   # g2 = terminal T2
    [20.0,  0.0,   97.0],   # g3 = intermediate
    [20.0, 20.0,   97.0],   # g4 = Steiner hub (optimal branching point)
    [20.0, 40.0,   97.0],   # g5 = intermediate
    [40.0,  0.0,  100.0],   # g6 = terminal T3
    [40.0, 20.0,   97.0],   # g7 = intermediate
    [40.0, 40.0,   97.0],   # g8 = intermediate
]


# ---------------------------------------------------------------------------
# Exact Steiner solver: Dreyfus-Wagner DP  O(3^k * n + 2^k * n^2 + n^3)
# Used ONLY to prove suboptimality at generation time; not emitted to fixture.
# ---------------------------------------------------------------------------

def dreyfus_wagner_exact(G: nx.Graph, terminals: list[str]) -> float:
    """Compute exact Steiner tree weight via Dreyfus-Wagner DP.

    Terminates in polynomial time for k <= 5 terminals and n <= 200 nodes.
    Returns the minimum total edge weight of a Steiner tree spanning `terminals`.
    """
    node_list = list(G.nodes())
    node_idx = {n: i for i, n in enumerate(node_list)}
    n = len(node_list)
    k = len(terminals)
    t_idx = [node_idx[t] for t in terminals]

    # --- All-pairs shortest paths (Floyd-Warshall) ---
    D = np.full((n, n), np.inf)
    for i in range(n):
        D[i, i] = 0.0
    for u, v, data in G.edges(data=True):
        i, j = node_idx[u], node_idx[v]
        w = data["weight"]
        D[i, j] = w
        D[j, i] = w
    for kk in range(n):
        for i in range(n):
            if D[i, kk] == np.inf:
                continue
            for j in range(n):
                if D[i, kk] + D[kk, j] < D[i, j]:
                    D[i, j] = D[i, kk] + D[kk, j]

    # --- Dreyfus-Wagner DP ---
    # dp[S][v] = min cost of a Steiner tree spanning terminal subset S, rooted at v
    dp = np.full((1 << k, n), np.inf)

    # Base: single-terminal subsets
    for i, t in enumerate(t_idx):
        for v in range(n):
            dp[1 << i, v] = D[t, v]

    # Fill table
    for S in range(1, 1 << k):
        if bin(S).count("1") < 2:
            continue
        # Partition into two non-empty proper subsets
        sub = (S - 1) & S
        while sub > 0:
            comp = S ^ sub
            if sub < comp:  # avoid double-counting
                for v in range(n):
                    val = dp[sub, v] + dp[comp, v]
                    if val < dp[S, v]:
                        dp[S, v] = val
            sub = (sub - 1) & S
        # Relax via shortest paths through the full graph
        for u in range(n):
            if dp[S, u] == np.inf:
                continue
            for v in range(n):
                if dp[S, u] + D[u, v] < dp[S, v]:
                    dp[S, v] = dp[S, u] + D[u, v]

    full_mask = (1 << k) - 1
    return float(np.min(dp[full_mask]))


def main() -> None:
    out_dir = Path(__file__).resolve().parents[1] / "fixtures"
    out_dir.mkdir(parents=True, exist_ok=True)
    out_path = out_dir / "steiner_suboptimal_golden.json"

    # -----------------------------------------------------------------------
    # Build TerrainModel + TerrainGraph from the hand-crafted 3x3 DEM
    # -----------------------------------------------------------------------
    assert len(POINTS) == 9, f"Expected 9 DEM points, got {len(POINTS)}"
    xs_unique = sorted(set(p[0] for p in POINTS))
    ys_unique = sorted(set(p[1] for p in POINTS))
    assert xs_unique == [0.0, 20.0, 40.0], f"Unexpected x values: {xs_unique}"
    assert ys_unique == [0.0, 20.0, 40.0], f"Unexpected y values: {ys_unique}"
    print(f"Grid: 3x3 = {len(POINTS)} nodes at step={BUILD_GRID_RES}m")

    terrain = TerrainModel.from_xyz_list(POINTS)
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

    nx_g = graph.nx_graph
    print(f"TerrainGraph: {nx_g.number_of_nodes()} nodes, {nx_g.number_of_edges()} edges")
    assert nx_g.number_of_nodes() == 9, f"Expected 9 nodes, got {nx_g.number_of_nodes()}"

    # Verify terminal IDs exist
    for t in TERMINALS:
        assert t in nx_g, f"Terminal '{t}' not found in graph"
    print(f"Terminals: {TERMINALS}")
    for t in TERMINALS:
        nd = nx_g.nodes[t]
        print(f"  {t}: x={nd['x']:.1f}, y={nd['y']:.1f}, z={nd['z']:.4f}")

    # -----------------------------------------------------------------------
    # Oracle Mehlhorn approximation (networkx)
    # -----------------------------------------------------------------------
    steiner = nx.approximation.steiner_tree(nx_g, TERMINALS, weight="weight")

    oracle_weight = sum(d["weight"] for _, _, d in steiner.edges(data=True))
    oracle_node_count = steiner.number_of_nodes()
    oracle_edge_count = steiner.number_of_edges()

    print(f"Oracle Mehlhorn: {oracle_node_count} nodes, {oracle_edge_count} edges")
    print(f"Oracle Mehlhorn weight: {oracle_weight:.6f}")

    # Verify oracle tree integrity
    for t in TERMINALS:
        assert t in steiner, f"Terminal '{t}' missing from oracle Steiner tree"
    assert nx.is_connected(steiner), "Oracle Steiner tree is not connected!"
    assert oracle_edge_count == oracle_node_count - 1, (
        f"Oracle Steiner tree not a tree: {oracle_edge_count} edges, {oracle_node_count} nodes"
    )
    print("Oracle tree is valid (connected, acyclic, all terminals present) OK")

    # Collect edges as sorted pairs
    oracle_edges = []
    for u, v in steiner.edges():
        pair = sorted([str(u), str(v)])
        oracle_edges.append(pair)
    oracle_edges.sort()

    # -----------------------------------------------------------------------
    # Exact Steiner (Dreyfus-Wagner) — suboptimality proof
    # -----------------------------------------------------------------------
    print("Running Dreyfus-Wagner exact solver...")
    exact_optimal_weight = dreyfus_wagner_exact(nx_g, TERMINALS)
    print(f"Exact optimal weight: {exact_optimal_weight:.6f}")

    approximation_gap = oracle_weight / exact_optimal_weight
    print(f"Approximation gap (Mehlhorn/Exact): {approximation_gap:.6f}")

    # *** MANDATORY ASSERTION: fixture must be genuinely suboptimal ***
    assert oracle_weight > exact_optimal_weight * (1 + 1e-6), (
        f"FIXTURE IS VACUOUS: Mehlhorn weight ({oracle_weight:.6f}) is NOT greater than "
        f"exact optimal ({exact_optimal_weight:.6f}). "
        f"This geometry does not demonstrate Mehlhorn suboptimality. "
        f"The approximation_gap must be > 1.0 + 1e-6. "
        f"Revise the geometry before using this fixture for parity testing."
    )
    print(f"SUBOPTIMALITY PROVEN: Mehlhorn ({oracle_weight:.6f}) > Exact ({exact_optimal_weight:.6f})")
    print(f"  Mehlhorn is {approximation_gap:.4f}x the optimal ({(approximation_gap-1)*100:.2f}% worse)")

    # -----------------------------------------------------------------------
    # Assemble and write fixture
    # -----------------------------------------------------------------------
    fixture = {
        "schema_version": SCHEMA_VERSION,
        "fixture_name": "steiner_3x3_suboptimal",
        "points": POINTS,
        "build_grid_res": float(BUILD_GRID_RES),
        "graph_resolution": float(GRAPH_RESOLUTION),
        "terminals": list(TERMINALS),
        # Oracle (Mehlhorn) result — the parity target for the Rust test
        "oracle_steiner_weight": float(oracle_weight),
        "oracle_edges": oracle_edges,
        "oracle_node_count": int(oracle_node_count),
        "oracle_edge_count": int(oracle_edge_count),
        # Exact result — for documentation and suboptimality proof
        "exact_optimal_weight": float(exact_optimal_weight),
        "approximation_gap": float(approximation_gap),
    }

    with open(out_path, "w", encoding="utf-8") as f:
        json.dump(fixture, f, indent=2)

    print(f"\nWritten: {out_path}")
    print(f"  fixture_name: {fixture['fixture_name']}")
    print(f"  points: {len(POINTS)}")
    print(f"  terminals: {TERMINALS}")
    print(f"  oracle_steiner_weight: {oracle_weight:.6f}")
    print(f"  exact_optimal_weight: {exact_optimal_weight:.6f}")
    print(f"  approximation_gap: {approximation_gap:.6f}")
    print(f"  oracle_node_count: {oracle_node_count}")
    print(f"  oracle_edge_count: {oracle_edge_count}")
    print()
    print("SUBOPTIMALITY PROOF:")
    print(f"  oracle_mehlhorn_weight ({oracle_weight:.6f})")
    print(f"  > exact_optimal_weight * (1+1e-6) ({exact_optimal_weight * (1+1e-6):.6f})")
    print(f"  ratio = {approximation_gap:.6f} (must be > 1.0)")


if __name__ == "__main__":
    main()
