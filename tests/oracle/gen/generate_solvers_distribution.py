"""Generate DistributionSolver oracle fixtures for hydro-solvers T-5.4 parity tests.

Emits:
  tests/oracle/fixtures/solvers_distribution_golden.json
  tests/oracle/fixtures/solvers_distribution_valves_saturated.json
  tests/oracle/fixtures/solvers_distribution_alternatives.json

Fixture parameters (locked — changing breaks golden vectors):

Fixture 1 (primary):
  - Terrain: 5x5 grid (20m spacing), z(x,y)=100+0.006*x+0.001*y
  - source=g0=(0,0), demand_points=[g4=(0,80), g20=(80,0), g24=(80,80)]
  - source_head=40.0, demand_per_node=0.002 m³/s, material=PVC
  - mesh_density=0.5, num_alternatives=1
  - valve_spacing=200.0 (int divisor=2), hydrant_spacing=200.0

Fixture 2 (valves-saturated, ordering trap):
  Same terrain, same source/demands, valve_spacing=100.0 (divisor=1) → every junction
  becomes a valve before the hydrant pass → hydrant_count == 0. hydrant_spacing=150.0.

Fixture 3 (alternatives):
  - Asymmetric terrain: z(x,y)=100+0.003*x+0.004*y
  - source=g12=(40,40), demand_points=[g0=(0,0), g4=(0,80), g24=(80,80)]
  - num_alternatives=3, mesh_density=0.5

Grid layout (5x5, i=0..4 x-axis, j=0..4 y-axis):
  counter = i*5 + j
  x = i * 20, y = j * 20

  g0=(0,0)    g1=(0,20)   g2=(0,40)   g3=(0,60)   g4=(0,80)
  g5=(20,0)   g6=(20,20)  g7=(20,40)  g8=(20,60)  g9=(20,80)
  g10=(40,0)  g11=(40,20) g12=(40,40) g13=(40,60) g14=(40,80)
  g15=(60,0)  g16=(60,20) g17=(60,40) g18=(60,60) g19=(60,80)
  g20=(80,0)  g21=(80,20) g22=(80,40) g23=(80,60) g24=(80,80)
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

import numpy as np
from hydro_engine.core.constraints import DesignConstraints
from hydro_engine.core.terrain import TerrainModel
from hydro_engine.core.types import PipeMaterial
from hydro_engine.solvers.distribution import DistributionSolver

# ── Terrain helpers ───────────────────────────────────────────────────────────


def make_terrain(slope_x: float, slope_y: float):
    """Build a 5x5 grid terrain with given slopes."""
    grid_res = 20.0
    xs = np.arange(0.0, 80.0 + grid_res, grid_res)
    ys = np.arange(0.0, 80.0 + grid_res, grid_res)

    points = []
    for x in xs:
        for y in ys:
            z = 100.0 + slope_x * x + slope_y * y
            points.append([x, y, z])

    points_np = np.array(points)
    terrain = TerrainModel(points_np[:, :2], points_np[:, 2])
    terrain.build_grid(resolution=grid_res)
    return terrain, points, grid_res


def serialize_solution(solver, sol, terrain):
    """Serialize a solution (network + score) to a dict."""
    network = sol.network
    score = sol.score

    # Re-evaluate to capture details
    eval_score = solver.evaluate(network)

    details_dict = {}
    if hasattr(eval_score, "details") and eval_score.details:
        details_dict = {
            k: float(v) if isinstance(v, (int, float)) else v
            for k, v in eval_score.details.items()
        }

    nodes_data = []
    for n in network.nodes:
        pressure = n.metadata.get("pressure_mca", None)
        accessory = n.metadata.get("accessory", None)
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
            "accessory": accessory,
        })

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
        })

    return {
        "network": {
            "node_count": network.node_count,
            "pipe_count": network.pipe_count,
            "total_length": network.total_length,
            "nodes": nodes_data,
            "pipes": pipes_data,
        },
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
    }


# ── Fixture 1: primary ────────────────────────────────────────────────────────


def make_primary_fixture():
    """Primary distribution fixture: single alternative, default valve/hydrant spacing."""
    terrain, points, grid_res = make_terrain(slope_x=0.006, slope_y=0.001)

    source = (0.0, 0.0)         # g0
    demand_points = [
        (0.0, 80.0),            # g4
        (80.0, 0.0),            # g20
        (80.0, 80.0),           # g24
    ]
    source_head = 40.0
    demand_per_node = 0.002

    constraints = DesignConstraints()
    solver = DistributionSolver(terrain, constraints)

    solutions = solver.solve(
        demand_points=demand_points,
        source=source,
        source_head=source_head,
        network_name="DistributionTest",
        demand_per_node=demand_per_node,
        material=PipeMaterial.PVC,
        grid_resolution=grid_res,
        num_alternatives=1,
        mesh_density=0.5,
        diameter_offset=0,
        valve_spacing=200.0,
        hydrant_spacing=200.0,
    )

    sol = solutions[0]
    sol_data = serialize_solution(solver, sol, terrain)

    print(f"Primary fixture:")
    print(f"  node_count    = {sol_data['network']['node_count']}")
    print(f"  pipe_count    = {sol_data['network']['pipe_count']}")
    print(f"  total_length  = {sol_data['network']['total_length']:.3f}")
    print(f"  total_cost    = {sol_data['score']['total_cost']:.6f}")
    print(f"  violations    = {sol_data['score']['norm_violations']}")
    print(f"  valve_count   = {sol_data['evaluate_only']['details'].get('valve_count', 0)}")
    print(f"  hydrant_count = {sol_data['evaluate_only']['details'].get('hydrant_count', 0)}")
    print(f"  pressure_std  = {sol_data['evaluate_only']['details'].get('pressure_std', 0)}")

    fixture = {
        "schema_version": 1,
        "fixture_name": "solvers_distribution_golden",
        "description": (
            "DistributionSolver oracle fixture: 5x5 grid (20m spacing), "
            "z=100.0+0.006*x+0.001*y, source=g0=(0,0), "
            "demands=[g4=(0,80), g20=(80,0), g24=(80,80)]. "
            "source_head=40.0, demand_per_node=0.002 m3/s, PVC, "
            "num_alternatives=1, mesh_density=0.5. DesignConstraints() defaults."
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
            "mesh_density": 0.5,
            "diameter_offset": 0,
            "valve_spacing": 200.0,
            "hydrant_spacing": 200.0,
        },
        "node_count": sol_data["network"]["node_count"],
        "pipe_count": sol_data["network"]["pipe_count"],
        "total_length": sol_data["network"]["total_length"],
        "nodes": sol_data["network"]["nodes"],
        "pipes": sol_data["network"]["pipes"],
        "score": sol_data["score"],
        "evaluate_only": sol_data["evaluate_only"],
        "cost_formula": {
            "w_length": 1.0,
            "w_excavation": 1.5,
            "w_violations": 150.0,
            "w_pressure_std": 5.0,
            "w_valve": 500.0,
            "w_hydrant": 800.0,
            "expected_cost": sol_data["score"]["total_cost"],
        },
    }
    return fixture


# ── Fixture 2: valves + hydrants ──────────────────────────────────────────────


def make_valves_saturated_fixture():
    """Valve-saturation ordering trap: divisor=1 turns EVERY junction into a valve.

    Parity trap: _place_valves_and_hydrants places valves FIRST (mutating nodes to
    VALVE in-place), then the hydrant pass only marks nodes still typed JUNCTION.
    With valve_spacing=100.0 (int divisor=1) every junction/service node becomes a
    valve, so the hydrant pass finds none → hydrant_count == 0 regardless of
    hydrant_spacing. This locks in the placement ordering as a regression.
    """
    terrain, points, grid_res = make_terrain(slope_x=0.006, slope_y=0.001)

    source = (0.0, 0.0)         # g0
    demand_points = [
        (0.0, 80.0),            # g4
        (80.0, 0.0),            # g20
        (80.0, 80.0),           # g24
    ]
    source_head = 40.0
    demand_per_node = 0.002

    # valve_spacing=100.0 → int(max(1, 100/100)) = 1 → every junction becomes a valve.
    # hydrant_spacing small enough that hydrants WOULD be placed if any junction survived.
    valve_spacing = 100.0
    hydrant_spacing = 150.0

    constraints = DesignConstraints()
    solver = DistributionSolver(terrain, constraints)

    solutions = solver.solve(
        demand_points=demand_points,
        source=source,
        source_head=source_head,
        network_name="DistributionValvesTest",
        demand_per_node=demand_per_node,
        material=PipeMaterial.PVC,
        grid_resolution=grid_res,
        num_alternatives=1,
        mesh_density=0.5,
        diameter_offset=0,
        valve_spacing=valve_spacing,
        hydrant_spacing=hydrant_spacing,
    )

    sol = solutions[0]
    sol_data = serialize_solution(solver, sol, terrain)

    vc = sol_data["evaluate_only"]["details"].get("valve_count", 0)
    hc = sol_data["evaluate_only"]["details"].get("hydrant_count", 0)
    print(f"\nValves-saturated fixture (placement ordering trap):")
    print(f"  valve_count   = {vc}")
    print(f"  hydrant_count = {hc}")
    print(f"  total_cost    = {sol_data['score']['total_cost']:.6f}")

    non_tank_nodes = [n for n in sol_data["network"]["nodes"] if n["node_type"] != "TANK"]
    if vc == 0:
        raise AssertionError(
            "valve_count=0 with valve_spacing=100.0 (divisor=1). "
            "Every junction/service node should have become a valve."
        )
    if hc != 0:
        raise AssertionError(
            f"hydrant_count={hc} but the ordering trap requires 0: with divisor=1 all "
            "junctions become valves before the hydrant pass runs, leaving none to mark."
        )
    print(f"  non-tank node count = {len(non_tank_nodes)} (all converted to valves)")

    fixture = {
        "schema_version": 1,
        "fixture_name": "solvers_distribution_valves_saturated",
        "description": (
            "DistributionSolver valve-saturation ordering trap: same terrain as primary, "
            "valve_spacing=100.0 (int divisor=1) converts EVERY junction/service node to "
            "a valve BEFORE the hydrant pass, so hydrant_count == 0 regardless of "
            "hydrant_spacing=150.0. Locks in the valves-before-hydrants placement order."
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
            "mesh_density": 0.5,
            "diameter_offset": 0,
            "valve_spacing": valve_spacing,
            "hydrant_spacing": hydrant_spacing,
        },
        "node_count": sol_data["network"]["node_count"],
        "pipe_count": sol_data["network"]["pipe_count"],
        "total_length": sol_data["network"]["total_length"],
        "nodes": sol_data["network"]["nodes"],
        "pipes": sol_data["network"]["pipes"],
        "score": sol_data["score"],
        "evaluate_only": sol_data["evaluate_only"],
        "expected_valve_count": int(vc),
        "expected_hydrant_count": int(hc),  # 0 — ordering trap: valves consume all junctions
        "cost_formula": {
            "w_length": 1.0,
            "w_excavation": 1.5,
            "w_violations": 150.0,
            "w_pressure_std": 5.0,
            "w_valve": 500.0,
            "w_hydrant": 800.0,
            "expected_cost": sol_data["score"]["total_cost"],
        },
    }
    return fixture


# ── Fixture 3: alternatives ───────────────────────────────────────────────────


def make_alternatives_fixture():
    """Alternatives fixture: 3 alternatives with asymmetric terrain for distinct costs."""
    # Asymmetric slopes for distinct costs across alternatives
    terrain, points, grid_res = make_terrain(slope_x=0.003, slope_y=0.004)

    # source at g12=(40,40), demands at g0, g4, g24
    source = (40.0, 40.0)       # g12
    demand_points = [
        (0.0, 0.0),             # g0
        (0.0, 80.0),            # g4
        (80.0, 80.0),           # g24
    ]
    source_head = 40.0
    demand_per_node = 0.002

    constraints = DesignConstraints()
    solver = DistributionSolver(terrain, constraints)

    solutions = solver.solve(
        demand_points=demand_points,
        source=source,
        source_head=source_head,
        network_name="DistributionAlt",
        demand_per_node=demand_per_node,
        material=PipeMaterial.PVC,
        grid_resolution=grid_res,
        num_alternatives=3,
        mesh_density=0.5,
        diameter_offset=0,
        valve_spacing=200.0,
        hydrant_spacing=200.0,
    )

    print(f"\nAlternatives fixture: {len(solutions)} solutions")
    costs = [s.score.total_cost for s in solutions]
    for i, (sol, cost) in enumerate(zip(solutions, costs)):
        print(
            f"  Alt {i+1}: total_cost={cost:.6f}, total_length={sol.score.total_length:.3f}m, "
            f"nodes={sol.network.node_count}, pipes={sol.network.pipe_count}, rank={sol.rank}"
        )

    # Assert pairwise strictly distinct costs
    print("\nPairwise distinctness check:")
    for i in range(len(costs)):
        for j in range(i + 1, len(costs)):
            diff = abs(costs[i] - costs[j])
            if diff < 1e-3:
                raise AssertionError(
                    f"Distribution alternatives costs[{i}]={costs[i]:.6f} and "
                    f"costs[{j}]={costs[j]:.6f} are NOT pairwise strictly distinct "
                    f"(|diff|={diff:.2e} < 1e-3). "
                    "Redesign the fixture terrain or parameters."
                )
            else:
                print(f"  costs[{i}]={costs[i]:.6f} vs costs[{j}]={costs[j]:.6f}: "
                      f"|diff|={diff:.6f} >= 1e-3  OK")

    alts_data = []
    for sol in solutions:
        net = sol.network
        eval_score = solver.evaluate(net)
        details_dict = {}
        if hasattr(eval_score, "details") and eval_score.details:
            details_dict = {
                k: float(v) if isinstance(v, (int, float)) else v
                for k, v in eval_score.details.items()
            }
        alts_data.append({
            "rank": sol.rank,
            "total_cost": eval_score.total_cost,
            "total_length": eval_score.total_length,
            "total_excavation": eval_score.total_excavation,
            "norm_violations": eval_score.norm_violations,
            "pump_count": eval_score.pump_count,
            "node_count": net.node_count,
            "pipe_count": net.pipe_count,
            "details": details_dict,
        })

    return {
        "schema_version": 1,
        "fixture_name": "solvers_distribution_alternatives",
        "description": (
            "DistributionSolver alternatives fixture: 3 alternatives. "
            "Asymmetric terrain z=100+0.003*x+0.004*y. "
            "source=g12=(40,40), demands=[g0, g4, g24]. "
            "Pairwise-distinct total_costs guarantee ordering assertions are non-vacuous."
        ),
        "solver_params": {
            "source": list(source),
            "demand_points": [list(dp) for dp in demand_points],
            "source_head": source_head,
            "demand_per_node": demand_per_node,
            "material": "PVC",
            "grid_resolution": grid_res,
            "num_alternatives": 3,
            "mesh_density": 0.5,
            "diameter_offset": 0,
            "valve_spacing": 200.0,
            "hydrant_spacing": 200.0,
        },
        "terrain": {
            "grid_res": grid_res,
            "x_min": 0.0,
            "x_max": 80.0,
            "y_min": 0.0,
            "y_max": 80.0,
            "slope_x": 0.003,
            "slope_y": 0.004,
            "base_z": 100.0,
            "points": points,
        },
        "num_alternatives_requested": 3,
        "num_solutions_returned": len(solutions),
        "solutions": alts_data,
        "distinct_costs": costs,
    }


# ── Main ──────────────────────────────────────────────────────────────────────


def main():
    print("Generating DistributionSolver oracle fixtures...")

    # Fixture 1: primary
    print("\n=== Fixture 1: primary (num_alternatives=1) ===")
    fixture1 = make_primary_fixture()
    out1 = FIXTURES_DIR / "solvers_distribution_golden.json"
    with open(out1, "w") as f:
        json.dump(fixture1, f, indent=2)
    print(f"\nWritten: {out1}")

    # Fixture 2: valve-saturation ordering trap
    print("\n=== Fixture 2: valves-saturated (ordering trap) ===")
    fixture2 = make_valves_saturated_fixture()
    out2 = FIXTURES_DIR / "solvers_distribution_valves_saturated.json"
    with open(out2, "w") as f:
        json.dump(fixture2, f, indent=2)
    print(f"Written: {out2}")

    # Fixture 3: alternatives
    print("\n=== Fixture 3: alternatives (num_alternatives=3) ===")
    fixture3 = make_alternatives_fixture()
    out3 = FIXTURES_DIR / "solvers_distribution_alternatives.json"
    with open(out3, "w") as f:
        json.dump(fixture3, f, indent=2)
    print(f"Written: {out3}")

    print("\nAll distribution fixtures generated successfully.")


if __name__ == "__main__":
    main()
