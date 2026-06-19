# Task: Activate Optimizer Parity Gates

## Goal
Replace the two ignored placeholder tests with executable, committed oracle parity gates for optimizer quality metrics and path smoothing.

## Phases
- [x] Phase 1: Inspect ignored tests and current optimizer/routing APIs
- [x] Phase 2: Define reproducible fixtures and the smallest test seam
- [x] Phase 3: Add the failing path-length parity test
- [x] Phase 4: Implement the required routing fix and committed fixture without widening production APIs
- [x] Phase 5: Run focused and crate-level verification for the path-parity work unit
- [x] Phase 6: Replace the remaining HV/IGD+ placeholder with an executable five-objective differential metric gate
- [x] Phase 7: Run final verification for the metrics work unit

## Decisions
| Decision | Rationale | Date |
|----------|-----------|------|
| Do not remove `#[ignore]` until placeholders are replaced | A green placeholder would hide the missing parity contract instead of closing it | 2026-06-19 |
| Activate path-length parity before optimizer-quality parity | The path gate exposes a bounded routing defect and is an independently reviewable work unit | 2026-06-19 |
| Keep path parity inside the routing unit-test module | It exercises crate-private behavior without widening the production API solely for testing | 2026-06-19 |
| Replace the non-viable sewer optimizer skeleton with exact metric parity | A committed DEAP/IGD+ fixture gives a deterministic cross-language gate; the Python optimizer cannot be a practical CI dependency at over ten minutes for 4/1 | 2026-06-19 |

## Errors Encountered
| Error | Attempt | Resolution |
|-------|---------|------------|
| Broad `rg` fixture search returned exit code 1 after producing excessive output | Initial inventory | Narrow subsequent searches to explicit files and directories |
| Recursive Python-oracle scan timed out and hit a denied `.pytest_cache` | Oracle discovery | Restrict searches to the oracle's `src/` and `tests/` trees with `rg` |
| Path parity RED returned Rust length `20.0` vs oracle `21.328780519261954` | Strict-TDD RED | Offset visibility candidates by the configured clearance before A* |
| `cargo fmt --check` found wrapping and trailing-newline differences | Full verification attempt | Run `cargo fmt --all`, then restart the full verification gate |
| Python sewer parity run at validated 20/10 budget exceeded four minutes | Oracle generation attempt | Use the established 8/3 deterministic smoke budget via a non-user-facing test fixture configuration |
| Python sewer parity run at 8/3 still exceeded two minutes | Oracle generation retry | Reduce the dedicated oracle case to a 3x3 terrain and the Rust-valid minimum 4/1 budget |
| Compact 3x3 sewer optimizer run at 4/1 still exceeded two minutes | Third oracle attempt | Separate metric implementation from fixture generation; generate the expensive oracle offline only after the metric gate is proven |
| Terminated oracle shells left three CPU-bound Python children alive | Process audit | Identified exact child PIDs, stopped only those processes, and verified no Python process remained |
| Pump-station oracle at 8/3 also exceeded two minutes | Fast-solver probe | Retry once at the minimum 4/1 budget and allow the offline generator to finish; never run it in CI |
| Pump-station oracle at 4/1 consumed a full CPU for over ten minutes | Final end-to-end oracle attempt | Replace the stale ignored skeleton with exact differential metric parity; track expensive optimizer-quality benchmarking separately |
