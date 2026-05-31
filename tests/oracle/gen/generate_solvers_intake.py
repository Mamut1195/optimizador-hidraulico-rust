"""Generate IntakeSolver oracle fixtures for hydro-solvers T-5.4 parity tests.

Emits:
  tests/oracle/fixtures/solvers_intake_golden.json
  tests/oracle/fixtures/solvers_intake_alternatives.json

Fixture parameters (locked — changing breaks golden vectors):

Fixture 1 (primary / golden):
  - design_flow=0.050, source_elev=120.0, pipe_elev=118.0
  - channel_slope=0.005, material=CONCRETE, weir_type=0 (rectangular)
  - num_alternatives=1
  - Asserts: screen dims, channel depth (linspace), weir head/length, pipe diam, total_cost

Fixture 2 (alternatives + v_notch):
  - design_flow=0.030, source_elev=115.0, pipe_elev=113.0
  - channel_slope=0.005, material=CONCRETE, weir_type=1 (v_notch)
  - num_alternatives=3 → width_multipliers = linspace(1.0, 1.5, 3) = [1.0, 1.25, 1.5]
  - 3 solutions with pairwise-distinct total_costs
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
from hydro_engine.solvers.intake import IntakeSolver


# ── Serialisation helpers ─────────────────────────────────────────────────────


def serialize_solution(solver, sol):
    """Serialize an intake solution (network + score) to a dict."""
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

    meta = network.metadata

    return {
        "network": {
            "name": network.name,
            "node_count": network.node_count,
            "pipe_count": network.pipe_count,
            "total_length": network.total_length,
            "nodes": nodes_data,
            "pipes": pipes_data,
            "metadata": {
                "design_flow_m3s": meta.get("design_flow_m3s"),
                "source_type": meta.get("source_type"),
                "screen": meta.get("screen"),
                "channel": meta.get("channel"),
                "weir": meta.get("weir"),
                "pipe_transition": meta.get("pipe_transition"),
            },
        },
        "score": {
            "total_cost": score.total_cost,
            "total_length": score.total_length,
            "total_excavation": score.total_excavation,
            "norm_violations": score.norm_violations,
            "pump_count": score.pump_count,
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
    """Primary intake fixture: single alternative, rectangular weir, CONCRETE."""
    design_flow = 0.050
    source_elevation = 120.0
    pipe_elevation = 118.0
    channel_slope = 0.005
    material = PipeMaterial.CONCRETE
    weir_type = 0  # rectangular
    num_alternatives = 1

    constraints = DesignConstraints()
    solver = IntakeSolver(None, constraints)

    solutions = solver.solve(
        design_flow=design_flow,
        source_type="river",
        source_elevation=source_elevation,
        pipe_elevation=pipe_elevation,
        channel_slope=channel_slope,
        material=material,
        num_alternatives=num_alternatives,
        channel_width_factor=1.0,
        channel_slope_factor=1.0,
        weir_type=weir_type,
        screen_velocity_factor=1.0,
    )

    sol = solutions[0]
    sol_data = serialize_solution(solver, sol)

    meta = sol_data["network"]["metadata"]
    screen = meta["screen"]
    channel = meta["channel"]
    weir = meta["weir"]
    pipe_trans = meta["pipe_transition"]

    print(f"Primary fixture (rectangular weir):")
    print(f"  rank              = {sol.rank}")
    print(f"  screen.net_width  = {screen.get('net_width')}")
    print(f"  screen.height     = {screen.get('height')}")
    print(f"  screen.velocity   = {screen.get('velocity_through_screen')}")
    print(f"  channel.depth_m   = {channel.get('depth_m')}")
    print(f"  channel.width_m   = {channel.get('width_m')}")
    print(f"  channel.length_m  = {channel.get('length_m')}")
    print(f"  channel.velocity  = {channel.get('velocity_m_s')}")
    print(f"  weir.crest_length = {weir.get('crest_length')}")
    print(f"  weir.head         = {weir.get('head')}")
    print(f"  pipe.diameter     = {pipe_trans.get('diameter')}")
    print(f"  pipe.velocity     = {pipe_trans.get('velocity_m_s')}")
    print(f"  total_length      = {sol_data['network']['total_length']:.6f}")
    print(f"  total_cost        = {sol_data['score']['total_cost']:.6f}")
    print(f"  violations        = {sol_data['score']['norm_violations']}")

    fixture = {
        "schema_version": 1,
        "fixture_name": "solvers_intake_golden",
        "description": (
            "IntakeSolver oracle fixture: design_flow=0.050 m3/s, "
            "source_elev=120.0, pipe_elev=118.0, channel_slope=0.005, "
            "material=CONCRETE, weir_type=0 (rectangular), num_alternatives=1. "
            "DesignConstraints() defaults. "
            "NOTE: SolutionScore.total_excavation is repurposed to carry "
            "concrete_volume + screen_concrete."
        ),
        "solver_params": {
            "design_flow": design_flow,
            "source_elevation": source_elevation,
            "pipe_elevation": pipe_elevation,
            "channel_slope": channel_slope,
            "material": "CONCRETE",
            "weir_type": weir_type,
            "num_alternatives": num_alternatives,
            "channel_width_factor": 1.0,
            "channel_slope_factor": 1.0,
            "screen_velocity_factor": 1.0,
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
            "w_concrete": 600.0,
            "w_pipe": 120.0,
            "w_violations": 3000.0,
            "expected_cost": sol_data["score"]["total_cost"],
        },
        "note_total_excavation_repurposed": (
            "SolutionScore.total_excavation = concrete_volume + screen_concrete (not real excavation)"
        ),
    }
    return fixture


# ── Fixture 2: alternatives + v_notch ────────────────────────────────────────


def make_alternatives_fixture():
    """Alternatives fixture: 3 alternatives with v_notch weir, pairwise-distinct costs."""
    design_flow = 0.030
    source_elevation = 115.0
    pipe_elevation = 113.0
    channel_slope = 0.005
    material = PipeMaterial.CONCRETE
    weir_type = 1  # v_notch
    num_alternatives = 3  # → width_multipliers = linspace(1.0, 1.5, 3) = [1.0, 1.25, 1.5]

    constraints = DesignConstraints()
    solver = IntakeSolver(None, constraints)

    solutions = solver.solve(
        design_flow=design_flow,
        source_type="river",
        source_elevation=source_elevation,
        pipe_elevation=pipe_elevation,
        channel_slope=channel_slope,
        material=material,
        num_alternatives=num_alternatives,
        channel_width_factor=1.0,
        channel_slope_factor=1.0,
        weir_type=weir_type,
        screen_velocity_factor=1.0,
    )

    print(f"\nAlternatives fixture (v_notch): {len(solutions)} solutions")
    costs = [sol.score.total_cost for sol in solutions]
    for i, (sol, cost) in enumerate(zip(solutions, costs)):
        meta = sol.network.metadata
        channel = meta.get("channel", {})
        weir = meta.get("weir", {})
        print(
            f"  Alt {i+1}: total_cost={cost:.6f}, "
            f"ch_width={channel.get('width_m'):.4f}, "
            f"ch_depth={channel.get('depth_m'):.4f}, "
            f"weir_head={weir.get('head')}, "
            f"rank={sol.rank}"
        )

    # Assert pairwise strictly distinct costs
    print("\nPairwise distinctness check:")
    for i in range(len(costs)):
        for j in range(i + 1, len(costs)):
            diff = abs(costs[i] - costs[j])
            if diff < 1e-3:
                raise AssertionError(
                    f"Intake alternatives costs[{i}]={costs[i]:.6f} and "
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
        screen = meta.get("screen", {})
        channel = meta.get("channel", {})
        weir = meta.get("weir", {})
        pipe_trans = meta.get("pipe_transition", {})
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
                "design_flow_m3s": meta.get("design_flow_m3s"),
                "source_type": meta.get("source_type"),
                "screen": screen,
                "channel": channel,
                "weir": weir,
                "pipe_transition": pipe_trans,
            },
        })

    return {
        "schema_version": 1,
        "fixture_name": "solvers_intake_alternatives",
        "description": (
            "IntakeSolver alternatives fixture: 3 alternatives with v_notch weir. "
            "design_flow=0.030 m3/s, source_elev=115.0, pipe_elev=113.0, "
            "channel_slope=0.005, material=CONCRETE, weir_type=1 (v_notch). "
            "width_multipliers = linspace(1.0, 1.5, 3) = [1.0, 1.25, 1.5]. "
            "Pairwise-distinct total_costs guarantee ordering assertions are non-vacuous."
        ),
        "solver_params": {
            "design_flow": design_flow,
            "source_elevation": source_elevation,
            "pipe_elevation": pipe_elevation,
            "channel_slope": channel_slope,
            "material": "CONCRETE",
            "weir_type": weir_type,
            "num_alternatives": num_alternatives,
            "channel_width_factor": 1.0,
            "channel_slope_factor": 1.0,
            "screen_velocity_factor": 1.0,
        },
        "num_alternatives_requested": num_alternatives,
        "num_solutions_returned": len(solutions),
        "solutions": alts_data,
        "distinct_costs": costs,
    }


# ── Main ──────────────────────────────────────────────────────────────────────


def main():
    print("Generating IntakeSolver oracle fixtures...")

    # Fixture 1: primary / golden
    print("\n=== Fixture 1: primary (num_alternatives=1, rectangular weir) ===")
    fixture1 = make_primary_fixture()
    out1 = FIXTURES_DIR / "solvers_intake_golden.json"
    with open(out1, "w") as f:
        json.dump(fixture1, f, indent=2)
    print(f"\nWritten: {out1}")

    # Fixture 2: alternatives
    print("\n=== Fixture 2: alternatives (num_alternatives=3, v_notch weir) ===")
    fixture2 = make_alternatives_fixture()
    out2 = FIXTURES_DIR / "solvers_intake_alternatives.json"
    with open(out2, "w") as f:
        json.dump(fixture2, f, indent=2)
    print(f"Written: {out2}")

    print("\nAll intake fixtures generated successfully.")


if __name__ == "__main__":
    main()
