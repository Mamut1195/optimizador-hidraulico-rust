"""Generate exact five-objective HV and IGD+ parity values."""

from __future__ import annotations

import json
import math
from pathlib import Path

import numpy as np
from deap.tools._hypervolume import hv

REPO_ROOT = Path(__file__).resolve().parents[3]


def igd_plus(approximation: list[list[float]], reference: list[list[float]]) -> float:
    total = 0.0
    for reference_point in reference:
        distances = []
        for approximation_point in approximation:
            squared = sum(
                max(0.0, a - r) ** 2
                for a, r in zip(approximation_point, reference_point, strict=True)
            )
            distances.append(math.sqrt(squared))
        total += min(distances)
    return total / len(reference)


def main() -> None:
    approximation_front = [
        [1.0, 4.0, 2.0, 5.0, 3.0],
        [2.0, 2.5, 3.0, 3.0, 2.0],
        [4.0, 1.0, 4.0, 2.0, 1.0],
    ]
    reference_front = [
        [0.8, 3.8, 1.8, 4.8, 2.8],
        [1.8, 2.3, 2.8, 2.8, 1.8],
        [3.8, 0.8, 3.8, 1.8, 0.8],
        [2.5, 1.7, 2.4, 2.2, 1.4],
    ]
    reference_point = [6.0] * 5

    fixture = {
        "schema_version": 1,
        "fixture_name": "optimizer_quality_metrics_golden",
        "description": "Exact five-objective metric parity generated with DEAP HV and independent Python IGD+.",
        "approximation_front": approximation_front,
        "reference_front": reference_front,
        "reference_point": reference_point,
        "oracle_hypervolume": float(
            hv.hypervolume(np.array(approximation_front), np.array(reference_point))
        ),
        "oracle_igd_plus": igd_plus(approximation_front, reference_front),
        "absolute_tolerance": 1e-12,
    }

    output = (
        REPO_ROOT
        / "tests"
        / "oracle"
        / "fixtures"
        / "optimizer_quality_metrics_golden.json"
    )
    output.write_text(json.dumps(fixture, indent=2) + "\n", encoding="utf-8")
    print(f"Written: {output}")


if __name__ == "__main__":
    main()
