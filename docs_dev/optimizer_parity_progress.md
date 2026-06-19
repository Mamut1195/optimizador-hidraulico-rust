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
