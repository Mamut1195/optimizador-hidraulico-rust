import json
import unittest
from pathlib import Path
from unittest.mock import patch

from scripts.check_benchmark_budgets import evaluate_budgets


class BenchmarkBudgetTests(unittest.TestCase):
    def test_accepts_measurements_within_budget(self) -> None:
        estimate = json.dumps({"slope": {"point_estimate": 9_900.0}})
        with patch.object(Path, "is_file", return_value=True), patch.object(
            Path, "read_text", return_value=estimate
        ):
            failures = evaluate_budgets(
                Path("criterion"),
                {"terrain_graph_from_grid": 100_000.0},
            )

        self.assertEqual(failures, [])

    def test_reports_over_budget_and_missing_measurements(self) -> None:
        estimate = json.dumps({"slope": {"point_estimate": 6_000_000.0}})
        with patch.object(
            Path,
            "is_file",
            side_effect=[True, False],
        ), patch.object(Path, "read_text", return_value=estimate):
            failures = evaluate_budgets(
                Path("criterion"),
                {
                    "pump_station_optimizer_8x3": 5_000_000.0,
                    "validate_only_json": 100_000.0,
                },
            )

        self.assertEqual(len(failures), 2)
        self.assertIn("exceeds", failures[0])
        self.assertIn("missing", failures[1])


if __name__ == "__main__":
    unittest.main()
