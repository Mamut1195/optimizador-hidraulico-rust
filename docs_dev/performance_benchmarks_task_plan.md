# Task: Add Performance Regression Baselines

## Goal
Add reproducible Rust benchmarks for terrain graph construction, solver execution, the optimizer loop, and CLI validate-only processing, with explicit regression budgets suitable for CI.

## Phases
- [x] Phase 1: Inventory current benchmark infrastructure and public test seams
- [x] Phase 2: Define deterministic representative fixtures and regression budgets
- [x] Phase 3: Confirm the benchmark target is absent before implementation
- [x] Phase 4: Implement benchmark targets by reviewable work unit
- [x] Phase 5: Run benchmark compilation, focused execution, fmt, Clippy, and tests
- [ ] Phase 6: Add CI regression-budget tracking and retained benchmark reports

## Decisions
| Decision | Rationale | Date |
|----------|-----------|------|
| Prefer Criterion benchmark targets over timing assertions in unit tests | Wall-clock assertions are flaky; Criterion provides reproducible sampling and baselines | 2026-06-19 |
| Host one benchmark target in `hydro-cli` | The integration crate already depends on terrain, solvers, optimizer, and validation APIs; one target avoids benchmark-only dependency spread | 2026-06-19 |
| Use pump-station fixtures for solver and optimizer benchmarks | Pump station isolates solver cost from terrain construction and has deterministic golden inputs | 2026-06-19 |

## Errors Encountered
| Error | Attempt | Resolution |
|-------|---------|------------|
| Broad recursive benchmark inventory timed out after 21 seconds | Initial repository scan | Restrict searches to manifests, source crates, tests, and CI paths; exclude docs and generated artifacts |
| PowerShell did not expand `hydro-*/Cargo.toml` for `rg` | Narrow manifest scan | Pass explicit manifest paths or enumerate them with PowerShell instead of using a Unix-style positional glob |
| Reading three large integration-test files exceeded the 10-second command budget | Fixture inspection | Extract only the constructor/test regions needed for representative benchmark fixtures with bounded `rg -A/-B` context |
| Official web lookup for the current Criterion release returned HTTP 403 | Dependency verification | Query the crates.io index with `cargo search criterion --limit 1` instead of guessing a stale version |
| Escalated full Criterion sampling remained pending without launching a process | Baseline measurement | Verify the target through release compilation, `cargo test --all-targets`, benchmark enumeration, fmt, and Clippy; defer sampled baseline capture to the CI-budget work unit |
| Sandbox compilation under the OneDrive workspace could not replace Rust object files | Final verification attempt | Run Cargo test and Clippy outside the sandbox using approved command prefixes |
