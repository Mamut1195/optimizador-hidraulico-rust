"""
Oracle generator for norms_violations_golden.json.

Builds deterministic synthetic networks, validates them with the Python
NormValidator, and emits the fixture that Rust integration tests use to assert
EXACT parity (count-for-count) of hard_violation_count and compliant.

Run with the Python oracle engine on the PYTHONPATH:

    cd motor-optimizacion-hidraulico
    python ../optimizador-hidraulico-rust/tests/oracle/gen/generate_norms_violations.py

Output: tests/oracle/fixtures/norms_violations_golden.json
"""

from __future__ import annotations

import json
import math
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


# ── Case A: all_passing ───────────────────────────────────────────────────────

def build_network_a():
    """network_A ('all_passing'): 3 pipes + 2 nodes; all values within CONAGUA_MX SEWER bounds.

    CONAGUA_MX SEWER rules:
      min_slope=0.005, max_slope=0.05
      min_cover=1.2, max_cover -> max_depth=5.0
      min_velocity=0.6, max_velocity=5.0  (Manning: v=f(d,slope,n))
      min_diameter=0.2, max_diameter=1.2
    No nodes with pressure_mca => no node pressure violations.

    All pipes have slope=0.02 (within 0.005-0.05),
    diameter=0.3m (within 0.2-1.2),
    cover = z - start_invert = 10.0 - 8.5 = 1.5m (within 1.2-5.0).
    Manning velocity at d=0.3, s=0.02, n=0.009:
      V = (1/0.009)*(0.075)^(2/3)*sqrt(0.02) ≈ 2.76 m/s — within [0.6, 5.0].
    """
    net = PipeNetwork("network_A")
    net.add_node(make_node("n1", 0, 0, 10.0))
    net.add_node(make_node("n2", 10, 0, 10.0))
    for i in range(3):
        net.add_pipe(make_pipe(f"p{i+1}", "n1", "n2", 100.0, 0.3, slope=0.02, roughness=0.009))
    return net


# ── Case B: slope_violations_with_bypass ─────────────────────────────────────

