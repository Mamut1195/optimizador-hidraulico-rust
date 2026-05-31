"""Generate PumpStationSolver oracle fixtures for hydro-solvers T-5.4 parity tests.

Emits:
  tests/oracle/fixtures/solvers_pump_station_golden.json
  tests/oracle/fixtures/solvers_pump_station_alternatives.json

Fixture parameters (locked — changing breaks golden vectors):

Fixture 1 (primary / golden):
  - design_flow=0.050, suction_elev=100.0, discharge_elev=115.0
  - suction_l=10.0, discharge_l=50.0, suction_d=0.3, discharge_d=0.25
  - material=STEEL, num_alternatives=1
  - Asserts: tdh, power_kw (motor exact), efficiency, wet_well dims, total_cost

Fixture 2 (alternatives / motor-table boundary):
  - design_flow=0.100, suction_elev=95.0, discharge_elev=125.0 (big lift)
  - suction_l=15.0, discharge_l=80.0, suction_d=0.25, discharge_d=0.20
  - material=STEEL, num_alternatives=3
  - 3 alternatives: num_operating = 1, 2, 3 (range(1, 4))
  - Pairwise-distinct total_costs (abort if tied)
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

from hydro_engine.core.constraints import DesignConstraints
from hydro_engine.core.types import PipeMaterial
from hydro_engine.solvers.pump_station import PumpStationSolver


# ── Serialisation helpers ─────────────────────────────────────────────────────


def serialize_solution(solver, sol):
    """Serialize a pump station solution (network + score) to a dict."""
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
        nodes_data.append({
            "id": n.id,
            "x": n.x,
            "y": n.y,
            "z": n.z,
            "node_type": (
                n.node_type.name
                if hasattr(n.node_type, "name")
                else str(n.node_type)
            ),
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
        })

    # Extract pump / wet_well metadata from network
    meta = network.metadata
    pump_info = meta.get("pump", {})
    wet_well_info = meta.get("wet_well", {})

    return {
        "network": {
            "name": network.name,
            "node_count": network.node_count,
            "pipe_count": network.pipe_count,
            "total_length": network.total_length,
            "nodes": nodes_data,
            "pipes": pipes_data,
            "metadata": {
                "tdh": meta.get("tdh"),
                "static_lift": meta.get("static_lift"),
                "hw_c": meta.get("hw_c"),
                "pump": pump_info,
                "wet_well": wet_well_info,
            },
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


# ── Fixture 1: primary (golden) ───────────────────────────────────────────────


def make_primary_fixture():
    """Primary pump station fixture: single alternative, default steel pipe."""
    design_flow = 0.050
    suction_elev = 100.0
    discharge_elev = 115.0
    suction_l = 10.0
    discharge_l = 50.0
    suction_d = 0.3
    discharge_d = 0.25
    material = PipeMaterial.STEEL
    num_alternatives = 1

    constraints = DesignConstraints()
    solver = PumpStationSolver(None, constraints)

    solutions = solver.solve(
        design_flow=design_flow,
        suction_elevation=suction_elev,
        discharge_elevation=discharge_elev,
        suction_pipe_length=suction_l,
        discharge_pipe_length=discharge_l,
        suction_diameter=suction_d,
        discharge_diameter=discharge_d,
        material=material,
        num_alternatives=num_alternatives,
    )

    sol = solutions[0]
    sol_data = serialize_solution(solver, sol)

    meta = sol_data["network"]["metadata"]
    pump_info = meta["pump"]
    wet_well_info = meta["wet_well"]

    print(f"Primary fixture:")
    print(f"  rank          = {sol.rank}")
    print(f"  tdh           = {meta['tdh']}")
    print(f"  static_lift   = {meta['static_lift']}")
    print(f"  hw_c          = {meta['hw_c']}")
    print(f"  power_kw      = {pump_info.get('power_kw')}")
    print(f"  efficiency    = {pump_info.get('efficiency')}")
    print(f"  num_operating = {pump_info.get('num_operating')}")
    print(f"  total_pumps   = {pump_info.get('total_pumps')}")
    print(f"  well_volume   = {wet_well_info.get('volume_m3')}")
    print(f"  total_length  = {sol_data['network']['total_length']:.6f}")
    print(f"  total_cost    = {sol_data['score']['total_cost']:.6f}")
    print(f"  violations    = {sol_data['score']['norm_violations']}")
    print(f"  pump_count    = {sol_data['score']['pump_count']}")

    fixture = {
        "schema_version": 1,
        "fixture_name": "solvers_pump_station_golden",
        "description": (
            "PumpStationSolver oracle fixture: design_flow=0.050 m3/s, "
            "suction_elev=100.0, discharge_elev=115.0, "
            "suction_l=10.0, discharge_l=50.0, suction_d=0.3, discharge_d=0.25, "
            "material=STEEL, num_alternatives=1. DesignConstraints() defaults. "
            "NOTE: SolutionScore.total_excavation is repurposed to carry wet_well_volume_m3."
        ),
        "solver_params": {
            "design_flow": design_flow,
            "suction_elevation": suction_elev,
            "discharge_elevation": discharge_elev,
            "suction_pipe_length": suction_l,
            "discharge_pipe_length": discharge_l,
            "suction_diameter": suction_d,
            "discharge_diameter": discharge_d,
            "material": "STEEL",
            "num_alternatives": num_alternatives,
        },
        "node_count": sol_data["network"]["node_count"],
        "pipe_count": sol_data["network"]["pipe_count"],
        "total_length": sol_data["network"]["total_length"],
        "nodes": sol_data["network"]["nodes"],
        "pipes": sol_data["network"]["pipes"],
        "network_metadata": sol_data["network"]["metadata"],
        "score": sol_data["score"],
        "evaluate_only": sol_data["evaluate_only"],
        "cost_formula": {
            "w_equipment": 1200.0,
            "w_civil": 800.0,
            "w_pipe": 150.0,
            "w_violations": 5000.0,
            "expected_cost": sol_data["score"]["total_cost"],
        },
        "note_total_excavation_repurposed": (
            "SolutionScore.total_excavation = wet_well.volume_m3 (not real excavation)"
        ),
    }
    return fixture


# ── Fixture 2: alternatives (motor-table boundary) ───────────────────────────


def make_alternatives_fixture():
    """Alternatives fixture: 3 pump configs with pairwise-distinct costs."""
    design_flow = 0.100
    suction_elev = 95.0
    discharge_elev = 125.0  # big lift = 30m static head
    suction_l = 15.0
    discharge_l = 80.0
    suction_d = 0.25
    discharge_d = 0.20
    material = PipeMaterial.STEEL
    num_alternatives = 3  # generates num_operating = 1, 2, 3

    constraints = DesignConstraints()
    solver = PumpStationSolver(None, constraints)

    solutions = solver.solve(
        design_flow=design_flow,
        suction_elevation=suction_elev,
        discharge_elevation=discharge_elev,
        suction_pipe_length=suction_l,
        discharge_pipe_length=discharge_l,
        suction_diameter=suction_d,
        discharge_diameter=discharge_d,
        material=material,
        num_alternatives=num_alternatives,
    )

    print(f"\nAlternatives fixture: {len(solutions)} solutions")
    costs = [sol.score.total_cost for sol in solutions]
    for i, (sol, cost) in enumerate(zip(solutions, costs)):
        meta = sol.network.metadata
        pump_info = meta.get("pump", {})
        wet_well_info = meta.get("wet_well", {})
        print(
            f"  Alt {i+1}: total_cost={cost:.6f}, "
            f"num_operating={pump_info.get('num_operating')}, "
            f"power_kw={pump_info.get('power_kw')}, "
            f"total_pumps={pump_info.get('total_pumps')}, "
            f"well_vol={wet_well_info.get('volume_m3')}, "
            f"rank={sol.rank}"
        )

    # Assert pairwise strictly distinct costs
    print("\nPairwise distinctness check:")
    for i in range(len(costs)):
        for j in range(i + 1, len(costs)):
            diff = abs(costs[i] - costs[j])
            if diff < 1e-3:
                raise AssertionError(
                    f"PumpStation alternatives costs[{i}]={costs[i]:.6f} and "
                    f"costs[{j}]={costs[j]:.6f} are NOT pairwise strictly distinct "
                    f"(|diff|={diff:.2e} < 1e-3). "
                    "Redesign the fixture parameters."
                )
            else:
                print(
                    f"  costs[{i}]={costs[i]:.6f} vs costs[{j}]={costs[j]:.6f}: "
                    f"|diff|={diff:.6f} >= 1e-3  OK"
                )

    alts_data = []
    for sol in solutions:
        network = sol.network
        eval_score = solver.evaluate(network)
        meta = network.metadata
        pump_info = meta.get("pump", {})
        wet_well_info = meta.get("wet_well", {})
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
            "node_count": network.node_count,
            "pipe_count": network.pipe_count,
            "details": details_dict,
            "network_metadata": {
                "tdh": meta.get("tdh"),
                "static_lift": meta.get("static_lift"),
                "hw_c": meta.get("hw_c"),
                "pump": pump_info,
                "wet_well": wet_well_info,
            },
        })

    return {
        "schema_version": 1,
        "fixture_name": "solvers_pump_station_alternatives",
        "description": (
            "PumpStationSolver alternatives fixture: 3 pump configs "
            "(num_operating = 1, 2, 3). design_flow=0.100 m3/s, "
            "suction_elev=95.0, discharge_elev=125.0 (big 30m static lift), "
            "suction_l=15.0, discharge_l=80.0, suction_d=0.25, discharge_d=0.20, STEEL. "
            "Pairwise-distinct total_costs guarantee ordering assertions are non-vacuous."
        ),
        "solver_params": {
            "design_flow": design_flow,
            "suction_elevation": suction_elev,
            "discharge_elevation": discharge_elev,
            "suction_pipe_length": suction_l,
            "discharge_pipe_length": discharge_l,
            "suction_diameter": suction_d,
            "discharge_diameter": discharge_d,
            "material": "STEEL",
            "num_alternatives": num_alternatives,
        },
        "num_alternatives_requested": num_alternatives,
        "num_solutions_returned": len(solutions),
        "solutions": alts_data,
        "distinct_costs": costs,
    }


# ── Main ──────────────────────────────────────────────────────────────────────


def main():
    print("Generating PumpStationSolver oracle fixtures...")

    # Fixture 1: primary / golden
    print("\n=== Fixture 1: primary (num_alternatives=1) ===")
    fixture1 = make_primary_fixture()
    out1 = FIXTURES_DIR / "solvers_pump_station_golden.json"
    with open(out1, "w") as f:
        json.dump(fixture1, f, indent=2)
    print(f"\nWritten: {out1}")

    # Fixture 2: alternatives
    print("\n=== Fixture 2: alternatives (num_alternatives=3) ===")
    fixture2 = make_alternatives_fixture()
    out2 = FIXTURES_DIR / "solvers_pump_station_alternatives.json"
    with open(out2, "w") as f:
        json.dump(fixture2, f, indent=2)
    print(f"Written: {out2}")

    print("\nAll pump station fixtures generated successfully.")


if __name__ == "__main__":
    main()
