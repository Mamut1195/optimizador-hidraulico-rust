# Task: Harden CLI Validated Request Extraction

## Goal
Replace post-validation `unwrap()` calls in the CLI execution path with explicit typed extraction while preserving behavior.

## Phases
- [x] Phase 1: Recover prior context and identify the unsafe extraction sites
- [x] Phase 2: Add regression tests for invalid request extraction
- [x] Phase 3: Implement typed extraction helpers
- [x] Phase 4: Run formatting, tests, and Clippy

## Decisions
| Decision | Rationale | Date |
|----------|-----------|------|
| Keep this as one CLI hardening work unit | The change should be independently reviewable and reversible | 2026-06-19 |
| Return `CliError::ValidationError` from extraction | Validation and dispatch can evolve independently without introducing panic paths | 2026-06-19 |

## Errors Encountered
| Error | Attempt | Resolution |
|-------|---------|------------|
| `required_field` unresolved import | RED test run | Expected strict-TDD failure; implement the helper next |
| OneDrive denied removal of Rust object files | GREEN test run | Use a fresh workspace-local target directory with incremental compilation disabled |
| `C:\\tmp` target directory was denied by the managed sandbox | GREEN retry | Re-run the isolated target with the required sandbox approval |
| `cargo fmt --check` reported expected formatting differences | Verification | Run `cargo fmt --all`, then re-run the check |
