"""
Oracle generator for norms_violations_golden.json.

Builds 3 deterministic synthetic networks, validates them with the Python
NormValidator, and emits the fixture that Rust integration tests use to assert
EXACT parity (count-for-count) of hard_violation_count and compliant.

Run with the Python oracle engine on the PYTHONPATH:

    cd motor-optimizacion-hidraulico
    python ../optimizador-hidraulico-rust/tests/oracle/gen/generate_norms_violations.py

Output: tests/oracle/fixtures/norms_violations_golden.json
"""

from __future__ import annotations

import json
import pathlib
import sys

ORACLE_ROOT = pathlib.Path(__file__).parent.parent.parent.parent.parent / "motor-optimizacion-hidraulico"
if str(ORACLE_ROOT) not in sys.path:
    sys.path.insert(0, str(ORACLE_ROOT / "src"))

from hydro_engine.core.network import PipeNetwork
from hydro_engine.core.node import NetworkNode
from hydro_engine.core.pipe import NetworkPipe
from hydro_engine.core.types import NodeType, ProjectType
from hydro_engine.norms.profile import NormRegistry
from hydro_engine.norms.validator import NormValidator
from hydro_engine.solvers.base import Solution, SolutionScore

FIXTURES = pathlib.Path(__file__).parent.parent / "fixtures"
FIXTURES.mkdir(exist_ok=True)
OUTPUT = FIXTURES / "norms_violations_golden.json"


def make_node(id, x, y, z, node_type=NodeType.MANHOLE, **meta):
    node = NetworkNode(id=id, x=x, y=y, z=z, node_type=node_type)
    node.metadata.update(meta)
    return node


def make_pipe(id, start, end, length, diameter, slope, roughness=0.009, design_flow=0.001,
              start_invert=8.5, **meta):
    """Make a pipe with explicit start_invert; end_invert computed from slope.

    Default start_invert=8.5 gives cover=1.5m when node z=10.0 (satisfies
    CONAGUA_MX min_cover=1.2m and max_depth=5.0m).
    """
    end_invert = start_invert - slope * length
    pipe = NetworkPipe(
        id=id,
        start_node_id=start,
        end_node_id=end,
        length=length,
        diameter=diameter,
        start_invert=start_invert,
        end_invert=end_invert,
    )
    pipe.roughness = roughness
    pipe.design_flow = design_flow
    pipe.metadata.update(meta)
    return pipe


def build_network_a():
    """network_A ('all_passing'): 3 pipes + 2 nodes; all values within CONAGUA_MX SEWER bounds.

    CONAGUA_MX SEWER rules:
      min_slope=0.005, max_slope=0.05
      min_cover=1.2, max_cover -> max_depth=5.0
      min_velocity=0.6, max_velocity=5.0  (Manning: v=f(d,slope,n))
      min_diameter=0.2, max_diameter=1.2
    No nodes with pressure_mca => no node pressure violations.

    All pipes have slope=0.01 (within 0.005-0.05),
    diameter=0.3m (within 0.2-1.2),
    cover = z - start_invert = 10.0 - 8.5 = 1.5m (within 1.2-5.0).
    Manning velocity at d=0.3, s=0.01, n=0.009: v = (1/0.009)*(0.3/4)^(2/3)*sqrt(0.01)
    ≈ 111.1 * 0.0412 * 0.1 ≈ 0.458 m/s ... just below min_velocity=0.6.
    Let's use slope=0.02: v = (1/0.009)*(0.075)^(2/3)*sqrt(0.02) ≈ 0.648 m/s — OK.
    """
    net = PipeNetwork("network_A")
    net.add_node(make_node("n1", 0, 0, 10.0))
    net.add_node(make_node("n2", 10, 0, 10.0))
    for i in range(3):
        net.add_pipe(make_pipe(f"p{i+1}", "n1", "n2", 100.0, 0.3, slope=0.02, roughness=0.009))
    return net


def build_network_b():
    """network_B ('slope_violations'): 3 pipes.

    2 pipes with slope=0.001 (below CONAGUA_MX SEWER min_slope=0.005) → hard violations.
    1 pipe with slope=0.001 BUT bypassed_by_pump=True → skip → NOT counted.

    Expected: hard_violation_count=2, compliant=False.
    """
    net = PipeNetwork("network_B")
    net.add_node(make_node("n1", 0, 0, 10.0))
    net.add_node(make_node("n2", 10, 0, 10.0))
    # 2 violating pipes (slope below min_slope=0.005)
    p1 = make_pipe("p1", "n1", "n2", 100.0, 0.3, slope=0.001, roughness=0.009)
    p2 = make_pipe("p2", "n1", "n2", 100.0, 0.3, slope=0.001, roughness=0.009)
    # 1 bypassed pipe — slope violation skipped
    p3 = make_pipe("p3", "n1", "n2", 100.0, 0.3, slope=0.001, roughness=0.009, bypassed_by_pump=True)
    net.add_pipe(p1)
    net.add_pipe(p2)
    net.add_pipe(p3)
    return net


