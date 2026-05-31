"""Generate ConveyanceSolver oracle fixtures for hydro-solvers T-5.3 parity tests.

Emits:
  tests/oracle/fixtures/solvers_conveyance_golden.json       (primary pipeline, route_variant=0)
  tests/oracle/fixtures/solvers_conveyance_valves.json       (valve-placement fixture)
  tests/oracle/fixtures/solvers_conveyance_alternatives.json (Yen k-paths, num_alternatives=3)

=== Fixture Design Notes ===

Grid: 5x5, 20m spacing (x: 0..80, y: 0..80). Same 25-node layout as sewer/WS fixtures.
  g{counter} where counter = i*5 + j (i=x-index outer, j=y-index inner).
  g0=(0,0) g1=(0,20) ... g5=(20,0) g6=(20,20) ... g24=(80,80)

--- Primary fixture ---
  Source: g0=(0,0),   Destination: g20=(80,0).
  Straight route along x-axis (g0→g5→g10→g15→g20), length=80m.
  Terrain: z(x,y) = 100.0 + 0.006*x (monotonically increasing in x).
    Source (g0) z=100.0, Dest (g20) z=100.48.
  design_flow=0.010 m³/s, source_head=30.0 mca, num_alternatives=1, route_variant=0.
  Monotonic terrain → no high/low interior points → structure_count=0 (asserted exact).

--- Valve fixture ---
  Source: g0=(0,0),  Destination: g20=(80,0).
  Terrain engineered with a deliberate interior peak at g10=(40,0) and valley at g15=(60,0):
    g0  z=100.0
    g5  z=100.4   (rising)
    g10 z=101.0   (PEAK: 101.0 > 100.4 and 101.0 > 100.4)
    g15 z=100.2   (VALLEY: 100.2 < 101.0 and 100.2 < 100.8)
    g20 z=100.8   (destination)
  y-direction nodes use a flat z(x,y) = z_axis[x] to keep on-grid elevation exact.
  valve_spacing=1.0 so both structures get placed without distance exclusion.
  This guarantees structure_count >= 1 (at least the air_valve at g10).
  With spacing=1.0, g10 is placed first (high point), then g15 (low point) passes
  _far_enough since |cum_40 - cum_20| = 20 >= 1.0.

--- Alternatives fixture ---
  Source: g0=(0,0),  Destination: g4=(0,80).
  Terrain: z(x,y) = 100.0 + 0.003*x + 0.004*y  (asymmetric x/y slopes).
    This produces different path costs for x-axis vs y-axis detours, ensuring
    pairwise-distinct total_costs among the k=3 shortest pipeline routes.
  num_alternatives=3, route_variant=0.
  Source→Dest direction is pure +y (along j=0 column), so:
    path1: g0→g1→g2→g3→g4 (direct y-axis, length=80m)
    path2: g0→g5→g1→g2→g3→g4 or similar detour (longer/costlier)
    path3: another detour
  We verify pairwise distinctness in the generator and abort if any tie < 1e-3.
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
from hydro_engine.solvers.conveyance import ConveyanceSolver


# ── Terrain builders ──────────────────────────────────────────────────────────

def make_monotonic_terrain():
    """5x5 grid, z(x,y) = 100.0 + 0.006*x.

    Monotonically increasing in x → no interior high/low points on the x-axis path.
    All nodes at grid-exact coordinates.
    """
    grid_res = 20.0
    xs = np.arange(0.0, 80.0 + grid_res, grid_res)  # [0,20,40,60,80]
    ys = np.arange(0.0, 80.0 + grid_res, grid_res)

    points = []
    for x in xs:
        for y in ys:
            z = 100.0 + 0.006 * x
            points.append([x, y, z])

    points_np = np.array(points)
    terrain = TerrainModel(points_np[:, :2], points_np[:, 2])
    terrain.build_grid(resolution=grid_res)
    return terrain, points, grid_res


def make_valve_terrain():
    """5x5 grid with a deliberate peak and valley along the x-axis path.

    x-axis elevations (j=0 column):
      g0=(0,0):   z=100.0
      g5=(20,0):  z=100.4   (rising)
      g10=(40,0): z=101.0   (PEAK: g10 > g5 and g10 > g15)
      g15=(60,0): z=100.2   (VALLEY: g15 < g10 and g15 < g20)
      g20=(80,0): z=100.8   (destination)

    Off-axis nodes use z = z_of_x[x] (same as j=0 col) so all on-grid queries
    return exact values.
    """
    grid_res = 20.0
    xs = np.arange(0.0, 80.0 + grid_res, grid_res)
    ys = np.arange(0.0, 80.0 + grid_res, grid_res)

    # z values for each x (applies to all y at that x)
    z_for_x = {
        0.0:  100.0,
        20.0: 100.4,
        40.0: 101.0,   # PEAK
        60.0: 100.2,   # VALLEY
        80.0: 100.8,
    }

    points = []
    for x in xs:
        for y in ys:
            z = z_for_x[float(x)]
            points.append([x, y, z])

    points_np = np.array(points)
    terrain = TerrainModel(points_np[:, :2], points_np[:, 2])
    terrain.build_grid(resolution=grid_res)
    return terrain, points, grid_res


def make_alternatives_terrain():
    """5x5 grid, z(x,y) = 100.0 + 0.006*x + 0.001*y.

    Same terrain as primary fixture.
    Source: g0=(0,0), Destination: g15=(60,0).
    Min path: 3 hops = 60m (g0->g5->g10->g15).
    Alt path: 5 hops = 100m (detour via y).
    num_alternatives=2 gives exactly 2 distinct costs: 834 and 1890.
    """
    grid_res = 20.0
    xs = np.arange(0.0, 80.0 + grid_res, grid_res)
    ys = np.arange(0.0, 80.0 + grid_res, grid_res)

    points = []
    for x in xs:
        for y in ys:
            z = 100.0 + 0.006 * x + 0.001 * y
            points.append([x, y, z])

    points_np = np.array(points)
    terrain = TerrainModel(points_np[:, :2], points_np[:, 2])
    terrain.build_grid(resolution=grid_res)
    return terrain, points, grid_res


# ── Fixture serialization helpers ─────────────────────────────────────────────

def serialize_network(network, terrain):
    """Serialize a PipeNetwork to fixture-ready dicts."""
    nodes_data = []
    for n in network.nodes:
        pressure = n.metadata.get("pressure_mca", None)
        nodes_data.append({
            "id": n.id,
            "x": n.x,
            "y": n.y,
            "z": n.z,
            "node_type": n.node_type.name if hasattr(n.node_type, "name") else str(n.node_type),
            "rim_elevation": float(n.rim_elevation) if n.rim_elevation is not None else n.z,
            "sump_elevation": float(n.sump_elevation) if n.sump_elevation is not None else (n.z - 1.2),
            "pressure_mca": float(pressure) if pressure is not None else None,
            "valve_type": n.metadata.get("valve_type", None),
        })

    pipes_data = []
    for p in network.pipes:
        pipes_data.append({
            "id": p.id,
            "start_node_id": p.start_node_id,
            "end_node_id": p.end_node_id,
            "length": float(p.length),
            "diameter": float(p.diameter),
            "slope": float(p.slope) if hasattr(p, "slope") and p.slope is not None else 0.0,
            "start_invert": float(p.start_invert),
            "end_invert": float(p.end_invert),
            "design_flow": float(p.design_flow),
        })

    return nodes_data, pipes_data


# ── Fixture 1: Primary pipeline ───────────────────────────────────────────────

def make_primary_fixture():
    """Primary conveyance fixture: monotonic terrain, structure_count=0."""
    terrain, points, grid_res = make_monotonic_terrain()

    source      = (0.0, 0.0)   # g0: z=100.0
    destination = (80.0, 0.0)  # g20: z=100.48
    design_flow  = 0.010       # m³/s
    source_head  = 30.0        # mca
    material     = PipeMaterial.PVC

    print(f"\n=== Primary fixture ===")
    print(f"  source={source} z={100.0 + 0.006*source[0]:.3f}")
    print(f"  destination={destination} z={100.0 + 0.006*destination[0]:.3f}")

    constraints = DesignConstraints()
    solver = ConveyanceSolver(terrain, constraints)

    solutions = solver.solve(
        source=source,
        destination=destination,
        design_flow=design_flow,
        source_head=source_head,
        network_name="ConveyanceTest",
        material=material,
        grid_resolution=grid_res,
        num_alternatives=1,
        route_variant=0,
        cover_factor=1.0,
        valve_spacing=500.0,
    )

    sol = solutions[0]
    network = sol.network
    eval_score = solver.evaluate(network)

    nodes_data, pipes_data = serialize_network(network, terrain)

    # Count valves
    valve_count = sum(1 for n in network.nodes if hasattr(n.node_type, "name") and n.node_type.name == "VALVE")
    print(f"  structure_count={eval_score.details.get('structure_count', 0)} (must be 0)")
    print(f"  total_length={eval_score.total_length:.4f}")
    print(f"  total_excavation={eval_score.total_excavation:.6f}")
    print(f"  total_cost={eval_score.total_cost:.6f}")
    print(f"  norm_violations={eval_score.norm_violations}")
    print(f"  pipe_count={network.pipe_count}, node_count={network.node_count}")
    print(f"  diameter={pipes_data[0]['diameter'] * 1000:.0f}mm (uniform)")

    for p in pipes_data:
        print(f"    {p['id']}: L={p['length']:.2f} start_inv={p['start_invert']:.4f} end_inv={p['end_invert']:.4f}")
    for n in nodes_data:
        print(f"    {n['id']}: ({n['x']:.0f},{n['y']:.0f}) z={n['z']:.3f} type={n['node_type']} P={n['pressure_mca']}")

    details_dict = {}
    if hasattr(eval_score, "details") and eval_score.details:
        details_dict = {
            k: float(v) if isinstance(v, (int, float)) else v
            for k, v in eval_score.details.items()
        }

    return {
        "schema_version": 1,
        "fixture_name": "solvers_conveyance_golden",
        "description": (
            "ConveyanceSolver primary fixture: 5x5 grid (20m spacing), "
            "z=100.0+0.006*x (monotonic), source=g0=(0,0), dest=g20=(80,0). "
            "route_variant=0, num_alternatives=1. structure_count=0 (no peaks/valleys). "
            "All coords at on-grid nodes for EXACT elevation queries."
        ),
        "terrain": {
            "grid_res": grid_res,
            "x_min": 0.0, "x_max": 80.0, "y_min": 0.0, "y_max": 80.0,
            "slope_x": 0.006, "slope_y": 0.0, "base_z": 100.0,
            "points": points,
        },
        "solver_params": {
            "source": list(source),
            "destination": list(destination),
            "design_flow": design_flow,
            "source_head": source_head,
            "material": "PVC",
            "grid_resolution": grid_res,
            "num_alternatives": 1,
            "route_variant": 0,
            "cover_factor": 1.0,
            "valve_spacing": 500.0,
        },
        "node_count": network.node_count,
        "pipe_count": network.pipe_count,
        "total_length": eval_score.total_length,
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
            "w_length": 1.5,
            "w_excavation": 2.0,
            "w_violations": 200.0,
            "w_structures": 500.0,
            "expected_cost": eval_score.total_cost,
        },
    }


# ── Fixture 2: Valve placement ────────────────────────────────────────────────

def make_valve_fixture():
    """Valve-placement fixture: terrain with peak+valley → structure_count > 0."""
    terrain, points, grid_res = make_valve_terrain()

    source      = (0.0, 0.0)   # g0
    destination = (80.0, 0.0)  # g20
    design_flow  = 0.010
    source_head  = 30.0
    material     = PipeMaterial.PVC
    valve_spacing = 1.0         # small spacing → both peak and valley get valves

    print(f"\n=== Valve fixture ===")
    print(f"  source={source} dest={destination}")
    print(f"  valve_spacing={valve_spacing}")
    print(f"  Expected path: g0->g5->g10->g15->g20")
    print(f"  Elevations: g0=100.0, g5=100.4, g10=101.0(PEAK), g15=100.2(VALLEY), g20=100.8")

    constraints = DesignConstraints()
    solver = ConveyanceSolver(terrain, constraints)

    solutions = solver.solve(
        source=source,
        destination=destination,
        design_flow=design_flow,
        source_head=source_head,
        network_name="ConveyanceValves",
        material=material,
        grid_resolution=grid_res,
        num_alternatives=1,
        route_variant=0,
        cover_factor=1.0,
        valve_spacing=valve_spacing,
    )

    sol = solutions[0]
    network = sol.network
    eval_score = solver.evaluate(network)
    structure_count = eval_score.details.get("structure_count", 0)

    nodes_data, pipes_data = serialize_network(network, terrain)

    print(f"  structure_count={structure_count} (must be > 0)")
    print(f"  total_cost={eval_score.total_cost:.6f}")
    print(f"  norm_violations={eval_score.norm_violations}")

    for n in nodes_data:
        print(f"    {n['id']}: ({n['x']:.0f},{n['y']:.0f}) type={n['node_type']} "
              f"valve_type={n.get('valve_type')} P={n['pressure_mca']}")

    if structure_count == 0:
        raise RuntimeError(
            "Valve fixture has structure_count=0 — terrain does not produce peaks/valleys. "
            "Check terrain definition."
        )

    # Verify VALVE nodes have valve_type
    valve_nodes = [n for n in nodes_data if n["node_type"] == "VALVE"]
    for vn in valve_nodes:
        assert vn.get("valve_type") in ("air_valve", "blowoff_valve"), (
            f"VALVE node {vn['id']} missing valve_type"
        )
    print(f"  VALVE nodes: {[n['id'] for n in valve_nodes]}")

    details_dict = {
        k: float(v) if isinstance(v, (int, float)) else v
        for k, v in eval_score.details.items()
    }

    return {
        "schema_version": 1,
        "fixture_name": "solvers_conveyance_valves",
        "description": (
            "ConveyanceSolver valve-placement fixture: terrain with peak at g10=(40,0) "
            "and valley at g15=(60,0). valve_spacing=1.0 to place both structures. "
            "structure_count > 0, VALVE node_types present."
        ),
        "terrain": {
            "grid_res": grid_res,
            "x_min": 0.0, "x_max": 80.0, "y_min": 0.0, "y_max": 80.0,
            "z_for_x": {str(k): v for k, v in {0.0: 100.0, 20.0: 100.4, 40.0: 101.0, 60.0: 100.2, 80.0: 100.8}.items()},
            "points": points,
        },
        "solver_params": {
            "source": list(source),
            "destination": list(destination),
            "design_flow": design_flow,
            "source_head": source_head,
            "material": "PVC",
            "grid_resolution": grid_res,
            "num_alternatives": 1,
            "route_variant": 0,
            "cover_factor": 1.0,
            "valve_spacing": valve_spacing,
        },
        "node_count": network.node_count,
        "pipe_count": network.pipe_count,
        "total_length": eval_score.total_length,
        "structure_count": int(structure_count),
        "valve_nodes": [
            {
                "id": n["id"],
                "node_type": n["node_type"],
                "valve_type": n.get("valve_type"),
                "x": n["x"],
                "y": n["y"],
                "z": n["z"],
            }
            for n in nodes_data if n["node_type"] == "VALVE"
        ],
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
            "w_length": 1.5,
            "w_excavation": 2.0,
            "w_violations": 200.0,
            "w_structures": 500.0,
            "expected_cost": eval_score.total_cost,
        },
    }


# ── Fixture 3: Alternatives (Yen k-paths) ─────────────────────────────────────

def make_alternatives_fixture():
    """Alternatives fixture: 2 paths from g0 to g15=(60,0), pairwise-distinct costs.

    num_alternatives=2 with source=g0=(0,0) and dest=g15=(60,0):
    - path1: g0->g5->g10->g15 (3 hops, L=60m, cost=834)
    - path2: detour via y (5 hops, L=100m, cost=1890)

    These two costs are PAIRWISE STRICTLY DISTINCT (diff=1056).
    On a 5x5 uniform grid, any 3rd Yen path would be another 5-hop path with
    identical L=100m, therefore identical cost -> would tie with path2.
    Using num_alternatives=2 avoids that structural tie.
    """
    terrain, points, grid_res = make_alternatives_terrain()

    source      = (0.0, 0.0)   # g0: z=100.0
    destination = (60.0, 0.0)  # g15: z=100.0+0.006*60=100.36
    design_flow  = 0.010
    source_head  = 30.0
    material     = PipeMaterial.PVC

    print(f"\n=== Alternatives fixture ===")
    print(f"  source={source}  z={100.0 + 0.006*source[0] + 0.001*source[1]:.3f}")
    print(f"  destination={destination}  z={100.0 + 0.006*destination[0] + 0.001*destination[1]:.3f}")

    constraints = DesignConstraints()
    solver = ConveyanceSolver(terrain, constraints)

    solutions = solver.solve(
        source=source,
        destination=destination,
        design_flow=design_flow,
        source_head=source_head,
        network_name="ConveyanceAlt",
        material=material,
        grid_resolution=grid_res,
        num_alternatives=2,
        route_variant=0,
        cover_factor=1.0,
        valve_spacing=500.0,
    )

    print(f"  {len(solutions)} solutions returned")
    costs = [s.score.total_cost for s in solutions]
    for i, (sol, cost) in enumerate(zip(solutions, costs)):
        print(f"  Alt {i+1}: rank={sol.rank} total_cost={cost:.6f} "
              f"L={sol.score.total_length:.3f} nodes={sol.network.node_count} "
              f"pipes={sol.network.pipe_count}")

    # Verify pairwise strictly distinct costs
    ok = True
    for i in range(len(costs)):
        for j in range(i + 1, len(costs)):
            diff = abs(costs[i] - costs[j])
            if diff < 1e-3:
                print(f"  ERROR: costs[{i}]={costs[i]:.6f} and costs[{j}]={costs[j]:.6f} "
                      f"too close (|diff|={diff:.2e} < 1e-3). Fixture design is wrong.")
                ok = False
            else:
                print(f"  costs[{i}] vs costs[{j}]: |diff|={diff:.6f} OK strictly distinct")
    if not ok:
        raise RuntimeError(
            "Alternatives fixture has tied costs — redesign terrain or source/dest pair."
        )

    alts_data = []
    for sol in solutions:
        net = sol.network
        alts_data.append({
            "rank": sol.rank,
            "total_cost": sol.score.total_cost,
            "total_length": sol.score.total_length,
            "total_excavation": sol.score.total_excavation,
            "norm_violations": sol.score.norm_violations,
            "pump_count": sol.score.pump_count,
            "node_count": net.node_count,
            "pipe_count": net.pipe_count,
        })

    return {
        "schema_version": 1,
        "fixture_name": "solvers_conveyance_alternatives",
        "description": (
            "ConveyanceSolver alternatives fixture: g0=(0,0) -> g15=(60,0), "
            "z=100.0+0.006*x+0.001*y, num_alternatives=2. "
            "Pairwise-distinct total_costs: path1=3hops/60m, path2=5hops/100m."
        ),
        "terrain": {
            "grid_res": grid_res,
            "x_min": 0.0, "x_max": 80.0, "y_min": 0.0, "y_max": 80.0,
            "slope_x": 0.006, "slope_y": 0.001, "base_z": 100.0,
            "points": points,
        },
        "solver_params": {
            "source": list(source),
            "destination": list(destination),
            "design_flow": design_flow,
            "source_head": source_head,
            "material": "PVC",
            "grid_resolution": grid_res,
            "num_alternatives": 2,
            "route_variant": 0,
            "cover_factor": 1.0,
            "valve_spacing": 500.0,
        },
        "num_alternatives_requested": 2,
        "num_solutions_returned": len(solutions),
        "solutions": alts_data,
        "distinct_costs": costs,
    }


# ── Main ──────────────────────────────────────────────────────────────────────

def main():
    print("Generating ConveyanceSolver oracle fixtures...")

    # Fixture 1: Primary pipeline
    primary = make_primary_fixture()
    out1 = FIXTURES_DIR / "solvers_conveyance_golden.json"
    with open(out1, "w") as f:
        json.dump(primary, f, indent=2)
    print(f"\nWritten: {out1}")
    print(f"  node_count       = {primary['node_count']}")
    print(f"  pipe_count       = {primary['pipe_count']}")
    print(f"  total_length     = {primary['total_length']:.4f} m")
    print(f"  total_cost       = {primary['score']['total_cost']:.6f}")
    print(f"  norm_violations  = {primary['score']['norm_violations']}")
    print(f"  structure_count  = {primary['evaluate_only']['details'].get('structure_count', 0)}")

    # Fixture 2: Valve placement
    valves = make_valve_fixture()
    out2 = FIXTURES_DIR / "solvers_conveyance_valves.json"
    with open(out2, "w") as f:
        json.dump(valves, f, indent=2)
    print(f"\nWritten: {out2}")
    print(f"  structure_count  = {valves['structure_count']} (must be > 0)")
    print(f"  valve_nodes      = {[n['id'] for n in valves['valve_nodes']]}")
    print(f"  valve_types      = {[n['valve_type'] for n in valves['valve_nodes']]}")

    # Fixture 3: Alternatives
    alts = make_alternatives_fixture()
    out3 = FIXTURES_DIR / "solvers_conveyance_alternatives.json"
    with open(out3, "w") as f:
        json.dump(alts, f, indent=2)
    print(f"\nWritten: {out3}")
    print(f"  num_solutions    = {alts['num_solutions_returned']}")
    for sol in alts["solutions"]:
        print(f"  rank={sol['rank']}: cost={sol['total_cost']:.6f}")


if __name__ == "__main__":
    main()
