# Oracle Harness — Differential Testing Against the Python Engine

This directory contains:

- `gen/` — Python scripts that import the `hydro_engine` oracle and emit golden JSON fixtures.
- `fixtures/` — Committed golden JSON fixtures used by Rust integration tests.
- `src/` — Rust fixture loader crate (`oracle`) that deserializes fixtures and exposes typed helpers.

## Purpose

Every public Rust function that has an EXACT-match parity target (abs ≤ 1e-9) is tested against
a value produced by the Python oracle.  Fixtures are committed so the test suite runs without
a Python runtime in CI.

## Generating Fixtures

Fixtures are committed and treated as **ground truth**.  Regeneration is a deliberate act —
do not regenerate unless the Python oracle changed intentionally.

### Prerequisites

- Python 3.11+
- `numpy` installed (`pip install numpy`)
- The Python oracle engine at `../motor-optimizacion-hidraulico/src/` (relative to the workspace root)

### Regenerate all hydraulics fixtures

```bash
# From workspace root:
python tests/oracle/gen/generate_hydraulics.py
```

The script auto-detects the oracle path at
`<workspace-root>/../motor-optimizacion-hidraulico/src`.
Set `PYTHONPATH` explicitly if the auto-detection fails:

```bash
PYTHONPATH=/path/to/motor-optimizacion-hidraulico/src \
    python tests/oracle/gen/generate_hydraulics.py
```

### Lock-in policy

Once a fixture is committed, its values are the parity targets for Rust.
A diff in a fixture file in a PR always means one of:

1. **Intentional oracle update** — the Python engine logic changed and we are updating targets.
2. **Unintended regression** — something changed by mistake.

Both cases require explicit review.  Fixture diffs should never be silently auto-merged.

## Fixture schemas

### `hydraulics_golden.json`

| Key | Rows | Description |
|-----|------|-------------|
| `darcy_weisbach` | 72 | Swamee-Jain friction factor: `(reynolds, roughness_mm, diameter_m) → friction_factor` |
| `head_loss_dw` | 60 | DW head loss: `(velocity_m_s, diameter_m, length_m, roughness_mm) → head_loss_m` |
| `hazen_williams` | 144 | HW head loss + velocity: `(flow_m3s, diameter_m, length_m, c) → head_loss_m, velocity_m_s` |
| `manning_full_pipe` | 90 | Manning full-pipe: `(diameter_m, slope, roughness_n) → velocity_m_s, capacity_m3s` |
| `manning_partial_flow` | 11 | Partial-flow ratios: `(y_over_d) → area_ratio, radius_ratio, flow_ratio` |
| `rectangular_channel` | 72 | Open-channel flow: `(width_m, depth_m, slope, roughness_n) → velocity_m_s, flow_m3s` |
| `pump_curves` | 25 | Pump head at flow: `(shutoff_head_m, q_bep_m3s, query_flow_m3s) → expected_head_m` |

## Using the loader in Rust tests

Add `oracle` as a `dev-dependency` in your crate's `Cargo.toml`:

```toml
[dev-dependencies]
oracle = { path = "../tests/oracle" }
approx = { workspace = true }
```

Then in your test:

```rust
use approx::assert_abs_diff_eq;
use oracle::hydraulics::load_hydraulics_golden;

#[test]
fn darcy_weisbach_parity() {
    let golden = load_hydraulics_golden();
    for row in &golden.darcy_weisbach {
        let rust_result = your_friction_factor(row.reynolds, row.roughness_mm, row.diameter_m);
        assert_abs_diff_eq!(rust_result, row.friction_factor, epsilon = 1e-9);
    }
}
```
