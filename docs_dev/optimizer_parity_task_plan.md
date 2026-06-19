# Task: Activate Optimizer Parity Gates

## Goal
Replace the two ignored placeholder tests with executable, committed oracle parity gates for optimizer quality metrics and path smoothing.

## Phases
- [x] Phase 1: Inspect ignored tests and current optimizer/routing APIs
- [x] Phase 2: Define reproducible fixtures and the smallest test seam
- [x] Phase 3: Add the failing path-length parity test
- [x] Phase 4: Implement the required routing fix and committed fixture without widening production APIs
- [x] Phase 5: Run focused and crate-level verification for the path-parity work unit
- [ ] Phase 6: Replace the remaining HV/IGD+ placeholder with an executable optimizer-quality gate

## Decisions
| Decision | Rationale | Date |
|----------|-----------|------|
| Do not remove `#[ignore]` until placeholders are replaced | A green placeholder would hide the missing parity contract instead of closing it | 2026-06-19 |
| Activate path-length parity before optimizer-quality parity | The path gate exposes a bounded routing defect and is an independently reviewable work unit | 2026-06-19 |
| Keep path parity inside the routing unit-test module | It exercises crate-private behavior without widening the production API solely for testing | 2026-06-19 |

## Errors Encountered
| Error | Attempt | Resolution |
|-------|---------|------------|
| Broad `rg` fixture search returned exit code 1 after producing excessive output | Initial inventory | Narrow subsequent searches to explicit files and directories |
| Recursive Python-oracle scan timed out and hit a denied `.pytest_cache` | Oracle discovery | Restrict searches to the oracle's `src/` and `tests/` trees with `rg` |
| Path parity RED returned Rust length `20.0` vs oracle `21.328780519261954` | Strict-TDD RED | Offset visibility candidates by the configured clearance before A* |
| `cargo fmt --check` found wrapping and trailing-newline differences | Full verification attempt | Run `cargo fmt --all`, then restart the full verification gate |