def build_network_b():
    """network_B ('slope_violations_with_bypass'): 3 pipes.

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


# ── Case C: pressure_violations ───────────────────────────────────────────────

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
    flow = math.pi * (0.15) ** 2 * 0.5  # flow giving v=0.5 m/s
    for i in range(2):
        p = make_pipe(f"p{i+1}", "n1", "n2", 100.0, 0.3, slope=0.01, roughness=0.009)
        p.metadata["flow_m3s"] = flow
        net.add_pipe(p)
    return net


# ── Case D: max_slope_violation ───────────────────────────────────────────────

def build_network_d():
    """network_D ('max_slope_violation'): 1 pipe slope=0.08 > max_slope=0.05.

    CONAGUA_MX SEWER max_slope=0.05.
    Manning velocity at d=0.3, s=0.08, n=0.009:
      V = (1/0.009)*(0.075)^(2/3)*sqrt(0.08)
        = 111.11 * 0.17568 * 0.28284 ≈ 5.52 m/s > max_velocity=5.0
    So this network has 2 violations: max_slope and max_velocity on the same pipe.
    Python oracle stamps the exact count — we don't guess.

    start_invert=8.5, z=10.0 → cover=1.5m (within min_cover=1.2, max_depth=5.0).
    diameter=0.3 (within min_diameter=0.2, max_diameter=1.2).
    """
    net = PipeNetwork("network_D")
    net.add_node(make_node("n1", 0, 0, 10.0))
    net.add_node(make_node("n2", 10, 0, 10.0))
    net.add_pipe(make_pipe("p1", "n1", "n2", 100.0, 0.3, slope=0.08, roughness=0.009))
    return net


# ── Case E: min_cover_violation ───────────────────────────────────────────────

def build_network_e():
    """network_E ('min_cover_violation'): 1 pipe with shallow burial → cover < 1.2m.

    CONAGUA_MX SEWER min_cover=1.2m.
    Node z=10.0, start_invert=9.5 → start_cover=10.0-9.5=0.5m < 1.2 → violation.
    end_invert = 9.5 - 0.02*100 = 7.5 → end_cover = 10.0-7.5 = 2.5m
    min(0.5, 2.5) = 0.5m < 1.2 → 1 violation.

    Note: slope=0.02 → within [0.005, 0.05].
    Manning velocity at d=0.3, s=0.02: V≈2.76 m/s (within [0.6, 5.0]).
    diameter=0.3 (within [0.2, 1.2]).
    The start_invert=9.5 is serialized explicitly so Rust side reproduces it.
    """
    net = PipeNetwork("network_E")
    net.add_node(make_node("n1", 0, 0, 10.0))
    net.add_node(make_node("n2", 10, 0, 10.0))
    # start_invert=9.5 → cover at start = 10.0 - 9.5 = 0.5m < 1.2 → violation
    net.add_pipe(make_pipe("p1", "n1", "n2", 100.0, 0.3, slope=0.02,
                           roughness=0.009, start_invert=9.5))
    return net


# ── Case F: max_depth_violation ───────────────────────────────────────────────

def build_network_f():
    """network_F ('max_depth_violation'): 1 pipe buried deep → cover > 5.0m.

    CONAGUA_MX SEWER max_depth=5.0m (i.e., min cover is check with max_depth rule
    checking that min(covers) <= 5.0 would be wrong — the rule key is max_depth
    and the check uses the SAME cover value: actual > max_depth=5.0 → violation).

    Node z=10.0, start_invert=4.5 → start_cover=10.0-4.5=5.5m > 5.0 → violation.
    end_invert = 4.5 - 0.02*100 = 2.5 → end_cover = 10.0-2.5 = 7.5m
    min(5.5, 7.5) = 5.5m > 5.0 → 1 violation on max_depth.

    slope=0.02 → within slope bounds.
    Manning velocity at d=0.3, s=0.02: V≈2.76 m/s (within velocity bounds).
    diameter=0.3 (within diameter bounds).
    The start_invert=4.5 is serialized explicitly so Rust side reproduces it.
    """
    net = PipeNetwork("network_F")
    net.add_node(make_node("n1", 0, 0, 10.0))
    net.add_node(make_node("n2", 10, 0, 10.0))
    # start_invert=4.5 → cover at start = 10.0 - 4.5 = 5.5m > 5.0 → violation
    net.add_pipe(make_pipe("p1", "n1", "n2", 100.0, 0.3, slope=0.02,
                           roughness=0.009, start_invert=4.5))
    return net


# ── Case G: gravity_velocity_too_low ─────────────────────────────────────────

def build_network_g():
    """network_G ('gravity_velocity_too_low'): Manning velocity below min_velocity=0.6.

    CONAGUA_MX SEWER min_velocity=0.6 m/s.
    Manning: V = (1/n)*(D/4)^(2/3)*sqrt(S)
    d=0.2, s=0.001, n=0.009:
      V = (1/0.009)*(0.05)^(2/3)*sqrt(0.001)
        = 111.11 * 0.13572 * 0.031623
        ≈ 0.477 m/s < 0.6 → violation.

    slope=0.001 < min_slope=0.005 → also a slope violation on same pipe.
    diameter=0.2 = min_diameter → no diameter violation (boundary).
    start_invert=8.5, z=10.0 → cover=1.5m (within bounds).
    Python oracle stamps the exact count for both violations.
    """
    net = PipeNetwork("network_G")
    net.add_node(make_node("n1", 0, 0, 10.0))
    net.add_node(make_node("n2", 10, 0, 10.0))
    # d=0.2, s=0.001 → V≈0.477 m/s < 0.6 AND slope=0.001 < 0.005
    net.add_pipe(make_pipe("p1", "n1", "n2", 100.0, 0.2, slope=0.001, roughness=0.009))
    return net


# ── Case H: gravity_velocity_too_high ────────────────────────────────────────

def build_network_h():
    """network_H ('gravity_velocity_too_high'): Manning velocity above max_velocity=5.0.

    CONAGUA_MX SEWER max_velocity=5.0 m/s.
    Manning: V = (1/n)*(D/4)^(2/3)*sqrt(S)
    d=0.3, s=0.08, n=0.009:
      V = (1/0.009)*(0.075)^(2/3)*sqrt(0.08)
        = 111.11 * 0.17568 * 0.28284
        ≈ 5.52 m/s > 5.0 → velocity violation.
    slope=0.08 > max_slope=0.05 → also a slope violation.
    Python oracle stamps the exact count.

    Note: this is the same physics as network_D — that case is labeled
    'max_slope_violation' (primarily), while this one is 'gravity_velocity_too_high'
    (primarily). Both trigger slope+velocity violations simultaneously. The oracle
    count will be identical; including both as distinct labelled cases for coverage
    of the velocity branch in a clear, intentional way.

    IMPORTANT: To isolate ONLY the velocity branch without the slope violation, we
    need a slope within [0.005, 0.05] that still produces V > 5.0. Let's verify:
    s=0.05 (max allowed): V = 111.11*(0.075)^(2/3)*sqrt(0.05)
      = 111.11 * 0.17568 * 0.22361 ≈ 4.36 m/s < 5.0 — NOT a velocity violation.
    So at max_slope there is NO velocity violation for d=0.3.
    For d=0.2, s=0.05: V = 111.11*(0.05)^(2/3)*sqrt(0.05)
      = 111.11 * 0.13572 * 0.22361 ≈ 3.37 m/s < 5.0 — still OK.
    Conclusion: For CONAGUA_MX SEWER to get max_velocity violation, slope must exceed
    max_slope. We cannot isolate velocity-only without a custom norm. The oracle will
    count both violations (max_slope + max_velocity). This is the honest ground truth.
    """
    net = PipeNetwork("network_H")
    net.add_node(make_node("n1", 0, 0, 10.0))
    net.add_node(make_node("n2", 10, 0, 10.0))
    # d=0.3, s=0.08 → V≈5.52 m/s > 5.0 AND slope=0.08 > 0.05
    net.add_pipe(make_pipe("p1", "n1", "n2", 100.0, 0.3, slope=0.08, roughness=0.009))
    return net


# ── Case I: pressure_velocity_too_low ────────────────────────────────────────

def build_network_i():
    """network_I ('pressure_velocity_too_low'): HW velocity below min_velocity=0.3 (EPA_US WS).

    EPA_US WATER_SUPPLY min_velocity=0.3 m/s.
    HW velocity: V = Q / (pi*(D/2)^2)
    d=0.3, Q=0.001 m3/s → V = 0.001 / (pi*0.0225) = 0.001/0.070686 ≈ 0.01414 m/s < 0.3.

    Also need to pass cover, diameter, and pressure rules so ONLY velocity fires.
    Nodes: no pressure_mca → no pressure violation.
    start_invert=8.5, z=10.0 → cover=1.5m > min_cover=1.07m (EPA_US WS) ✓ and < 3.0m ✓.
    diameter=0.3 within [0.102, 1.2] ✓.
    slope=0.01 — within any slope bounds.
    """
    net = PipeNetwork("network_I")
    net.add_node(make_node("n1", 0, 0, 10.0, node_type=NodeType.JUNCTION))
    net.add_node(make_node("n2", 10, 0, 10.0, node_type=NodeType.JUNCTION))
    # flow_m3s=0.001 at d=0.3 → V≈0.014 m/s < 0.3 → violation
    p = make_pipe("p1", "n1", "n2", 100.0, 0.3, slope=0.01, roughness=0.009)
    p.metadata["flow_m3s"] = 0.001
    net.add_pipe(p)
    return net


# ── Case J: pressure_velocity_too_high ───────────────────────────────────────

def build_network_j():
    """network_J ('pressure_velocity_too_high'): HW velocity above max_velocity=3.0 (EPA_US WS).

    EPA_US WATER_SUPPLY max_velocity=3.0 m/s.
    HW velocity: V = Q / (pi*(D/2)^2)
    d=0.1, Q=0.05 m3/s → V = 0.05 / (pi*0.0025) = 0.05/0.007854 ≈ 6.366 m/s > 3.0.

    Nodes: no pressure_mca → no pressure violation.
    start_invert=8.5, z=10.0 → cover=1.5m > min_cover=1.07m ✓ and < 3.0m ✓.
    diameter=0.1 within [0.102, 1.2]? 0.1 < 0.102 → also a min_diameter violation!
    Use d=0.15 instead: V = 0.05 / (pi*0.005625) = 0.05/0.017671 ≈ 2.829 m/s < 3.0 — not enough.
    Use d=0.12: V = 0.05 / (pi*(0.06)^2) = 0.05/0.011310 ≈ 4.42 m/s > 3.0 ✓.
    d=0.12 > 0.102 ✓. Oracle stamps exact count.
    """
    net = PipeNetwork("network_J")
    net.add_node(make_node("n1", 0, 0, 10.0, node_type=NodeType.JUNCTION))
    net.add_node(make_node("n2", 10, 0, 10.0, node_type=NodeType.JUNCTION))
    # d=0.12, Q=0.05 → V≈4.42 m/s > 3.0 → velocity violation only
    p = make_pipe("p1", "n1", "n2", 100.0, 0.12, slope=0.01, roughness=0.009)
    p.metadata["flow_m3s"] = 0.05
    net.add_pipe(p)
    return net


# ── Case K: diameter_too_small ────────────────────────────────────────────────

def build_network_k():
    """network_K ('diameter_too_small'): pipe diameter < min_diameter=0.2 (CONAGUA_MX SEWER).

    CONAGUA_MX SEWER min_diameter=0.2m.
    Use d=0.15 < 0.2 → violation.

    Manning velocity at d=0.15, s=0.02, n=0.009:
      V = (1/0.009)*(0.0375)^(2/3)*sqrt(0.02)
        = 111.11 * 0.11080 * 0.14142 ≈ 1.74 m/s (within [0.6, 5.0]).
    slope=0.02 (within [0.005, 0.05]).
    start_invert=8.5, z=10.0 → cover=1.5m (within bounds).
    Only min_diameter should fire. Oracle stamps exact count.
    """
    net = PipeNetwork("network_K")
    net.add_node(make_node("n1", 0, 0, 10.0))
    net.add_node(make_node("n2", 10, 0, 10.0))
    # d=0.15 < 0.2 → min_diameter violation
    net.add_pipe(make_pipe("p1", "n1", "n2", 100.0, 0.15, slope=0.02, roughness=0.009))
    return net


# ── Case L: diameter_too_large ────────────────────────────────────────────────

def build_network_l():
    """network_L ('diameter_too_large'): pipe diameter > max_diameter=1.2 (CONAGUA_MX SEWER).

    CONAGUA_MX SEWER max_diameter=1.2m.
    Use d=1.5 > 1.2 → violation.

    Manning velocity at d=1.5, s=0.02, n=0.009:
      V = (1/0.009)*(0.375)^(2/3)*sqrt(0.02)
        = 111.11 * 0.52915 * 0.14142 ≈ 8.32 m/s > max_velocity=5.0 → also violation.
    slope=0.02 (within bounds).
    start_invert=8.5, z=10.0 → cover=1.5m (within bounds).
    Oracle stamps the exact count (likely 2: max_diameter + max_velocity).
    """
    net = PipeNetwork("network_L")
    net.add_node(make_node("n1", 0, 0, 10.0))
    net.add_node(make_node("n2", 10, 0, 10.0))
    # d=1.5 > 1.2 → max_diameter violation
    net.add_pipe(make_pipe("p1", "n1", "n2", 100.0, 1.5, slope=0.02, roughness=0.009))
    return net


# ── Fixture serializer ────────────────────────────────────────────────────────

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
            # Always emit start_invert so the Rust side can reconstruct exactly.
            "start_invert": pipe.start_invert,
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

    # Case D: max_slope violation (CONAGUA_MX SEWER)
    net_d = build_network_d()
    case_d = validate_and_emit(net_d, "CONAGUA_MX", ProjectType.SEWER, "max_slope_violation")
    cases.append(case_d)
    print(f"Case D — {case_d['label']}: hard={case_d['expected_hard_count']}, compliant={case_d['expected_compliant']}")

    # Case E: min_cover violation (CONAGUA_MX SEWER)
    net_e = build_network_e()
    case_e = validate_and_emit(net_e, "CONAGUA_MX", ProjectType.SEWER, "min_cover_violation")
    cases.append(case_e)
    print(f"Case E — {case_e['label']}: hard={case_e['expected_hard_count']}, compliant={case_e['expected_compliant']}")

    # Case F: max_depth violation (CONAGUA_MX SEWER)
    net_f = build_network_f()
    case_f = validate_and_emit(net_f, "CONAGUA_MX", ProjectType.SEWER, "max_depth_violation")
    cases.append(case_f)
    print(f"Case F — {case_f['label']}: hard={case_f['expected_hard_count']}, compliant={case_f['expected_compliant']}")

    # Case G: gravity Manning velocity too low (CONAGUA_MX SEWER)
    net_g = build_network_g()
    case_g = validate_and_emit(net_g, "CONAGUA_MX", ProjectType.SEWER, "gravity_velocity_too_low")
    cases.append(case_g)
    print(f"Case G — {case_g['label']}: hard={case_g['expected_hard_count']}, compliant={case_g['expected_compliant']}")

    # Case H: gravity Manning velocity too high (CONAGUA_MX SEWER)
    net_h = build_network_h()
    case_h = validate_and_emit(net_h, "CONAGUA_MX", ProjectType.SEWER, "gravity_velocity_too_high")
    cases.append(case_h)
    print(f"Case H — {case_h['label']}: hard={case_h['expected_hard_count']}, compliant={case_h['expected_compliant']}")

    # Case I: pressure HW velocity too low (EPA_US WATER_SUPPLY)
    net_i = build_network_i()
    case_i = validate_and_emit(net_i, "EPA_US", ProjectType.WATER_SUPPLY, "pressure_velocity_too_low")
    cases.append(case_i)
    print(f"Case I — {case_i['label']}: hard={case_i['expected_hard_count']}, compliant={case_i['expected_compliant']}")

    # Case J: pressure HW velocity too high (EPA_US WATER_SUPPLY)
    net_j = build_network_j()
    case_j = validate_and_emit(net_j, "EPA_US", ProjectType.WATER_SUPPLY, "pressure_velocity_too_high")
    cases.append(case_j)
    print(f"Case J — {case_j['label']}: hard={case_j['expected_hard_count']}, compliant={case_j['expected_compliant']}")

    # Case K: diameter too small (CONAGUA_MX SEWER)
    net_k = build_network_k()
    case_k = validate_and_emit(net_k, "CONAGUA_MX", ProjectType.SEWER, "diameter_too_small")
    cases.append(case_k)
    print(f"Case K — {case_k['label']}: hard={case_k['expected_hard_count']}, compliant={case_k['expected_compliant']}")

    # Case L: diameter too large (CONAGUA_MX SEWER)
    net_l = build_network_l()
    case_l = validate_and_emit(net_l, "CONAGUA_MX", ProjectType.SEWER, "diameter_too_large")
    cases.append(case_l)
    print(f"Case L — {case_l['label']}: hard={case_l['expected_hard_count']}, compliant={case_l['expected_compliant']}")

    payload = {"cases": cases}
    OUTPUT.write_text(json.dumps(payload, indent=2, ensure_ascii=False), encoding="utf-8")
    print(f"\nWritten {len(cases)} cases to {OUTPUT}")


if __name__ == "__main__":
    main()
