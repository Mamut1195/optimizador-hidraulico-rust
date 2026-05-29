"""
Oracle generator for norms_smoke.json.

Queries the Python NormRegistry to produce a fixture that Rust integration
tests use to verify:
  - all 15 canonical profiles load without error
  - alias resolution works (spot-checked aliases)
  - rule counts per project_type match the Python oracle exactly

Run with the Python oracle engine on the PYTHONPATH:

    cd motor-optimizacion-hidraulico
    python ../optimizador-hidraulico-rust/tests/oracle/gen/generate_norms_smoke.py

Output: tests/oracle/fixtures/norms_smoke.json
"""

from __future__ import annotations

import json
import pathlib
import sys

# Allow running from the motor-optimizacion-hidraulico repo root.
ORACLE_ROOT = pathlib.Path(__file__).parent.parent.parent.parent.parent / "motor-optimizacion-hidraulico"
if str(ORACLE_ROOT) not in sys.path:
    sys.path.insert(0, str(ORACLE_ROOT / "src"))

from hydro_engine.norms.profile import NormRegistry
from hydro_engine.core.types import ProjectType

FIXTURES = (
    pathlib.Path(__file__).parent.parent / "fixtures"
)
FIXTURES.mkdir(exist_ok=True)

OUTPUT = FIXTURES / "norms_smoke.json"


def main() -> None:
    profiles = NormRegistry._load_profiles()

    entries = []
    for code, profile in profiles.items():
        rule_counts: dict[str, int] = {}
        for pt in ProjectType:
            rules = profile.rules_for(pt)
            rule_counts[pt.name] = len(rules)
        entries.append(
            {
                "code": code,
                "rule_counts": rule_counts,
            }
        )

    payload = {"profiles": entries}
    OUTPUT.write_text(json.dumps(payload, indent=2, ensure_ascii=False), encoding="utf-8")
    print(f"Written {len(entries)} profile entries to {OUTPUT}")
    for e in entries:
        print(f"  {e['code']}: {e['rule_counts']}")


if __name__ == "__main__":
    main()
