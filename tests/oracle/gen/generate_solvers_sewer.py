"""Generate SewerSolver oracle fixture for hydro-solvers T-5.2 parity tests.

Emits:
  tests/oracle/fixtures/solvers_sewer_golden.json

Fixture parameters (locked — changing breaks golden vectors):
  - Terrain: 5x5 regular grid, 20m spacing, (0..80 in both axes)
  - Surface: z = 100.0 - 0.003 * x (gentle 0.3% slope in x-direction, all gravity-feasible)
  - Grid node IDs: g{counter}, i-outer (x) j-inner (y)
  - Service points: on-grid coordinates for EXACT elevation query (risk #1)
      sp0 = (20.0, 20.0) → g{i=1,j=1} = g6
      sp1 = (40.0, 40.0) → g{i=2,j=2} = g12
      sp2 = (60.0, 20.0) → g{i=3,j=1} = g16
  - Outlet: (0.0, 0.0) → g0 (lowest elevation corner)
  - flow_per_service: 0.002 m³/s
  - material: PVC (roughness n=0.009)
  - grid_resolution: 20.0 m
  - num_alternatives: 1 (only primary Steiner-based solution)
  - Default DesignConstraints

IMPORTANT: All service and outlet points use on-grid node coordinates
so that terrain.elevation_at() calls return EXACT values (no interpolation error).
This addresses the known risk #1 from the Phase-5 task forecast.

Grid layout (5x5, i=0..4 x-axis, j=0..4 y-axis):
  counter = i*5 + j
  x = i * 20, y = j * 20
  g0  = (0,0)    g1  = (0,20)   g2  = (0,40)   g3  = (0,60)   g4  = (0,80)
  g5  = (20,0)   g6  = (20,20)  g7  = (20,40)  g8  = (20,60)  g9  = (20,80)
  g10 = (40,0)   g11 = (40,20)  g12 = (40,40)  g13 = (40,60)  g14 = (40,80)
  g15 = (60,0)   g16 = (60,20)  g17 = (60,40)  g18 = (60,60)  g19 = (60,80)
  g20 = (80,0)   g21 = (80,20)  g22 = (80,40)  g23 = (80,60)  g24 = (80,80)

Elevation: z(x,y) = 100.0 - 0.003 * x
  g0  z=100.000  g5  z=99.940  g10 z=99.880
  g6  z=99.940   g12 z=99.880  g16 z=99.820
  g15 z=99.820   g16 z=99.820  outlet g0 z=100.000

Wait — outlet at g0 has highest elevation (100.0). Service points are at
lower elevation. So flow IS downhill from services to outlet? No, x increases
outward from outlet.

Correction: outlet should be at the LOWEST elevation. Use g20=(80,0) as outlet?
No. For sewer design outlet is the discharge point (lowest point). Let's place
outlet at g20 = (80, 0) with z=99.820 (lowest x) — actually z = 100 - 0.003*80 = 99.76.

Let me redefine: slope in negative x direction (outlet at x=80):
  z(x, y) = 100.0 - 0.003 * (80.0 - x) = 100.0 - 0.24 + 0.003*x = 99.76 + 0.003*x

So z increases with x. Outlet at g20=(80,0): z=100.0. Service points at lower x:
  sp0 at g6=(20,20): z = 99.76 + 0.003*20 = 99.76 + 0.06 = 99.82
  sp1 at g12=(40,40): z = 99.76 + 0.003*40 = 99.76 + 0.12 = 99.88
  sp2 at g16=(60,20): z = 99.76 + 0.003*60 = 99.76 + 0.18 = 99.94

All service points have LOWER z than outlet. This violates gravity flow.

SIMPLEST correct approach: slope in x-direction, outlet at x=0, services at higher x.
  z(x, y) = 100.0 + 0.003 * x (z increases with x)
  Outlet at g0=(0,0): z=100.0
  sp0 at g6=(20,20): z=100.06
  sp1 at g12=(40,40): z=100.12
  sp2 at g16=(60,20): z=100.18

Services are ABOVE outlet → gravity flow toward outlet at g0. This is correct sewer design.

But min_slope = 0.005. The terrain slope = 0.003 < 0.005, so ALL pipes need_pump?
Let's check: natural_slope = (start.z - end.z) / length. For a pipe from g6 to g0:
  Steiner path likely goes g6→g5→g0 (two 20m segments)
  g6=(20,20): z=100.06, g5=(20,0): z=100.06, g0=(0,0): z=100.0
  g6→g5: dz=0, length=20 → slope=0 < 0.005 → needs_pump
  g5→g0: dz=0.06, length=20 → slope=0.003 < 0.005 → needs_pump

All pipes with 0.3% slope would need pumps. That defeats testing invert logic.

FINAL CORRECT: Use steeper slope (0.6%) so natural_slope > min_slope (0.005):
  z(x, y) = 100.0 + 0.006 * x
  g0=(0,0): z=100.000 (outlet, lowest x → lowest z)... Wait no.
  Outlet at g0=(0,0): z=100.0
  g6=(20,20): z=100.0+0.006*20=100.12 → natural_slope from g6 toward g0 = 0.12/20 = 0.006 > 0.005 ✓

So with z = 100.0 + 0.006*x and outlet at g0=(0,0), services at higher x:
  g6=(20,20): z=100.12, natural slope toward g0 along x-axis: 0.006 ≥ 0.005 ✓
  g12=(40,40): z=100.24
  g16=(60,20): z=100.36

All gravity-feasible via the x-aligned paths. Good.

CostFunction: default (w_length=1.0, w_excavation=2.0, w_slope_penalty=5.0, w_pump_penalty=50.0)
              min_slope=0.005, max_slope=0.05, min_cover=1.2
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
from hydro_engine.solvers.sewer import SewerSolver

# ── Terrain grid ──────────────────────────────────────────────────────────────

def make_sewer_fixture():
    """Build a 5x5 sewer fixture with gravity-feasible terrain."""

    # 5x5 grid, 20m spacing, slope=0.006 in x-direction
    # z(x,y) = 100.0 + 0.006 * x
    grid_res = 20.0
    xs = np.arange(0.0, 80.0 + grid_res, grid_res)  # 0,20,40,60,80 → 5 values
    ys = np.arange(0.0, 80.0 + grid_res, grid_res)  # 0,20,40,60,80 → 5 values

    points = []
    for x in xs:
        for y in ys:
            z = 100.0 + 0.006 * x
            points.append([x, y, z])

    points_np = np.array(points)

    terrain = TerrainModel(points_np[:, :2], points_np[:, 2])
    terrain.build_grid(resolution=grid_res)

    # ── Design parameters ─────────────────────────────────────────────────
    # On-grid service points (node coordinates) for EXACT elevation queries (risk #1)
    # g0=(0,0), g6=(20,20), g12=(40,40), g16=(60,20), g20=(80,0)
    # Outlet at lowest elevation = g0=(0,0): z=100.0
    # Services at g6=(20,20), g12=(40,40), g16=(60,20)
    outlet = (0.0, 0.0)
    service_points = [
        (20.0, 20.0),  # g6: z=100.12
        (40.0, 40.0),  # g12: z=100.24
        (60.0, 20.0),  # g16: z=100.36
    ]

    constraints = DesignConstraints()
    solver = SewerSolver(terrain, constraints)

    solutions = solver.solve(
        service_points=service_points,
        outlet=outlet,
        network_name="SewerTest",
        flow_per_service=0.002,
        material=PipeMaterial.PVC,
        grid_resolution=grid_res,
        num_alternatives=1,
        route_variant=0,
        slope_factor=1.0,
        cover_factor=1.0,
        diameter_offset=0,
        manhole_spacing=None,
    )

    sol = solutions[0]
    network = sol.network
    score = sol.score

    # ── Serialize network ─────────────────────────────────────────────────
    nodes_data = []
    for n in network.nodes:
        nodes_data.append({
            "id": n.id,
            "x": n.x,
            "y": n.y,
            "z": n.z,
            "node_type": n.node_type.name if hasattr(n.node_type, "name") else str(n.node_type),
            "rim_elevation": n.rim_elevation,
            "sump_elevation": n.sump_elevation,
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
            "roughness": p.roughness,
            "bypassed_by_pump": bool(p.metadata.get("bypassed_by_pump", False)),
        })

    # ── Evaluate only sub-fixture ─────────────────────────────────────────
    # Re-evaluate the same network to generate the evaluate_only sub-fixture.
    # This lets us test evaluate() independently.
    eval_score = solver.evaluate(network)

    details_dict = {}
    if hasattr(eval_score, "details") and eval_score.details:
        details_dict = {k: float(v) if isinstance(v, (int, float)) else v
                        for k, v in eval_score.details.items()}

    # ── Build fixture dict ────────────────────────────────────────────────
    fixture = {
        "schema_version": 1,
        "fixture_name": "solvers_sewer_golden",
        "description": (
            "SewerSolver oracle fixture: 5x5 grid (20m spacing), z=100+0.006*x, "
            "outlet=g0=(0,0), services=g6,g12,g16. "
            "Gravity-feasible (natural slope 0.006 >= min_slope 0.005). "
            "All service/outlet at on-grid coordinates for EXACT elevation queries."
        ),
        # Terrain params (for Rust to reconstruct the same terrain)
        "terrain": {
            "grid_res": grid_res,
            "x_min": 0.0,
            "x_max": 80.0,
            "y_min": 0.0,
            "y_max": 80.0,
            "slope_x": 0.006,
            "base_z": 100.0,
            "points": points,
        },
        # Solver input params
        "solver_params": {
            "outlet": list(outlet),
            "service_points": [list(sp) for sp in service_points],
            "flow_per_service": 0.002,
            "material": "PVC",
            "grid_resolution": grid_res,
            "num_alternatives": 1,
            "route_variant": 0,
            "slope_factor": 1.0,
            "cover_factor": 1.0,
            "diameter_offset": 0,
            "manhole_spacing": None,
        },
        # Network structure
        "node_count": network.node_count,
        "pipe_count": network.pipe_count,
        "total_length": network.total_length,
        "nodes": nodes_data,
        "pipes": pipes_data,
        # Primary score
        "score": {
            "total_cost": eval_score.total_cost,
            "total_length": eval_score.total_length,
            "total_excavation": eval_score.total_excavation,
            "norm_violations": eval_score.norm_violations,
            "pump_count": eval_score.pump_count,
        },
        # Evaluate-only sub-fixture (for independent evaluate() testing)
        "evaluate_only": {
            "total_cost": eval_score.total_cost,
            "total_length": eval_score.total_length,
            "total_excavation": eval_score.total_excavation,
            "norm_violations": eval_score.norm_violations,
            "pump_count": eval_score.pump_count,
            "details": details_dict,
        },
        # Cost formula assertion values
        "cost_formula": {
            "w_length": 1.0,
            "w_excavation": 2.0,
            "w_violations": 100.0,
            "expected_cost": eval_score.total_cost,
        },
    }

    return fixture


def main():
    print("Generating SewerSolver oracle fixture...")
    fixture = make_sewer_fixture()

    output_path = FIXTURES_DIR / "solvers_sewer_golden.json"
    with open(output_path, "w") as f:
        json.dump(fixture, f, indent=2)

    print(f"Written: {output_path}")
    print(f"  node_count = {fixture['node_count']}")
    print(f"  pipe_count = {fixture['pipe_count']}")
    print(f"  total_length = {fixture['total_length']:.3f} m")
    print(f"  total_cost = {fixture['score']['total_cost']:.6f}")
    print(f"  total_excavation = {fixture['score']['total_excavation']:.6f}")
    print(f"  norm_violations = {fixture['score']['norm_violations']}")
    print(f"  pump_count = {fixture['score']['pump_count']}")
    print()
    print("Pipes:")
    for p in fixture["pipes"]:
        print(
            f"  {p['id']}: {p['start_node_id']}→{p['end_node_id']} "
            f"L={p['length']:.1f}m D={p['diameter']*1000:.0f}mm "
            f"slope={p['slope']:.4f} inv={p['start_invert']:.3f}→{p['end_invert']:.3f} "
            f"pump={p['bypassed_by_pump']}"
        )
    print()
    print("Nodes:")
    for n in fixture["nodes"]:
        print(
            f"  {n['id']}: ({n['x']:.0f},{n['y']:.0f}) z={n['z']:.3f} "
            f"type={n['node_type']} rim={n['rim_elevation']:.3f} sump={n['sump_elevation']:.3f}"
        )


if __name__ == "__main__":
    main()
