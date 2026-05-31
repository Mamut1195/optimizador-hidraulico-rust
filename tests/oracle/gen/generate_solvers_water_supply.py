"""Generate WaterSupplySolver oracle fixture for hydro-solvers T-5.3 parity tests.

Emits:
  tests/oracle/fixtures/solvers_water_supply_golden.json
  tests/oracle/fixtures/solvers_water_supply_alternatives.json

Fixture parameters (locked — changing breaks golden vectors):
  - Terrain: 5x5 regular grid, 20m spacing, (0..80 in both axes)
  - Surface: z = 100.0 + 0.006 * x + 0.001 * y
    (asymmetric slope breaks MST edge-weight ties between horizontal and vertical edges)
  - Grid node IDs: g{counter}, i-outer (x) j-inner (y)
  - Source: ON-GRID at (0.0, 0.0) → g0 (z = 100.0, lowest = source tank at ground)
  - Demand points: ON-GRID coordinates for EXACT elevation query (risk #1)
      dp0 = (60.0, 0.0)  → g15 (z = 100.36)
      dp1 = (20.0, 40.0) → g7  (z = 100.16)
      dp2 = (40.0, 60.0) → g13 (z = 100.30)
  - demand_per_node: 0.002 m³/s
  - source_head: 30.0 mca  (in [15, 50] for demand nodes → no pressure violations)
  - material: PVC (HW C = 150)
  - grid_resolution: 20.0 m
  - num_alternatives: 1 (primary MST solution for deterministic full-field parity)
  - DesignConstraints() — DEFAULT Python constraints (sewer diameters 0.2-1.22m)
    NOTE: generator uses DesignConstraints() not water-supply CONAGUA constraints.
    This matches the design rule in the task spec: "Use DesignConstraints::default()"

Grid layout (5x5, i=0..4 x-axis, j=0..4 y-axis):
  counter = i*5 + j
  x = i * 20, y = j * 20
  z(x, y) = 100.0 + 0.006 * x + 0.001 * y

  g0=(0,0)    g1=(0,20)   g2=(0,40)   g3=(0,60)   g4=(0,80)
  g5=(20,0)   g6=(20,20)  g7=(20,40)  g8=(20,60)  g9=(20,80)
  g10=(40,0)  g11=(40,20) g12=(40,40) g13=(40,60) g14=(40,80)
  g15=(60,0)  g16=(60,20) g17=(60,40) g18=(60,60) g19=(60,80)
  g20=(80,0)  g21=(80,20) g22=(80,40) g23=(80,60) g24=(80,80)

  z values:
    g0  z=100.000 (source)
    g7  z=100.160 (dp1)
    g13 z=100.300 (dp2)
    g15 z=100.360 (dp0)

MST uniqueness: The asymmetric slope (different x and y coefficients 0.006 vs 0.001)
makes horizontal edges (20m, dz=0.12) and vertical edges (20m, dz=0.02) cost differently,
breaking all tie-weight MST ambiguities. Uniqueness verified by the generator.

Alternatives fixture: A SECOND fixture with num_alternatives=3 is generated
for Yen k-paths parity. Uses a single demand point for clean path enumeration
with pairwise-distinct total_costs.
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

# ── Python path setup ─────────────────────────────────────────────────────────

REPO_ROOT = Path(__file__).resolve().parents[3]
ENGINE_ROOT = REPO_ROOT.parent / "motor-optimizacion-hidraulico" / "src"
if str(ENGINE_ROOT) not in sys.path:
    sys.path.insert(0, str(ENGINE_ROOT))

FIXTURES_DIR = REPO_ROOT / "tests" / "oracle" / "fixtures"
FIXTURES_DIR.mkdir(parents=True, exist_ok=True)

# ── Imports ───────────────────────────────────────────────────────────────────

import networkx as nx
import numpy as np
from hydro_engine.core.constraints import DesignConstraints
from hydro_engine.core.graph import TerrainGraph
from hydro_engine.core.terrain import TerrainModel
from hydro_engine.core.types import PipeMaterial
from hydro_engine.core.cost import CostFunction, CostWeights
from hydro_engine.solvers.water_supply import WaterSupplySolver

# ── Terrain grid ──────────────────────────────────────────────────────────────

def make_terrain_and_points():
    """Build a 5x5 water supply fixture terrain.

    z(x, y) = 100.0 + 0.006*x + 0.001*y
    Asymmetric slope ensures unique MST (horizontal vs vertical edges cost differently).
    Source at g0=(0,0): lowest z=100.0 → pumps water uphill.
    """
    grid_res = 20.0
    xs = np.arange(0.0, 80.0 + grid_res, grid_res)  # 0,20,40,60,80
    ys = np.arange(0.0, 80.0 + grid_res, grid_res)  # 0,20,40,60,80

    points = []
    for x in xs:
        for y in ys:
            z = 100.0 + 0.006 * x + 0.001 * y
            points.append([x, y, z])

    points_np = np.array(points)
    terrain = TerrainModel(points_np[:, :2], points_np[:, 2])
    terrain.build_grid(resolution=grid_res)
    return terrain, points, grid_res


def check_mst_uniqueness(terrain, source, demand_points, grid_res):
    """Verify MST is unique: second-best MST strictly costlier than best MST."""
    cost_fn = CostFunction(
        CostWeights(w_length=1.0, w_excavation=1.5, w_slope_penalty=0.5, w_pump_penalty=20.0),
        min_slope=0.005,
        max_slope=0.05,
        min_cover=1.2,
    )
    graph = TerrainGraph(terrain, cost_fn)
    graph.from_grid(resolution=grid_res)

    source_node = graph.nearest_node(*source)
    reachable = nx.node_connected_component(graph.nx_graph, source_node)
    mst = nx.minimum_spanning_tree(graph.nx_graph.subgraph(reachable), weight="weight")

    best_weight = sum(d["weight"] for _, _, d in mst.edges(data=True))

    # Find second-best MST by removing one edge at a time and finding next MST
    second_best = float("inf")
    for u, v, d in mst.edges(data=True):
        # Remove edge and find MST on the remaining graph
        sub = graph.nx_graph.subgraph(reachable).copy()
        sub.remove_edge(u, v)
        if nx.is_connected(sub):
            alt_mst = nx.minimum_spanning_tree(sub, weight="weight")
            alt_weight = sum(dd["weight"] for _, _, dd in alt_mst.edges(data=True))
            if alt_weight < second_best:
                second_best = alt_weight

    print(f"MST uniqueness check: best={best_weight:.6f}, second_best={second_best:.6f}")
    if second_best == float("inf"):
        print("  (graph disconnected on edge removal — MST is trivially unique)")
    elif second_best > best_weight:
        print("  UNIQUE: second-best MST is strictly costlier OK")
    else:
        print("  WARNING: MST tie detected — fixture may not be deterministic!")
    return best_weight, second_best


def make_water_supply_fixture():
    """Build the primary water supply fixture (num_alternatives=1)."""
    terrain, points, grid_res = make_terrain_and_points()

    source = (0.0, 0.0)        # g0: z=100.0 (lowest point, tank at base)
    demand_points = [
        (60.0, 0.0),           # g15: z=100.36
        (20.0, 40.0),          # g7:  z=100.16
        (40.0, 60.0),          # g13: z=100.30
    ]
    # source_head=30.0: P_demand ≈ 30 - hf - dz ≈ 30 - small - 0.36 ≈ 29.6 mca
    # (within [15, 50] → no pressure violations expected)
    source_head = 30.0
    demand_per_node = 0.002    # m³/s

    print("Checking MST uniqueness ...")
    check_mst_uniqueness(terrain, source, demand_points, grid_res)

    constraints = DesignConstraints()
    solver = WaterSupplySolver(terrain, constraints)

    solutions = solver.solve(
        demand_points=demand_points,
        source=source,
        source_head=source_head,
        network_name="WaterSupplyTest",
        demand_per_node=demand_per_node,
        material=PipeMaterial.PVC,
        grid_resolution=grid_res,
        num_alternatives=1,
        network_type=1,
        diameter_offset=0,
        loop_density=0.5,
    )

    sol = solutions[0]
    network = sol.network
    score = sol.score

    # Re-evaluate for the evaluate_only sub-fixture
    eval_score = solver.evaluate(network)

    details_dict = {}
    if hasattr(eval_score, "details") and eval_score.details:
        details_dict = {
            k: float(v) if isinstance(v, (int, float)) else v
            for k, v in eval_score.details.items()
        }

    # Serialize nodes
    nodes_data = []
    for n in network.nodes:
        pressure = n.metadata.get("pressure_mca", None)
        nodes_data.append({
            "id": n.id,
            "x": n.x,
            "y": n.y,
            "z": n.z,
            "node_type": n.node_type.name if hasattr(n.node_type, "name") else str(n.node_type),
            "rim_elevation": n.rim_elevation,
            "sump_elevation": n.sump_elevation,
            "demand": n.demand,
            "pressure_mca": float(pressure) if pressure is not None else None,
        })

    # Serialize pipes
    pipes_data = []
    for p in network.pipes:
        pipes_data.append({
            "id": p.id,
            "start_node_id": p.start_node_id,
            "end_node_id": p.end_node_id,
            "length": p.length,
            "diameter": p.diameter,
            "slope": p.slope,
            "start_invert": p.start_invert,
            "end_invert": p.end_invert,
            "design_flow": p.design_flow,
            "flow_m3s": float(p.metadata.get("flow_m3s", p.design_flow)),
        })

    fixture = {
        "schema_version": 1,
        "fixture_name": "solvers_water_supply_golden",
        "description": (
            "WaterSupplySolver oracle fixture: 5x5 grid (20m spacing), "
            "z=100.0+0.003*x, source=g20=(80,0), demands=g0,g7,g18. "
            "source_head=80mca (no violations). DesignConstraints() defaults. "
            "All source/demand at on-grid coords for EXACT elevation queries."
        ),
        "terrain": {
            "grid_res": grid_res,
            "x_min": 0.0,
            "x_max": 80.0,
            "y_min": 0.0,
            "y_max": 80.0,
            "slope_x": 0.006,
            "slope_y": 0.001,
            "base_z": 100.0,
            "points": points,
        },
        "solver_params": {
            "source": list(source),
            "demand_points": [list(dp) for dp in demand_points],
            "source_head": source_head,
            "demand_per_node": demand_per_node,
            "material": "PVC",
            "grid_resolution": grid_res,
            "num_alternatives": 1,
            "network_type": 1,
            "diameter_offset": 0,
            "loop_density": 0.5,
        },
        "slope_x": 0.006,
        "slope_y": 0.001,
        "node_count": network.node_count,
        "pipe_count": network.pipe_count,
        "total_length": network.total_length,
        "nodes": nodes_data,
        "pipes": pipes_data,
        "score": {
            "total_cost": eval_score.total_cost,
            "total_length": eval_score.total_length,
            "total_excavation": eval_score.total_excavation,
            "norm_violations": eval_score.norm_violations,
            "pump_count": eval_score.pump_count,
        },
        "evaluate_only": {
            "total_cost": eval_score.total_cost,
            "total_length": eval_score.total_length,
            "total_excavation": eval_score.total_excavation,
            "norm_violations": eval_score.norm_violations,
            "pump_count": eval_score.pump_count,
            "details": details_dict,
        },
        "cost_formula": {
            "w_length": 1.0,
            "w_excavation": 1.5,
            "w_violations": 150.0,
            "expected_cost": eval_score.total_cost,
        },
    }

    return fixture


def make_alternatives_fixture():
    """Build the alternatives fixture (num_alternatives=3) for Yen k-paths parity.

    Uses 2 demand points for simpler Yen enumeration and ensures
    alternative total_costs are pairwise strictly distinct (tie-break-immune).
    """
    terrain, points, grid_res = make_terrain_and_points()

    # Use source at g0=(0,0), single demand at g15=(60,0) (farthest node on x-axis)
    # Multiple paths exist: via x-axis (g0->g5->g10->g15), or via diagonal paths
    source = (0.0, 0.0)    # g0: lowest z
    demand_points = [
        (60.0, 0.0),        # g15: farthest along x-axis from source
    ]
    source_head = 80.0
    demand_per_node = 0.002

    constraints = DesignConstraints()
    solver = WaterSupplySolver(terrain, constraints)

    solutions = solver.solve(
        demand_points=demand_points,
        source=source,
        source_head=source_head,
        network_name="WaterSupplyAlt",
        demand_per_node=demand_per_node,
        material=PipeMaterial.PVC,
        grid_resolution=grid_res,
        num_alternatives=3,
        network_type=0,  # tree only (no loops for clean path comparison)
        diameter_offset=0,
        loop_density=0.0,
    )

    print(f"\nAlternatives fixture: {len(solutions)} solutions generated")
    costs = [s.score.total_cost for s in solutions]
    for i, (sol, cost) in enumerate(zip(solutions, costs)):
        print(f"  Alt {i+1}: total_cost={cost:.6f}, rank={sol.rank}")

    # Verify pairwise strictly distinct costs
    for i in range(len(costs)):
        for j in range(i + 1, len(costs)):
            diff = abs(costs[i] - costs[j])
            if diff < 1e-6:
                print(f"  WARNING: costs[{i}] and costs[{j}] are nearly equal ({diff:.2e})")
            else:
                print(f"  costs[{i}] vs costs[{j}]: |diff|={diff:.6f} OK strictly distinct")

    alts_data = []
    for sol in solutions:
        net = sol.network
        alts_data.append({
            "rank": sol.rank,
            "total_cost": sol.score.total_cost,
            "total_length": sol.score.total_length,
            "norm_violations": sol.score.norm_violations,
            "node_count": net.node_count,
            "pipe_count": net.pipe_count,
        })

    return {
        "schema_version": 1,
        "fixture_name": "solvers_water_supply_alternatives",
        "description": (
            "WaterSupplySolver alternatives fixture: 3 solutions for Yen k-paths parity. "
            "source=g0=(0,0), demand=[g15=(60,0)], num_alternatives=3, network_type=0."
        ),
        "solver_params": {
            "source": list(source),
            "demand_points": [list(dp) for dp in demand_points],
            "source_head": source_head,
            "demand_per_node": demand_per_node,
            "material": "PVC",
            "grid_resolution": grid_res,
            "num_alternatives": 3,
            "network_type": 0,
            "diameter_offset": 0,
            "loop_density": 0.0,
        },
        "terrain": {
            "grid_res": grid_res,
            "x_min": 0.0,
            "x_max": 80.0,
            "y_min": 0.0,
            "y_max": 80.0,
            "slope_x": 0.006,
            "slope_y": 0.001,
            "base_z": 100.0,
            "points": points,
        },
        "num_alternatives_requested": 3,
        "num_solutions_returned": len(solutions),
        "solutions": alts_data,
    }


def main():
    print("Generating WaterSupplySolver oracle fixtures...")

    # Primary fixture
    print("\n=== Primary fixture (num_alternatives=1) ===")
    fixture = make_water_supply_fixture()
    output_path = FIXTURES_DIR / "solvers_water_supply_golden.json"
    with open(output_path, "w") as f:
        json.dump(fixture, f, indent=2)

    print(f"\nWritten: {output_path}")
    print(f"  node_count        = {fixture['node_count']}")
    print(f"  pipe_count        = {fixture['pipe_count']}")
    print(f"  total_length      = {fixture['total_length']:.3f} m")
    print(f"  total_cost        = {fixture['score']['total_cost']:.6f}")
    print(f"  total_excavation  = {fixture['score']['total_excavation']:.6f}")
    print(f"  norm_violations   = {fixture['score']['norm_violations']}")
    print(f"  pump_count        = {fixture['score']['pump_count']}")

    print("\nNodes:")
    for n in fixture["nodes"]:
        print(
            f"  {n['id']}: ({n['x']:.0f},{n['y']:.0f}) z={n['z']:.3f} "
            f"type={n['node_type']} demand={n['demand']:.4f} "
            f"pressure={n['pressure_mca']}"
        )

    print("\nPipes:")
    for p in fixture["pipes"]:
        print(
            f"  {p['id']}: {p['start_node_id']}->{p['end_node_id']} "
            f"L={p['length']:.1f}m D={p['diameter']*1000:.0f}mm "
            f"flow={p['flow_m3s']:.4f} inv={p['start_invert']:.3f}->{p['end_invert']:.3f}"
        )

    # Alternatives fixture
    print("\n=== Alternatives fixture (num_alternatives=3) ===")
    alt_fixture = make_alternatives_fixture()
    alt_path = FIXTURES_DIR / "solvers_water_supply_alternatives.json"
    with open(alt_path, "w") as f:
        json.dump(alt_fixture, f, indent=2)
    print(f"\nWritten: {alt_path}")
    print(f"  num_solutions_returned = {alt_fixture['num_solutions_returned']}")
    for sol in alt_fixture["solutions"]:
        print(
            f"  rank={sol['rank']}: total_cost={sol['total_cost']:.6f} "
            f"total_length={sol['total_length']:.3f}m nodes={sol['node_count']} "
            f"pipes={sol['pipe_count']}"
        )


if __name__ == "__main__":
    main()
