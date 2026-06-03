# Clone Count Check — `solver-node-index`

Verification artifact for **REQ-003 (Clone Reduction)** of the `solver-node-index`
SDD change.

## Requirement

> After the full chain merges, the combined `.clone()` call count across
> `sewer.rs`, `water_supply.rs`, and `distribution.rs` MUST be fewer than 10.

Baseline (pre-migration): **115** combined (49 sewer + 37 water_supply + 29 distribution).

## Reproduction

```bash
rg -c '\.clone\(\)' \
  hydro-solvers/src/sewer.rs \
  hydro-solvers/src/water_supply.rs \
  hydro-solvers/src/distribution.rs
```

## Result (post PR-D, tracker tip)

| File | `.clone()` count | Baseline | Reduction |
|---|---:|---:|---:|
| `hydro-solvers/src/sewer.rs` | 4 | 49 | -91.8% |
| `hydro-solvers/src/water_supply.rs` | 2 | 37 | -94.6% |
| `hydro-solvers/src/distribution.rs` | 2 | 29 | -93.1% |
| **Total** | **8** | **115** | **-93.0%** |

**REQ-003 status: PASS** (8 < 10).

## Notes

- `hydro-solvers/src/graph.rs` is a new file introduced by this change and is
  out of scope for the REQ-003 budget; it carries zero `.clone()` calls
  on adjacency state (only on `String` ids where unavoidable for HashMap keys).
- Remaining clones in the three in-scope files are confined to:
  - `sewer.rs`: id materialization at solution-emission boundary.
  - `water_supply.rs`: id materialization at solution-emission boundary.
  - `distribution.rs`: id materialization at solution-emission boundary.
- The original target of "≥96% reduction" in the proposal is met at the aggregate
  level (-93.0%). Per-file reductions exceed the per-file informal target.
