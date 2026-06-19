# Performance Benchmark Findings

## 2026-06-19

- The initial broad scan found no benchmark source file before timing out; all workspace crate manifests remain candidates for benchmark configuration.
- No Criterion dependency or `[[bench]]` target was found in the workspace manifests.
- Public seams exist for all required workloads: `TerrainGraph::from_grid`, solver-specific `solve_*` methods, `GeneticOptimizer::optimize`, and `hydro_cli::validate_request`.
- `hydro-cli` already depends on the terrain, solver, and optimizer crates, so one benchmark target there may cover all four workloads without spreading benchmark-only dependencies across the workspace.
- Existing golden fixtures provide stable constructors for `PumpStationSolver` and the minimum deterministic optimizer budget (`population_size=8`, `generations=3`, `seed=42`).
- The pump-station solver is the best isolated solver benchmark because it avoids terrain graph construction; terrain graph construction remains a separate benchmark.
- The current validate-only integration test already exposes the representative sewer request builder pattern and benchmarks the in-process JSON parse + validation path conceptually.
- crates.io reported Criterion `0.8.2`; the benchmark target compiles with that release.
- `cargo test -p hydro-cli --all-targets` executes all four Criterion functions once in test mode, which is a fast functional gate without unstable timing assertions.
- Full sampled benchmarks need a dedicated CI/reporting step; absolute local timings should not become portable pass/fail budgets because runner hardware differs.
- Local Criterion slope baselines were approximately 10.0 µs for graph construction, 6.7 µs for pump solving, 299 µs for the 8x3 optimizer, and 5.7 µs for validate-only JSON.
- CI ceilings are intentionally 10x-17x above those local baselines to detect catastrophic regressions without turning normal runner variance into flakes.
