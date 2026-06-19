# Optimizer Parity Progress

## 2026-06-19

- Recovered the audit priority after committing CLI extraction hardening.
- Confirmed the only ignored Rust tests are the two PR-8f optimizer parity skeletons.
- Inspected the skeleton, metric implementation references, routing visibility, and optimizer manifest.
- Identified that fixture generation alone is insufficient because both tests still contain placeholder computations and the required routing API is crate-private.
- Confirmed `contracts/` is absent, located the sibling Python oracle, and narrowed fixture work to the existing `tests/oracle` infrastructure.
- Executed the Python path-smoothing oracle for the blocking-square case and captured its deterministic length.
- Identified a Rust clearance/candidate-node defect that the new parity test must expose before implementation.
- Added the path-length parity test and confirmed RED at 6.23% relative error, above the 2% contract.
- Implemented clearance-offset visibility candidates with orientation-aware miter geometry.
- Confirmed focused GREEN for both path-length parity and the offset-node regression test.
- Added a reproducible Python generator and committed-format JSON oracle fixture; moved path parity into the crate-private routing test seam and removed its ignored placeholder.
- Full `hydro-optimizer` verification passed: 196 unit tests plus 6 integration smoke tests, with only the separate HV/IGD+ placeholder still ignored; Clippy and formatting are clean.
- Verified commit `456f0f8` and resumed Phase 6.
- Located the production Rust front extraction seam and confirmed the Python oracle can compute exact five-dimensional hypervolume through DEAP.
- Stopped the first Python oracle run after the minimum public 20/10 budget proved unsuitable for a fast CI parity fixture; the next attempt uses the existing deterministic 8/3 smoke budget.
- Stopped the 8/3 retry after it also exceeded the fixture-generation budget; narrowed the case to the smallest valid Rust optimizer budget and a compact terrain.
- Audited and cleaned up three orphaned Python oracle processes left by shell termination before resuming Rust tests.
- Added multidimensional HV tests, confirmed RED (`0` for 3-D/5-D), implemented exact recursive slicing, and confirmed GREEN.
- Exhausted bounded end-to-end Python oracle attempts, including a 10-minute minimum pump-station run, and pivoted to a deterministic metric-level differential gate rather than preserving a permanently ignored false promise.
- Added direction-sensitive IGD+ tests, confirmed RED for both domination directions, and corrected the minimization distance sign.
- Added a reproducible DEAP/independent-Python fixture, activated exact five-objective HV/IGD+ parity, and removed the final ignored placeholder test.
- Final verification passed: 201 unit tests and 6 integration smoke tests with zero ignored tests; formatting and strict Clippy are clean.
