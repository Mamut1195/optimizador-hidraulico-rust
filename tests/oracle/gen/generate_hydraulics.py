"""Generate golden fixture vectors for hydraulics parity tests.

Imports the Python ``hydro_engine`` oracle and emits deterministic test
vectors into ``tests/oracle/fixtures/hydraulics_golden.json``.

Usage
-----
Run from the *workspace root* with the oracle engine on PYTHONPATH:

    python tests/oracle/gen/generate_hydraulics.py

Or via the helper in ``tests/oracle/README.md``:

    PYTHONPATH=<oracle-src> python tests/oracle/gen/generate_hydraulics.py

The fixture is committed to the repository.  Regeneration is a *deliberate*
act — do not regenerate unless the Python oracle itself changed and you
intend to update the Rust parity targets.

Lock-in policy
--------------
Once generated and committed, fixture values are treated as ground truth for
parity assertions.  A diff in the fixture always means either (a) the Python
oracle changed intentionally, or (b) an unintended change crept in.  Both
cases require explicit review.
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

# ---------------------------------------------------------------------------
# PYTHONPATH setup — resolve the oracle engine source directory
# ---------------------------------------------------------------------------
ORACLE_SRC = Path(__file__).resolve().parents[4] / "motor-optimizacion-hidraulico" / "src"
if ORACLE_SRC.exists() and str(ORACLE_SRC) not in sys.path:
    sys.path.insert(0, str(ORACLE_SRC))

# ---------------------------------------------------------------------------
# Imports from the Python oracle
# ---------------------------------------------------------------------------
try:
    import numpy as np
    from hydro_engine.hydraulics.darcy_weisbach import friction_factor, head_loss_dw
    from hydro_engine.hydraulics.hazen_williams import head_loss as hw_head_loss
    from hydro_engine.hydraulics.hazen_williams import velocity as hw_velocity
    from hydro_engine.hydraulics.manning import (
        full_flow_capacity,
        full_flow_velocity,
        partial_flow_ratio,
    )
    from hydro_engine.hydraulics.open_channel import rectangular_channel_flow
    from hydro_engine.hydraulics.pump_curves import PumpCurve, head_at_flow, make_curve
except ImportError as exc:
    sys.exit(
        f"ERROR: Cannot import hydro_engine.  "
        f"Set PYTHONPATH to the oracle src directory.\n"
        f"Tried: {ORACLE_SRC}\n"
        f"Detail: {exc}"
    )

# ---------------------------------------------------------------------------
# Output directory
# ---------------------------------------------------------------------------
FIXTURES_DIR = Path(__file__).resolve().parents[1] / "fixtures"
FIXTURES_DIR.mkdir(parents=True, exist_ok=True)
OUTPUT_FILE = FIXTURES_DIR / "hydraulics_golden.json"

# ---------------------------------------------------------------------------
# Deterministic input grids (seeded — do NOT use random; values must be
# reproducible across Python versions and platforms)
# ---------------------------------------------------------------------------

# Darcy-Weisbach: friction factor via Swamee-Jain
# Grid: (reynolds, roughness_mm, diameter_m)
DW_REYNOLDS = [5_000, 10_000, 50_000, 100_000, 500_000, 1_000_000, 5_000_000, 10_000_000]
DW_ROUGHNESS_MM = [0.0015, 0.05, 0.5]  # smooth PVC, steel, concrete
DW_DIAMETERS_M = [0.2, 0.4, 0.8]


def generate_darcy_weisbach_rows() -> list[dict]:
    rows: list[dict] = []
    for re in DW_REYNOLDS:
        for roughness in DW_ROUGHNESS_MM:
            for d in DW_DIAMETERS_M:
                ff = float(friction_factor(re, roughness=roughness, diameter=d))
                rows.append(
                    {
                        "reynolds": float(re),
                        "roughness_mm": float(roughness),
                        "diameter_m": float(d),
                        "friction_factor": ff,
                    }
                )
    return rows


# Darcy-Weisbach: head loss
# Grid: (velocity_m_s, diameter_m, length_m, roughness_mm)
DW_VELOCITIES = [0.5, 1.0, 2.0, 3.0, 5.0]
DW_HL_DIAMETERS = [0.2, 0.4, 0.6]
DW_LENGTHS = [100.0, 500.0]
DW_HL_ROUGHNESS = [0.0015, 0.5]


def generate_head_loss_dw_rows() -> list[dict]:
    rows: list[dict] = []
    for v in DW_VELOCITIES:
        for d in DW_HL_DIAMETERS:
            for length in DW_LENGTHS:
                for roughness in DW_HL_ROUGHNESS:
                    hl = float(head_loss_dw(v, diameter=d, length=length, roughness=roughness))
                    rows.append(
                        {
                            "velocity_m_s": float(v),
                            "diameter_m": float(d),
                            "length_m": float(length),
                            "roughness_mm": float(roughness),
                            "head_loss_m": hl,
                        }
                    )
    return rows


# Hazen-Williams
# Grid: (flow_m3s, diameter_m, length_m, c)
HW_FLOWS = [0.002, 0.005, 0.01, 0.02, 0.05, 0.1]
HW_DIAMETERS = [0.1, 0.2, 0.3, 0.5]
HW_LENGTHS = [100.0, 500.0]
HW_C_VALUES = [100.0, 130.0, 150.0]


def generate_hazen_williams_rows() -> list[dict]:
    rows: list[dict] = []
    for q in HW_FLOWS:
        for d in HW_DIAMETERS:
            for length in HW_LENGTHS:
                for c in HW_C_VALUES:
                    hl = float(hw_head_loss(q, d, length, c=c))
                    v = float(hw_velocity(q, d))
                    rows.append(
                        {
                            "flow_m3s": float(q),
                            "diameter_m": float(d),
                            "length_m": float(length),
                            "c": float(c),
                            "head_loss_m": hl,
                            "velocity_m_s": v,
                        }
                    )
    return rows


# Manning full pipe
# Grid: (diameter_m, slope, roughness_n)
MANNING_DIAMETERS = [0.2, 0.3, 0.45, 0.61, 0.76, 0.91]
MANNING_SLOPES = [0.002, 0.005, 0.01, 0.02, 0.05]
MANNING_ROUGHNESS = [0.009, 0.011, 0.013]


def generate_manning_full_pipe_rows() -> list[dict]:
    rows: list[dict] = []
    for d in MANNING_DIAMETERS:
        for s in MANNING_SLOPES:
            for n in MANNING_ROUGHNESS:
                v = float(full_flow_velocity(d, s, roughness=n))
                q = float(full_flow_capacity(d, s, roughness=n))
                rows.append(
                    {
                        "diameter_m": float(d),
                        "slope": float(s),
                        "roughness_n": float(n),
                        "velocity_m_s": v,
                        "capacity_m3s": q,
                    }
                )
    return rows


# Manning partial flow ratios
# Grid: y/D from 0.1 to 0.9 in steps, plus a few key values
PARTIAL_YD = [0.1, 0.2, 0.25, 0.3, 0.4, 0.5, 0.6, 0.7, 0.75, 0.8, 0.9]


def generate_manning_partial_flow_rows() -> list[dict]:
    rows: list[dict] = []
    yd_arr = np.array(PARTIAL_YD, dtype=np.float64)
    area_ratios, radius_ratios, flow_ratios = partial_flow_ratio(yd_arr)
    for i, yd in enumerate(PARTIAL_YD):
        rows.append(
            {
                "y_over_d": float(yd),
                "area_ratio": float(area_ratios[i]),
                "radius_ratio": float(radius_ratios[i]),
                "flow_ratio": float(flow_ratios[i]),
            }
        )
    return rows


# Rectangular open channel
# Grid: (width_m, depth_m, slope, roughness_n)
RECT_WIDTHS = [0.5, 1.0, 2.0]
RECT_DEPTHS = [0.3, 0.5, 1.0, 1.5]
RECT_SLOPES = [0.001, 0.005, 0.01]
RECT_ROUGHNESS = [0.013, 0.025]


def generate_rectangular_channel_rows() -> list[dict]:
    rows: list[dict] = []
    for w in RECT_WIDTHS:
        for depth in RECT_DEPTHS:
            for s in RECT_SLOPES:
                for n in RECT_ROUGHNESS:
                    result = rectangular_channel_flow(w, depth, s, roughness=n)
                    rows.append(
                        {
                            "width_m": float(w),
                            "depth_m": float(depth),
                            "slope": float(s),
                            "roughness_n": float(n),
                            "velocity_m_s": float(result["velocity_m_s"]),
                            "flow_m3s": float(result["flow_m3_s"]),
                        }
                    )
    return rows


# Pump curves: head at flow
# Grid: (shutoff_head_m, q_bep_m3s, query_flow_m3s)
PUMP_SHUTOFF_HEADS = [10.0, 25.0, 40.0, 65.0, 80.0]
PUMP_Q_BEPS = [0.005, 0.02, 0.08, 0.25, 0.45]
PUMP_QUERY_FRACTIONS = [0.0, 0.3, 0.6, 0.9, 1.1]  # fraction of q_max = 1.5 * q_bep


def generate_pump_curve_rows() -> list[dict]:
    rows: list[dict] = []
    for h0, q_bep in zip(PUMP_SHUTOFF_HEADS, PUMP_Q_BEPS):
        curve: PumpCurve = make_curve(h0, q_bep)
        q_max = curve.q_max
        for frac in PUMP_QUERY_FRACTIONS:
            q_query = frac * q_max
            h = float(head_at_flow(curve, q_query))
            rows.append(
                {
                    "shutoff_head_m": float(h0),
                    "q_bep_m3s": float(q_bep),
                    "query_flow_m3s": float(q_query),
                    "expected_head_m": h,
                }
            )
    return rows


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------


def main() -> None:
    print("Generating hydraulics golden fixture...")

    fixture = {
        "darcy_weisbach": generate_darcy_weisbach_rows(),
        "head_loss_dw": generate_head_loss_dw_rows(),
        "hazen_williams": generate_hazen_williams_rows(),
        "manning_full_pipe": generate_manning_full_pipe_rows(),
        "manning_partial_flow": generate_manning_partial_flow_rows(),
        "rectangular_channel": generate_rectangular_channel_rows(),
        "pump_curves": generate_pump_curve_rows(),
    }

    # Summary
    for key, rows in fixture.items():
        print(f"  {key}: {len(rows)} rows")

    # Validate minimum counts before writing
    assert len(fixture["darcy_weisbach"]) >= 10, "need ≥ 10 DW friction rows"
    assert len(fixture["head_loss_dw"]) >= 10, "need ≥ 10 DW head-loss rows"
    assert len(fixture["hazen_williams"]) >= 10, "need ≥ 10 HW rows"
    assert len(fixture["manning_full_pipe"]) >= 10, "need ≥ 10 Manning full-pipe rows"
    assert len(fixture["manning_partial_flow"]) >= 10, "need ≥ 10 Manning partial-flow rows"
    assert len(fixture["rectangular_channel"]) >= 5, "need ≥ 5 open-channel rows"
    assert len(fixture["pump_curves"]) >= 5, "need ≥ 5 pump-curve rows"

    with OUTPUT_FILE.open("w", encoding="utf-8") as f:
        json.dump(fixture, f, indent=2, ensure_ascii=False)

    print(f"Written: {OUTPUT_FILE}")


if __name__ == "__main__":
    main()
