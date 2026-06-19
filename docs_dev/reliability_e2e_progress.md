# Progress: Reliability E2E tests

- Started from checkpoint after commit `1acced3 ci: add security dependency gates`.
- Existing working tree has pre-existing untracked docs/docs_dev/docs-site/TESTDATA and modified `.atl` files.
- Added mapping coverage for `nsga_num_workers`, norm policy, forbidden zones, and mandatory routes.
- Added `hydro-cli/tests/reliability_e2e.rs` with full `run()` coverage for strict norm compliance + parallel workers and spatial constraints + parallel workers.
- Fixed request-to-optimizer config plumbing in `hydro-cli/src/lib.rs`.
- Added `OptimizationConfig::set_spatial_constraints` to avoid exposing internal config helper types across crates.
- Verification passed: `cargo fmt --all -- --check`, `CARGO_INCREMENTAL=0 cargo test -p hydro-cli`, and `CARGO_INCREMENTAL=0 cargo clippy -p hydro-cli --all-targets -- -D warnings`.
