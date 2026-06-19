"""Fail CI when Criterion measurements exceed conservative absolute budgets."""

from __future__ import annotations

import argparse
import json
from pathlib import Path


def evaluate_budgets(
    criterion_dir: Path,
    budgets_ns: dict[str, float],
) -> list[str]:
    failures: list[str] = []
    for benchmark, budget_ns in budgets_ns.items():
        estimates_path = (
            criterion_dir / "performance" / benchmark / "new" / "estimates.json"
        )
        if not estimates_path.is_file():
            failures.append(f"{benchmark}: missing Criterion estimates at {estimates_path}")
            continue

        estimates = json.loads(estimates_path.read_text(encoding="utf-8"))
        measured_ns = float(estimates["slope"]["point_estimate"])
        if measured_ns > budget_ns:
            failures.append(
                f"{benchmark}: {measured_ns:.0f} ns exceeds {budget_ns:.0f} ns budget"
            )
        else:
            print(
                f"PASS {benchmark}: {measured_ns:.0f} ns <= {budget_ns:.0f} ns"
            )
    return failures


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--criterion-dir",
        type=Path,
        default=Path("target/criterion"),
    )
    parser.add_argument(
        "--budgets",
        type=Path,
        default=Path("benchmarks/performance_budgets.json"),
    )
    args = parser.parse_args()

    config = json.loads(args.budgets.read_text(encoding="utf-8"))
    failures = evaluate_budgets(args.criterion_dir, config["budgets_ns"])
    if failures:
        for failure in failures:
            print(f"FAIL {failure}")
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