def build_network_c():
    """network_C ('pressure_violations'): 2 nodes with pressure_mca=8.0.

    EPA_US WATER_SUPPLY min_pressure=14.1 mca → both nodes violate.
    Pipes are all within EPA_US WATER_SUPPLY pipe bounds (no pipe violations).

    Expected: hard_violation_count=2, compliant=False.

    EPA_US WATER_SUPPLY pipe rules:
      min_cover=1.07, max_depth=3.0
      min_velocity=0.3, max_velocity=3.0
      min_diameter=0.102, max_diameter=1.2
      min_pressure (node)=14.1, max_pressure (node)=56.2
    """
    net = PipeNetwork("network_C")
    n1 = make_node("n1", 0, 0, 10.0, node_type=NodeType.JUNCTION)
    n1.metadata["pressure_mca"] = 8.0  # below 14.1 → hard violation
    n2 = make_node("n2", 10, 0, 10.0, node_type=NodeType.JUNCTION)
    n2.metadata["pressure_mca"] = 8.0  # below 14.1 → hard violation
    # Reservoir node → should be skipped by validator
    n3 = make_node("n3", 20, 0, 10.0, node_type=NodeType.RESERVOIR)
    n3.metadata["pressure_mca"] = 8.0  # skipped because RESERVOIR
    net.add_node(n1)
    net.add_node(n2)
    net.add_node(n3)
    # Pipes: within EPA_US WATER_SUPPLY pipe rules
    # Need flow_m3s for velocity check (pressure type). Use high enough flow to get v within [0.3, 3.0].
    # d=0.3m, flow = pi*(0.15)^2 * 0.5 ≈ 0.0353 m3/s → v = 0.5 m/s (OK)
    import math
    flow = math.pi * (0.15) ** 2 * 0.5  # flow giving v=0.5 m/s
    for i in range(2):
        p = make_pipe(f"p{i+1}", "n1", "n2", 100.0, 0.3, slope=0.01, roughness=0.009)
        p.metadata["flow_m3s"] = flow
        net.add_pipe(p)
    return net


def validate_and_emit(net: PipeNetwork, norm_code: str, project_type: ProjectType, label: str) -> dict:
    profile = NormRegistry.get(norm_code)
    validator = NormValidator(profile)
    # Build a minimal Solution wrapper
    score = SolutionScore()
    solution = Solution(network=net, score=score)
    result = validator.validate_solution(solution, project_type)

    # Serialize the network for the fixture
    nodes = []
    for node in net.nodes:
        nodes.append({
            "id": node.id,
            "x": node.x,
            "y": node.y,
            "z": node.z,
            "node_type": node.node_type.name,
            "metadata": node.metadata,
        })
    pipes = []
    for pipe in net.pipes:
        pipes.append({
            "id": pipe.id,
            "start_node_id": pipe.start_node_id,
            "end_node_id": pipe.end_node_id,
            "length": pipe.length,
            "diameter": pipe.diameter,
            "slope": pipe.slope,
            "roughness": pipe.roughness,
            "design_flow": pipe.design_flow,
            "metadata": pipe.metadata,
        })

    return {
        "norm_code": norm_code,
        "project_type": project_type.name,
        "label": label,
        "network": {
            "name": net.name,
            "nodes": nodes,
            "pipes": pipes,
        },
        "expected_hard_count": result.hard_violation_count,
        "expected_compliant": result.compliant,
    }


def main() -> None:
    cases = []

    # Case A: all passing (CONAGUA_MX SEWER)
    net_a = build_network_a()
    case_a = validate_and_emit(net_a, "CONAGUA_MX", ProjectType.SEWER, "all_passing")
    cases.append(case_a)
    print(f"Case A — {case_a['label']}: hard={case_a['expected_hard_count']}, compliant={case_a['expected_compliant']}")

    # Case B: slope violations with pump bypass (CONAGUA_MX SEWER)
    net_b = build_network_b()
    case_b = validate_and_emit(net_b, "CONAGUA_MX", ProjectType.SEWER, "slope_violations_with_bypass")
    cases.append(case_b)
    print(f"Case B — {case_b['label']}: hard={case_b['expected_hard_count']}, compliant={case_b['expected_compliant']}")

    # Case C: pressure violations (EPA_US WATER_SUPPLY)
    net_c = build_network_c()
    case_c = validate_and_emit(net_c, "EPA_US", ProjectType.WATER_SUPPLY, "pressure_violations")
    cases.append(case_c)
    print(f"Case C — {case_c['label']}: hard={case_c['expected_hard_count']}, compliant={case_c['expected_compliant']}")

    payload = {"cases": cases}
    OUTPUT.write_text(json.dumps(payload, indent=2, ensure_ascii=False), encoding="utf-8")
    print(f"\nWritten {len(cases)} cases to {OUTPUT}")


if __name__ == "__main__":
    main()
