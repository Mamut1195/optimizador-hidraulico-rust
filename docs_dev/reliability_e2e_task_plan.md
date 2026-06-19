# Task: Reliability E2E tests

## Goal
Add the next 10/10 readiness work unit: reliability-focused end-to-end tests for strict norm compliance, parallel NSGA workers, forbidden zones, and mandatory routes.

## Phases
- [x] Phase 1: Load prior checkpoint
- [x] Phase 2: Map test/API structure
- [x] Phase 3: Implement smallest reliable E2E slice
- [x] Phase 4: Run targeted and workspace verification
- [ ] Phase 5: Save checkpoint

## Decisions
| Decision | Rationale | Date |
|----------|-----------|------|
| Keep this as one work-unit slice | Tests prove release-readiness behavior and should land together if compact enough. | 2026-06-19 |

## Errors Encountered
| Error | Attempt | Resolution |
|-------|---------|------------|
| `cargo test` accepts one filter before `--`; passed three filters. | Targeted RED run for new tests. | Rerun the full mapping test binary instead. |
