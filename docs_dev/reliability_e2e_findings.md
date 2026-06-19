# Findings: Reliability E2E tests

## Research Log

## Phase 2 findings
- `DesignRequest` already carries `forbidden_zones`, `mandatory_routes`, `nsga_num_workers`, `norm`, and `strict_norm_compliance`.
- `build_optimization_config` maps `nsga_num_workers` but currently does not map spatial constraints or norm fields into `OptimizationConfig`.
- `OptimizationConfig` has matching `ForbiddenZone`, `MandatoryRoute`, `norm_profile`, and `strict_norm_compliance` fields, so the next reliable slice is request-to-config plumbing plus full `run()` coverage using these fields.
