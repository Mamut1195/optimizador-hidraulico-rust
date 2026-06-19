"""Generate the Python PathSmoother parity fixture for hydro-optimizer."""

from __future__ import annotations

import json
import math
import sys
from pathlib import Path

from shapely.geometry import Polygon

REPO_ROOT = Path(__file__).resolve().parents[3]
ENGINE_ROOT = REPO_ROOT.parent / "motor-optimizacion-hidraulico" / "src"
if str(ENGINE_ROOT) not in sys.path:
    sys.path.insert(0, str(ENGINE_ROOT))

from hydro_engine.optimization.routing import PathSmoother


def main() -> None:
    start = (0.0, 5.0)
    end = (20.0, 5.0)
    vertices = [(7.0, 2.0), (13.0, 2.0), (13.0, 8.0), (7.0, 8.0)]
    clearance_distance = 0.1
    rdp_epsilon = 0.5

    polygon = Polygon(vertices)
    waypoints = PathSmoother(
        [polygon], obstacle_buffer=clearance_distance
    ).smooth(start, end)
    path = [start, *waypoints, end]
    oracle_length = sum(math.dist(a, b) for a, b in zip(path, path[1:]))

    fixture = {
        "schema_version": 1,
        "fixture_name": "path_smoothing_golden",
        "description": "Blocking-square PathSmoother parity fixture.",
        "start": start,
        "end": end,
        "forbidden_zones": [vertices],
        "clearance_distance": clearance_distance,
        "rdp_epsilon": rdp_epsilon,
        "oracle_waypoints": waypoints,
        "oracle_length": oracle_length,
        "relative_tolerance": 0.02,
    }

    output = REPO_ROOT / "tests" / "oracle" / "fixtures" / "path_smoothing_golden.json"
    output.write_text(json.dumps(fixture, indent=2) + "\n", encoding="utf-8")
    print(f"Written: {output}")
    print(f"  oracle_length = {oracle_length:.15f}")


if __name__ == "__main__":
    main()
