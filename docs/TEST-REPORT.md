# TEST-REPORT — optimizador-hidraulico-rust

**Date**: 2026-06-02
**Auditor**: Claude Code (Sonnet 4.6)
**Workspace commit reference**: v1 boundary closed at `c73e677` (Phase 7 / hydro-cli merged)

---

## 1. Summary

| Category | Count |
|---|---|
| Production crates | 7 |
| Oracle helper crate | 1 (tests/oracle) |
| Integration test files | 34 |
| Integration tests | 178 |
| Source files containing inline tests | 38 |
| Inline tests (estimated) | ~332 |
| **Total tests** | **~510** |
| Requirement identifiers covered | REQ-001 through REQ-018 |
| Work Units exercised | WU-1 through WU-8 |

Ignored tests (flagged `#[ignore]`): 2 (in `hydro-optimizer/tests/pr8f_parity_skeleton.rs`; awaiting fixture files for HV/IGD+ parity and path-smoothing length parity).

---

## 2. Methodology

**TDD gate enforcement**: Every test file in `hydro-*/tests/` carries a module-level doc comment explicitly marking it as the RED commit (test exists, implementation is `todo!()` or returns `Err`) and the GREEN commit (implementation satisfies all assertions). Test names are part of the public contract and must not be renamed.

**Oracle parity model**: A Python hydraulic engine generates "golden" JSON fixtures stored in `tests/oracle/fixtures/`. Rust results are compared against these using `approx::assert_abs_diff_eq!` with tolerances ranging from 1e-9 (pure floating-point arithmetic) to 1e-4 (network-level cost aggregation). The oracle helper crate (`tests/oracle/`) provides `load_*_golden()` functions used in integration tests.

**Snapshot testing**: `insta` snapshots are used in `hydro-terrain/tests/route_variant.rs` for deterministic edge-list output.

**Property-based testing**: `proptest` is used in `hydro-terrain/tests/steiner.rs` (20 random cases, 2-5 terminals each) to verify connectivity and tree invariants hold for arbitrary terminal subsets.

**Performance gates**: `hydro-cli/tests/validate_only.rs` asserts that `validate_request` completes in under 50 ms for both valid and invalid inputs, enforcing the REQ-007 latency SLA in-process.

**Binary smoke tests**: `hydro-cli/tests/binary_smoke.rs` spawns the compiled `hydro-cli` binary via `std::process::Command`, verifying the full stdin/stdout/exit-code contract independently of the library API.

---

## 3. Per-Crate Test Sections

### 3.1 hydro-types

**Source**: `hydro-types/src/`
**Inline tests**: ~52 (across constraints.rs, enums.rs, network.rs, request.rs, response.rs)
**Integration tests**: 0 (all coverage is inline)

#### 3.1.1 constraints.rs (~12 inline tests)

| Test name | Behavior verified | Regression caught |
|---|---|---|
| `default_values_match_oracle` | `DesignConstraints::default()` field values match Python oracle defaults | Silent drift in default constants |
| `default_available_diameters_match_oracle` | `available_diameters` list matches the 9-entry oracle list | Diameter set truncation or reorder |
| `design_constraints_json_roundtrip` | Serialize → deserialize → identical struct | Serde attribute regression |
| `validate_min_slope_gt_max_slope_returns_err` | `min_slope > max_slope` → `CrossFieldViolation` | Cross-field validator missing a branch |
| `validate_min_velocity_gt_max_velocity_returns_err` | Same pattern for velocity | Same |
| `validate_min_diameter_gt_max_diameter_returns_err` | Same pattern for diameter | Same |
| `validate_min_pressure_gt_max_pressure_returns_err` | Same pattern for pressure | Same |
| `validate_default_constraints_are_valid` | `DesignConstraints::default().validate()` returns `Ok(())` | Tightening defaults past validator bounds |
| `sort_diameters_produces_ascending_order` | `sort_diameters()` sorts ascending | Sort direction swap |
| `select_diameter_picks_smallest_gte_required` | Selects smallest diameter ≥ required | Off-by-one in diameter selection |
| `select_diameter_returns_largest_when_required_exceeds_all` | Falls back to largest when nothing fits | Unclamped panic on oversized flow |
| `select_diameter_works_with_unsorted_input` | Works on unsorted input list | Caller assuming pre-sorted input |

#### 3.1.2 enums.rs (~16 inline tests)

| Test name | Behavior verified | Regression caught |
|---|---|---|
| `project_type_sewer_deserializes` | `"sewer"` → `ProjectType::Sewer` | Variant rename |
| `project_type_water_supply_deserializes` | `"water_supply"` deserializes | Underscore/camel confusion |
| `project_type_all_variants_roundtrip` | All 6 variants serialize and deserialize symmetrically | New variant breaking round-trip |
| `project_type_unknown_variant_returns_err` | Unknown string → error | Silently accepting junk input |
| `node_type_manhole_deserializes_from_screaming_case` | `"MANHOLE"` → `NodeType::Manhole` | Case normalization removed |
| `node_type_all_variants_roundtrip` | All node types serialize/deserialize | New node type breaking serde |
| `pipe_material_pvc_roundtrip` | `"PVC"` → `PipeMaterial::PVC` | Material name change |
| `pipe_material_pead_alias_resolves_to_hdpe` | `"PEAD"` alias → `PipeMaterial::HDPE` | Alias removed |
| `pipe_material_concreto_alias_resolves_to_concrete` | Spanish alias resolution | Locale handling dropped |
| `pipe_material_acero_alias_resolves_to_steel` | Spanish alias for Steel | Same |
| `pipe_material_fierro_fundido_alias_resolves_to_cast_iron` | Spanish alias for CastIron | Same |
| `pipe_material_unknown_returns_err` | Unknown material → error | Silent default material assignment |
| `pipe_material_manning_n_values_match_spec` | Manning's n coefficient for each material matches spec | Roughness constant drift |
| `flow_type_gravity_roundtrip` | `FlowType::Gravity` serde | Flow type regression |
| `severity_hard_roundtrip` | `Severity::Hard` serde | Severity enum regression |

#### 3.1.3 network.rs (~6 inline tests)

| Test name | Behavior verified | Regression caught |
|---|---|---|
| `node_id_roundtrip_oracle_style_g12` | `NodeId("g12")` round-trips through JSON | Newtype stripping in serde |
| `node_id_roundtrip_pipe_with_spaces_and_parens` | `NodeId("Pipe - (3)")` survives JSON | Special-character escaping regression |
| `pipe_id_roundtrip` | `PipeId` round-trips | Same for pipe IDs |
| `network_node_defaults_match_oracle` | `NetworkNode::default()` matches Python oracle defaults | Silent default drift |
| `network_node_json_roundtrip` | Full node serialization/deserialization | Field rename or serde attr change |
| `pipe_network_counts_are_correct` | `node_count()` and `pipe_count()` return correct values | Counting method regression |

#### 3.1.4 request.rs (~13 inline tests)

| Test name | Behavior verified | Regression caught |
|---|---|---|
| `too_few_terrain_points_fails_validation` | < 3 terrain points → `Err` | Validator threshold lowered |
| `terrain_points_missing_from_json_is_a_deserialization_error` | Missing `terrain_points` → serde error | Field rename without serde alias |
| `existing_network_depth_out_of_range_is_rejected` | `depth < 0` → error | Range check deleted |
| `existing_network_setback_out_of_range_is_rejected` | Negative setback → error | Same |
| `unknown_project_type_is_rejected` | Unknown string → serde error | Catch-all variant added |
| `extra_fields_are_rejected` | Unknown JSON fields → error (strict deserialize) | `deny_unknown_fields` removed |
| `constraint_min_slope_gt_max_slope_fails_validation` | Embedded constraint validation propagates | Nested validation skipped |
| `material_pead_alias_resolves_to_hdpe` | Material alias at request level | Alias dropped from request serde |
| `material_concreto_alias_resolves_to_concrete` | Spanish alias | Same |
| `norm_conagua_is_normalized_to_uppercase` | Norm code uppercasing | Case normalization removed |
| `default_values_match_spec` | All `Default`-valued fields match spec table | Spec drift |
| `minimal_valid_request_passes_validation` | Bare-minimum valid request passes | Validator over-rejection |
| `effective_constraints_applies_overrides` | Per-request constraint overrides take effect | Override logic short-circuited |

#### 3.1.5 response.rs (~6 inline tests)

| Test name | Behavior verified | Regression caught |
|---|---|---|
| `solution_score_default_values` | `SolutionScore::default()` all-zero | Default drift |
| `solution_score_json_roundtrip` | Score serializes/deserializes | Field removed |
| `diagnostics_default_and_compute_backend` | `Diagnostics::default()` + `compute_backend` value | Backend label change |
| `design_result_json_does_not_contain_mcp_instructions` | Output JSON has no injected instructions | Prompt-injection guard removed |
| `design_result_json_roundtrip` | Full result serializes/deserializes | Schema regression |
| `convergence_record_roundtrip` | `ConvergenceRecord` serde | Field removal |

---

### 3.2 hydro-hydraulics

**Source**: `hydro-hydraulics/src/`
**Inline tests**: ~33 (darcy_weisbach.rs × 5, manning.rs × 6, hazen_williams.rs × 13, open_channel.rs × 4, pump_curves.rs × 5)
**Integration tests**: 7 (in `hydro-hydraulics/tests/oracle_parity.rs`)

#### 3.2.1 Inline tests

| File | Test name | Behavior verified | Regression caught |
|---|---|---|---|
| darcy_weisbach.rs | `friction_factor_known_value_turbulent` | Swamee-Jain f at known Re/roughness | Formula constant changed |
| darcy_weisbach.rs | `friction_factor_high_reynolds_smooth_pipe` | Smooth-pipe regime (ε→0) | Regime boundary regression |
| darcy_weisbach.rs | `head_loss_dw_zero_velocity` | Zero velocity → zero head loss | Division-by-zero guard removed |
| darcy_weisbach.rs | `head_loss_dw_physical_range` | Result within physical bounds | Unit conversion error |
| darcy_weisbach.rs | `friction_factor_roughness_mm_to_m_conversion` | mm-to-m conversion applied | Unit factor removed |
| manning.rs | `full_flow_velocity_known_case` | Manning full-pipe velocity at known params | n constant drift |
| manning.rs | `full_flow_velocity_slope_sign_invariant` | Negative slope treated as flat (abs) | Sign error in slope |
| manning.rs | `full_flow_capacity_is_velocity_times_area` | Q = V × A consistency | Formula factoring error |
| manning.rs | `partial_flow_ratio_half_full` | y/D = 0.5 gives canonical ratio | Partial-flow table drift |
| manning.rs | `partial_flow_ratio_clamps_extremes` | y/D = 0 and 1 clamp correctly | Extrapolation beyond bounds |
| manning.rs | `partial_flow_ratio_monotonic_in_depth` | Flow ratio monotonically increases with y/D | Non-monotone interpolation |
| hazen_williams.rs | `velocity_zero_diameter_returns_zero` | D=0 → V=0 (guard) | Division by zero |
| hazen_williams.rs | `velocity_known_case` | HW velocity at known C/D/slope | HW exponent changed |
| hazen_williams.rs | `head_loss_positive_for_positive_flow` | Head loss sign correct | Sign inversion |
| hazen_williams.rs | `head_loss_symmetric_in_flow_sign` | Head loss symmetric in ±Q | Asymmetric formula applied |
| hazen_williams.rs | `hw_coefficient_pvc_is_150` | C=150 for PVC | Material table entry changed |
| hazen_williams.rs | `hw_coefficient_pead_is_150` | C=150 for PEAD/HDPE | Alias coefficient drift |
| hazen_williams.rs | `hw_coefficient_acero_is_120` | C=120 for steel | Steel constant changed |
| hazen_williams.rs | `hw_coefficient_fierro_fundido_is_100` | C=100 for cast iron | Cast iron constant changed |
| hazen_williams.rs | `hw_coefficient_unknown_falls_back_to_150` | Unknown material → C=150 | Fallback removed |
| hazen_williams.rs | `required_diameter_pressure_returns_smallest_feasible` | Selects smallest diameter meeting head constraint | Selection logic inverted |
| hazen_williams.rs | `required_diameter_pressure_falls_back_to_largest_when_tight` | Falls back to largest when none feasible | Crash on unsatisfiable constraint |
| hazen_williams.rs | `required_diameter_pressure_sorts_available_internally` | Works on unsorted input | Caller-sorted assumption |
| hazen_williams.rs | `required_diameter_pressure_parity_small_flow_big_head` | Specific parity case | Formula regression |
| open_channel.rs | `rectangular_channel_zero_depth_gives_zero_velocity` | Guard for y=0 | Division by zero |
| open_channel.rs | `rectangular_channel_flow_equals_v_times_area` | Q = V × B × y | Area formula |
| open_channel.rs | `rectangular_channel_slope_sign_invariant` | abs(slope) used | Sign error |
| open_channel.rs | `rectangular_channel_physical_values` | Known-value parity | Manning constant drift |
| pump_curves.rs | `make_curve_shutoff_head_at_q_zero` | H(0) = H_shutoff | Curve intercept wrong |
| pump_curves.rs | `make_curve_head_at_bep_is_eighty_five_percent` | H(Q_bep) = 0.85 × H_shutoff | BEP ratio changed |
| pump_curves.rs | `make_curve_head_at_qmax_near_zero` | H(Q_max) ≈ 0 | Curve endpoint regression |
| pump_curves.rs | `head_at_flow_clipped_to_zero` | Negative head clipped to 0 | Negative head returned |
| pump_curves.rs | `make_curve_degenerate_q_bep_zero` | Q_bep=0 edge case handled | Divide-by-zero on degenerate pump |

#### 3.2.2 Integration tests — oracle_parity.rs (T-2.x series)

| Test name | Behavior verified | Regression caught |
|---|---|---|
| `t2_1_friction_factor_parity_72_vectors` | Swamee-Jain formula: 72 oracle vectors, max_relative ≤ 1e-9 | Any constant or exponent change |
| `t2_1_head_loss_dw_parity_60_vectors` | Darcy-Weisbach head loss: 60 vectors, ≤ 1e-9 | Formula refactor with rounding |
| `t2_2_hazen_williams_parity_144_vectors` | HW velocity + head_loss: 144 vectors, ≤ 1e-9 | HW exponent or constant change |
| `t2_3_manning_full_pipe_parity_90_vectors` | Full-pipe V + Q: 90 vectors, ≤ 1e-9 | Manning n table change |
| `t2_3_manning_partial_flow_parity_11_vectors` | Partial-flow ratios: 11 vectors, ≤ 1e-9 | Interpolation table replacement |
| `t2_3_rectangular_channel_parity_72_vectors` | Open-channel V + Q: 72 vectors, ≤ 1e-9 | Manning constant drift |
| `t2_4_pump_curves_parity_25_vectors` | Pump head-at-flow: 25 vectors, ≤ 1e-9 | Curve polynomial changed |

---

### 3.3 hydro-norms

**Source**: `hydro-norms/src/`
**Inline tests**: ~15 (types.rs × 6, registry.rs × 5, validator.rs × 4)
**Integration tests**: 12 (registry_smoke.rs × 7, validator_golden.rs × 5)

#### 3.3.1 Inline tests

| File | Test name | Behavior verified | Regression caught |
|---|---|---|---|
| types.rs | `norm_rule_serde_round_trip` | `NormRule` serializes/deserializes | Field rename |
| types.rs | `norm_violation_serde_round_trip` | `NormViolation` serializes/deserializes | Same |
| types.rs | `severity_re_export_matches_hydro_types` | Re-exported `Severity` is the same type | Type alias broken |
| types.rs | `element_type_all_variants_serialize_lowercase` | All `ElementType` variants serialize lowercase | Case convention drift |
| types.rs | `norm_rule_default_severity_is_hard` | `NormRule::default()` has `Severity::Hard` | Default changed |
| types.rs | `norm_validation_result_serde_round_trip` | `NormValidationResult` serde | Schema regression |
| registry.rs | `resolve_alias_returns_none_for_unknown_code` | Unknown code → `None` | Catch-all alias added |
| registry.rs | `resolve_alias_conagua_returns_conagua_mx` | `"CONAGUA"` → `"CONAGUA_MX"` | Alias removed |
| registry.rs | `resolve_alias_quito_returns_epmaps_ec` | `"QUITO"` → `"EPMAPS_EC"` | Alias removed |
| registry.rs | `conagua_mx_loads_without_panic` | CONAGUA_MX profile loads without panic | YAML fixture deleted/broken |
| registry.rs | `get_normalizes_hyphen_and_spaces` | Input normalization before registry lookup | Normalization removed |
| validator.rs | `round6_matches_python_round_6_decimal_places` | `round6()` matches Python `round(x, 6)` | Rounding function replaced |
| validator.rs | `check_rule_returns_none_when_within_bounds` | Within-bounds value → no violation | Tolerance inverted |
| validator.rs | `check_rule_returns_violation_when_below_min` | Below-min value → violation with correct magnitude | Magnitude formula changed |
| validator.rs | `check_rule_tolerance_boundary_is_respected` | Tolerance boundary is inclusive | Off-by-epsilon boundary error |

#### 3.3.2 Integration tests

**registry_smoke.rs** (T-3.2 series):

| Test name | Behavior verified | Regression caught |
|---|---|---|
| `all_15_profiles_load_without_error` | All 15 norm profiles load without error | Missing or malformed YAML fixture |
| `conagua_alias_resolves_to_conagua_mx` | `"CONAGUA"` alias resolves at integration level | Alias removed |
| `quito_alias_resolves_to_epmaps_ec` | `"QUITO"` alias resolves | Alias removed |
| `unknown_code_returns_error` | Unknown code → `Err` | Default profile substituted silently |
| `rule_counts_match_oracle_for_all_profiles_and_project_types` | 15×6 = 90 rule-count assertions vs. oracle | Rule dropped or added in YAML |
| `conagua_pump_station_copies_from_conveyance` | `copy_from` expansion works for pump_station | copy_from expansion removed |
| `available_codes_returns_sorted_canonical_codes` | 15 canonical codes, sorted ascending | Code added/removed from registry |

**validator_golden.rs** (T-3.3 series):

| Test name | Behavior verified | Regression caught |
|---|---|---|
| `violation_counts_match_oracle_for_all_fixture_cases` | hard_violation_count + compliant flag match oracle for all fixture cases | Violation counting formula changed |
| `pump_bypassed_pipe_slope_violation_is_skipped` | 3 pipes, 1 pump-bypassed → 2 violations (not 3) | Bypass logic removed |
| `pressure_case_skips_reservoir_node` | Reservoir node excluded from pressure validation | Node-type filter removed |
| `all_within_bounds_produces_zero_violations` | CONAGUA_MX SEWER all-within-bounds → compliant=true, 0 violations | False positive violations |

---

### 3.4 hydro-terrain

**Source**: `hydro-terrain/src/`
**Inline tests**: ~18 (terrain.rs × 10, graph.rs × 5, cost.rs × 3)
**Integration tests**: 27 (terrain_model.rs × 7, terrain_graph.rs × 5, steiner.rs × 8, route_variant.rs × 4, k_shortest_paths.rs × 3)

#### 3.4.1 Inline tests

| File | Test name | Behavior verified | Regression caught |
|---|---|---|---|
| terrain.rs | `arange_matches_numpy` | `arange()` step matches `numpy.arange` | Off-by-one vs. numpy |
| terrain.rs | `arange_with_step_boundary` | Boundary element handling | Fence-post error |
| terrain.rs | `from_xyz_list_empty_errors` | Empty input → `Err` | Panic on empty |
| terrain.rs | `from_xyz_list_too_few_errors` | Fewer than 3 points → `Err` | Under-threshold acceptance |
| terrain.rs | `nearest_idx_exact_on_grid` | Nearest-index lookup on-grid exact | Index off-by-one |
| terrain.rs | `elevation_at_nn_fallback` | NN elevation fallback without grid | Grid-required panic |
| terrain.rs | `build_grid_and_bilinear_on_sample` | Bilinear on-sample exact | Grid interpolation regression |
| terrain.rs | `min_max_elevation` | min/max correct over point set | Aggregation error |
| terrain.rs | `bounds_correct` | xmin/xmax/ymin/ymax correct | Bounds computation error |
| terrain.rs | `delaunay_grid_on_sample_exact` | Delaunay grid on-sample within 1e-9 | Grid type switched |
| graph.rs | `tiny_grid_node_count` | 3×3 grid → 9 nodes | Node generation off-by-one |
| graph.rs | `tiny_grid_edge_count` | 3×3 grid → correct edge count | Edge formula changed |
| graph.rs | `node_ids_follow_counter_order` | Node IDs are sequential integers | ID generation changed |
| graph.rs | `shortest_path_tiny` | Dijkstra on tiny graph returns known path | Dijkstra regression |
| graph.rs | `perturb_variant_zero_noop` | variant=0 leaves weights unchanged | Perturbation applied unconditionally |
| cost.rs | `flat_segment_has_length_plus_excavation_plus_slope_penalty` | Flat terrain cost formula | Cost weight changed |
| cost.rs | `optimal_slope_has_no_penalty` | Optimal slope → no slope penalty | Penalty applied to optimal slope |
| cost.rs | `uphill_adds_pump_penalty` | Uphill segment adds pump penalty | Pump penalty removed |

#### 3.4.2 Integration tests

**terrain_model.rs** (T-4.1 series):

| Test name | Behavior verified | Regression caught |
|---|---|---|
| `on_sample_elevation_exact` | On-sample elevation queries within 1e-9 of oracle | Grid precision regression |
| `nearest_node_index_exact` | Nearest-node index matches oracle exactly | Nearest-neighbor algorithm change |
| `off_grid_elevation_statistical_p95` | p95 absolute error < 0.1 m over 200 queries (regular grid) | Interpolation method swap |
| `terrain_model_structural` | point_count, min/max elevation, bounds from oracle fixture | Structural field regression |
| `elevation_at_nn_fallback_no_grid` | NN fallback without grid: first 5 on-sample points exact | Fallback path removed |
| `from_xyz_list_empty_returns_error` | Empty list → `Err(TerrainError::EmptyTerrain)` | Panic instead of error |
| `off_grid_elevation_irregular_scatter_p95` | ~1000 scattered points, p95 < 0.1 m (ADR-5 validation, W-01 fix) | Scatter interpolation regression |

**terrain_graph.rs** (T-4.2 series):

| Test name | Behavior verified | Regression caught |
|---|---|---|
| `graph_node_edge_counts` | node_count + edge_count match oracle exactly | Graph construction regression |
| `nearest_node_exact` | 5 oracle cases: nearest node ID exact | Nearest-node lookup regression |
| `dijkstra_path_exact` | Oracle path list exact, weight within 1e-9 | Dijkstra path selection changed |
| `perturb_weights_stub_compiles` | Perturbation runs without panic, graph retains nodes | Perturbation breaks graph |
| `shortest_path_unknown_node_returns_error` | Unknown node ID → `Err` | Panic on unknown node |

**steiner.rs** (T-4.3 series):

| Test name | Behavior verified | Regression caught |
|---|---|---|
| `steiner_connectivity_exact` | All 5 terminals connected in result tree | Terminal disconnected after merge |
| `steiner_no_cycles` | edge_count == node_count - 1 | Cycle introduced in Steiner tree |
| `steiner_weight_within_10pct_of_oracle` | Approximation ratio ≤ 1.10 (NEVER relax) | Approximation quality degraded |
| `steiner_suboptimal_fixture_is_genuinely_suboptimal` | approximation_gap > 1.0 + 1e-6 (test proves the oracle gap is real) | Fixture replaced with exact optimal |
| `steiner_strict_parity_edge_set` | Rust edge-set exactly equals Python oracle (unordered pairs) | Tie-breaking rule changed (CRITICAL-1 fix) |
| `steiner_strict_parity_weight` | Absolute weight difference < 1e-6 | Numeric precision regression |
| `steiner_insufficient_terminals_error` | 0 and 1 terminals → `InsufficientTerminals` error | Panic instead of error |
| `steiner_always_connects_terminals` (proptest, 20 cases) | Connectivity + tree invariant for any 2-5 terminal subset | Edge case terminal count |

**route_variant.rs** (T-4.4 series):

| Test name | Behavior verified | Regression caught |
|---|---|---|
| `variant_zero_is_deterministic` | Two calls same params → identical total_weight and edge_count | Non-determinism introduced |
| `variant_zero_insta_snapshot` | Insta snapshot of sorted edge list + rounded weight | Any change in edge set or weights |
| `variant_positive_connectivity_exact` | Variants [1,2,5]: all terminals still connected, tree invariant e=n-1 | Perturbation disconnects terminals |
| `variant_zero_weight_statistical_gate` | Rust weight / oracle ≤ 1.10 | Quality regression from optimization |

**k_shortest_paths.rs**:

| Test name | Behavior verified | Regression caught |
|---|---|---|
| `k_shortest_paths_oracle_parity` | Exact path order + weight within 1e-9 for all k-path oracle cases | Yen's algorithm path selection |
| `k_shortest_paths_k1_equals_dijkstra` | k=1 result matches Dijkstra shortest path | Inconsistency between algorithms |
| `k_shortest_paths_unknown_node_returns_error` | Unknown source → `Err` | Panic on missing node |

---

### 3.5 hydro-solvers

**Source**: `hydro-solvers/src/`
**Inline tests**: 1 (`lib.rs`: `workspace_member_compiles`)
**Integration tests**: 58 (sewer.rs × 7, water_supply.rs × 10, conveyance.rs × 12, distribution.rs × 9, pump_station.rs × 9, intake.rs × 11, solver_trait.rs × 4; includes one conveyance trait roundtrip counted in conveyance)

Note: The integration test counts above include the "solve_via_trait_roundtrip" tests, which are named in each solver-specific file.

#### 3.5.1 Inline tests

| Test name | Behavior verified | Regression caught |
|---|---|---|
| `workspace_member_compiles` | Crate compiles and links | Build breakage |

#### 3.5.2 Integration tests — solver_trait.rs (T-5.1a series)

| Test name | Behavior verified | Regression caught |
|---|---|---|
| `mock_solver_implements_trait` | `MockSolver.solve()` returns `Ok(empty)`, `constraints.min_slope` accessible | Trait signature change |
| `mock_solver_evaluate_returns_default_score` | `score.total_cost=0.0`, `norm_violations=0` | Score struct field removed |
| `solver_error_variants_are_constructible` | All 7 `SolverError` variants construct without panic | Variant renamed or removed |
| `solver_params_default_is_sane` | `route_variant=0`, `slope_factor=1.0`, `cover_factor=1.0`, `diameter_offset=0`, `manhole_spacing=None` | Default value drift |

#### 3.5.3 Integration tests — sewer.rs (T-5.2 series)

| Test name | Behavior verified | Regression caught |
|---|---|---|
| `sewer_evaluate_formula_exact` | `w_length=1.0, w_excav=2.0, w_violations=100`; result within 1e-6 of oracle | Weight constant changed |
| `sewer_score_formula_exact` | Cost formula + avg_excavation detail | Score field regression |
| `sewer_network_topology_structural` | node_count EXACT (CRITICAL-1 Steiner tie-break fix), pipe_count EXACT, gravity invert continuity, min_cover respected | Steiner tie-break reverted |
| `sewer_invert_and_cover_domain_invariants` | Deterministic defaults; invert/cover invariants hold | Default param drift |
| `sewer_solve_end_to_end_parity` | node_count EXACT, pipe_count EXACT, cost within 1e-4, all oracle node IDs present | Node identity regression |
| `sewer_solve_via_trait_roundtrip` | Happy path `Ok`; empty service_points → `Err(InsufficientTerminals)` | Error not propagated |

#### 3.5.4 Integration tests — water_supply.rs (T-5.3a series)

| Test name | Behavior verified | Regression caught |
|---|---|---|
| `ws_evaluate_formula_exact` | `w_length=1.0, w_excav=1.5, w_violations=150`; within 1e-6 | Weight constant changed |
| `ws_cost_formula_constants` | Fixture formula weights match implementation | Weight table drift |
| `ws_pump_count_is_always_zero` | `evaluate()` always returns `pump_count=0` | Pump count accidentally set |
| `ws_solve_end_to_end_parity` | MST produces same spanning tree; node/pipe counts EXACT, cost within 1e-4 | MST algorithm changed |
| `ws_domain_invariants` | 1 TANK, SERVICE nodes ≥ demand_points count, all pipes > 0, all non-source nodes have pressure_mca | Network invariant broken |
| `ws_network_structure_matches_oracle` | node_count, pipe_count, total_length within 1e-4 | Network building regression |
| `ws_alternatives_yen_integration` | 2 alternatives, strictly ascending ≥ 1e-3, distinct routes, costs within 1e-4, correct rank numbers | Alternative generation broken |
| `ws_pipe_metadata_flow_roundtrip` | All pipes have `flow_m3s > 0` in metadata | Flow assignment skipped |
| `ws_node_pressure_parity` | Source pressure = source_head exactly; all nodes have `pressure_mca` | Pressure propagation broken |
| `water_supply_solve_via_trait_roundtrip` | Happy path `Ok`; empty demand_points → `Err` | Error not propagated |

#### 3.5.5 Integration tests — conveyance.rs (T-5.3b series)

| Test name | Behavior verified | Regression caught |
|---|---|---|
| `conv_evaluate_formula_exact` | Oracle-reconstructed network → exact oracle score (`w_length=1.5, w_excav=2.0, w_viol=200, w_struct=500`) | Weight constant changed |
| `conv_cost_formula_constants` | Fixture weights match implementation | Weight table drift |
| `conv_pump_count_is_always_zero` | `evaluate()` always returns `pump_count=0` | Pump count accidentally set |
| `conv_solve_primary_end_to_end_parity` | node_count EXACT, pipe_count EXACT, total_length within 1e-4, cost within 1e-4 | Network building regression |
| `conv_primary_structure_count_is_zero` | Monotonic terrain → no air/blowoff valves | Spurious valve placement |
| `conv_per_pipe_diameter_and_inverts` | Diameter EXACT equality, inverts within 1e-9 | Diameter selection regression |
| `conv_per_node_pressure_parity` | Pressure_mca at each node within 1e-6 | Pressure calculation regression |
| `conv_domain_invariants` | 1 TANK, 1 RESERVOIR, all pipes length > 0, all nodes have pressure_mca | Network invariant broken |
| `conv_valve_placement_structure_count` | Peak/valley terrain → valve nodes with correct valve_type metadata | Valve placement logic removed |
| `conv_valve_evaluate_parity` | Valve fixture oracle score exact | Valve scoring regression |
| `conv_alternatives_yen_integration` | 2 alternatives, pairwise-distinct costs, sorted ascending, each within 1e-4 of oracle | Alternative generation broken |
| `conveyance_solve_via_trait_roundtrip` | Happy path `Ok`; source==destination → `Err` | Same-node edge case panic |

#### 3.5.6 Integration tests — distribution.rs (T-5.4a series)

| Test name | Behavior verified | Regression caught |
|---|---|---|
| `distrib_evaluate_formula_exact` | `w_length=1.0, w_excav=1.5, w_viol=150, w_pressure_std=5.0, w_valve=500, w_hydrant=800` | Weight constant changed |
| `distrib_cost_formula_constants` | Fixture weights match | Weight table drift |
| `distrib_pump_count_is_always_zero` | `pump_count=0` always | Pump count accidentally set |
| `distrib_solve_end_to_end_parity` | node_count EXACT, pipe_count EXACT, cost within 1e-4, valve/hydrant counts exact, violations exact | Network building regression |
| `distrib_valves_saturated_ordering_trap` | `valve_spacing=100` → all junctions become valves before hydrant pass → `hydrant_count=0` | Ordering trap bypassed |
| `distrib_alternatives_pairwise_distinct` | 3 alternatives, pairwise-distinct ≥ 1e-3 gap, costs within 1e-4, node/pipe counts per-alternative | Alternative generation broken |
| `distrib_domain_invariants` | 1 TANK, all pipes > 0, source pressure = source_head, all nodes have pressure_mca | Network invariant broken |
| `distrib_valves_saturated_evaluate_only` | Valve-saturated network evaluate-only scoring | Evaluate path regression |
| `distribution_solve_via_trait_roundtrip` | Happy path `Ok`; single demand point → `Err` | Single-point edge case panic |

#### 3.5.7 Integration tests — pump_station.rs (T-5.4b series)

| Test name | Behavior verified | Regression caught |
|---|---|---|
| `pump_station_golden_loads_and_has_correct_shape` | Fixture schema_version=1, 5 nodes, 4 pipes | Fixture schema change |
| `pump_station_evaluate_formula_exact` | `w_equipment=1200, w_civil=800, w_pipe=150, w_violations=5000` | Weight constant changed |
| `pump_station_cost_formula_constants` | Fixture weights match | Weight table drift |
| `pump_station_solve_end_to_end_parity` | TDH within 1e-6, power_kw EXACT (motor rounding), efficiency within 1e-6, wet_well dims within 1e-6, cost within 1e-4, pump_count EXACT | TDH/power formula regression |
| `pump_station_network_layout_invariants` | Inlet/WetWell/PumpManifold/DischargeManifold/Outlet nodes; inverts=z-0.5 | Node type assignment changed |
| `pump_station_alternatives_pairwise_distinct` | 3 alternatives, sorted ascending ≥ 1e-3 | Alternative generation broken |
| `pump_station_tdh_abs_static_lift_trap` | TDH uses `abs(static_lift)` — negative static lift gives same TDH as positive | Sign convention regression |
| `pump_station_solver_trait_default_errors` | `Solver::solve` → `Ok` | Trait dispatch broken |
| `pump_station_solve_via_trait_roundtrip` | Scenario A happy path; Scenario B zero design_flow must not return `Ok(vec![])` | Zero-flow edge case accepted |

#### 3.5.8 Integration tests — intake.rs (T-5.4c series)

| Test name | Behavior verified | Regression caught |
|---|---|---|
| `intake_golden_loads_and_has_correct_shape` | schema_version=1, node_count=6, pipe_count=5 | Fixture schema change |
| `intake_evaluate_formula_exact` | `w_concrete=600, w_pipe=120, w_violations=3000` | Weight constant changed |
| `intake_cost_formula_constants` | Fixture weights match | Weight table drift |
| `intake_solve_end_to_end_parity` | Screen dims within 1e-6, channel depth within 1e-4, weir head/length within 1e-6, pipe diameter EXACT, cost within 1e-4, violations EXACT, pump_count=0 | End-to-end parity regression |
| `intake_network_layout_invariants` | 6 nodes exact, 5 pipes exact; node IDs (Source/Screen/ChannelIn/ChannelOut/Weir/PipeStart); inverts=z-0.2 | Node topology changed |
| `intake_channel_velocity_is_rounded_in_metadata` | `velocity_m_s` exactly 3 dp rounded (=0.832) | Rounding removed |
| `intake_rectangular_weir_formula` | crest_length and head match oracle within 1e-6; weir_type="rectangular" | Weir formula regression |
| `intake_v_notch_weir_formula` | crest_length=0.0, angle=90.0, head matches oracle | V-notch formula regression |
| `intake_alternatives_pairwise_distinct` | 3 alternatives, sorted ascending ≥ 1e-3, costs within 1e-4 | Alternative generation broken |
| `intake_solver_trait_default_errors` | `Solver::solve` with valid params → `Ok` | Trait dispatch broken |
| `intake_solve_via_trait_roundtrip` | Happy path `Ok`; unknown source_type → `Err` | Unknown source type panic |

---

### 3.6 hydro-optimizer

**Source**: `hydro-optimizer/src/`
**Inline tests**: ~101 (operators.rs × 24, optimizer.rs × 9, config.rs × 18, constraints.rs × 24, encoding.rs × 15, diagnostics.rs × 3, objective.rs × 12, results.rs × 4, metrics.rs × 7, nsga3/ and routing/ modules × ~variable, lib.rs × 1)
**Integration tests**: 8 (solver_wiring_smoke.rs × 6, pr8f_parity_skeleton.rs × 2 [both ignored])

#### 3.6.1 Inline tests — operators.rs (~24 tests)

| Test name | Behavior verified | Regression caught |
|---|---|---|
| `test_adaptive_eta_at_gen_0_equals_eta_min` | η at generation 0 = η_min | Eta schedule offset |
| `test_adaptive_eta_at_gen_max_equals_eta_max` | η at last generation = η_max | Schedule endpoint wrong |
| `test_adaptive_eta_at_midpoint_equals_midpoint_value` | η linearly interpolated at midpoint | Non-linear schedule introduced |
| `test_adaptive_eta_max_gen_zero_does_not_panic` | max_gen=0 → no division by zero | Panic on zero generation count |
| `test_adaptive_eta_monotone_increasing` | η monotonically increases generation by generation | Schedule inversion |
| `test_sbx_offspring_within_bounds` | SBX offspring genes within `[lower, upper]` | Bounds violation in crossover |
| `test_sbx_invalidates_fitness` | SBX marks offspring fitness as `None` | Stale fitness cached |
| `test_sbx_identical_parents_unchanged` | Identical parents → identical offspring | Perturbation applied to identical pair |
| `test_sbx_deterministic_with_same_seed` | Same RNG seed → identical SBX result | Non-determinism introduced |
| `test_poly_mutation_offspring_within_bounds` | Polynomial mutation stays within bounds | Bounds violation in mutation |
| `test_poly_mutation_deterministic_with_same_seed` | Same seed → identical mutation | Non-determinism |
| `test_poly_mutation_invalidates_fitness_when_mutated` | Mutated individual has `fitness=None` | Stale fitness |
| `test_var_or_returns_exactly_lambda_offspring` | `var_or` returns exactly λ offspring | Lambda count off by one |
| `test_var_or_offspring_genes_within_bounds` | All offspring genes within bounds | Bounds violation |
| `test_var_or_deterministic_same_seed` | Same seed → identical `var_or` output | Non-determinism |
| `test_var_or_lambda_1` | λ=1 works | Edge case on single offspring |
| `test_var_or_pure_reproduction_cxpb_0_mutpb_0` | p_cx=0 p_mut=0 → pure copy | Operator applied when disabled |
| `test_init_population_returns_correct_size` | Population size correct | Off-by-one |
| `test_init_population_genes_within_bounds` | All initial genes within spec bounds | Initialization out of bounds |
| `test_init_population_fitness_is_none` | All initial individuals have `fitness=None` | Pre-evaluated fitness cached |
| `test_init_population_deterministic` | Same seed → same population | Non-determinism |
| `test_init_population_all_solver_types` | Initialization works for all 6 solver types | Encoding missing a solver type |
| `test_sewer_has_integer_genes` | Sewer chromosome has integer-typed genes | Gene type changed to float |
| `test_sbx_bounds_respected_for_integer_gene_positions` | SBX respects integer bounds | Integer gene treated as float |

#### 3.6.2 Inline tests — optimizer.rs (~9 tests)

| Test name | Behavior verified | Regression caught |
|---|---|---|
| `test_pareto_front_evicts_dominated` | Adding dominated solution removes it from front | Dominated solution retained |
| `test_pareto_front_retains_non_dominated` | Non-dominated solutions all retained | Solution dropped from front |
| `test_pareto_front_rejects_dominated` | New dominated solution not inserted | Dominated solution inserted |
| `test_pareto_front_equal_objectives_not_dominated` | Equal objectives → neither dominates | Equality treated as dominance |
| `test_pareto_front_empty_accepts_first` | Empty front accepts first solution | First solution rejected |
| `test_pareto_front_chain_dominance` | Chain: A dom B, B dom C → front={A} | Transitivity not applied |
| `test_dominates_5_strict_dominance` | Strict dominance with 5 objectives | 5-objective case regression |
| `test_dominates_5_equal_not_dominated` | Equal objectives in 5-objective case | Equal treated as dominated |
| `test_dominates_5_one_obj_worse_not_dominated` | One objective worse → not dominated | One-objective worse ignored |

#### 3.6.3 Inline tests — config.rs (~18 tests)

| Test name | Behavior verified | Regression caught |
|---|---|---|
| `test_config_default_is_valid` | `OptimizationConfig::default().validate()` passes | Default value out of bounds |
| `test_config_serde_roundtrip` | Config serializes/deserializes | Field rename |
| `test_config_invalid_population_rejected` | population=0 → `Err` | Lower-bound check removed |
| `test_config_rejects_generations_zero` | generations=0 → `Err` | Same |
| `test_config_rejects_crossover_below_zero` | p_crossover < 0 → `Err` | Range check removed |
| `test_config_rejects_crossover_above_one` | p_crossover > 1 → `Err` | Same |
| `test_config_rejects_mutation_below_zero` | p_mutation < 0 → `Err` | Same |
| `test_config_rejects_mutation_above_one` | p_mutation > 1 → `Err` | Same |
| `test_config_rejects_max_time_nonpositive` | max_time ≤ 0 → `Err` | Non-positive time accepted |
| `test_config_rejects_eta_min_nonpositive` | η_min ≤ 0 → `Err` | Non-positive η accepted |
| `test_config_rejects_eta_min_greater_than_eta_max` | η_min > η_max → `Err` | Cross-field check removed |
| `test_config_rejects_negative_weight` | Negative objective weight → `Err` | Negative weight accepted |
| `test_rng_root_produces_reproducible_sequence` | Same seed → same RNG sequence | RNG algorithm changed |
| `test_child_rng_diverges_for_different_indices` | Different worker indices → different RNG streams | Worker streams collide |
| `test_error_display_messages` | `OptimizationError::InvalidConfig` display string | Error message format changed |
| `test_error_solver_variant_display` | `OptimizationError::Solver(_)` display | Same |
| `test_error_time_budget_exceeded_display` | `OptimizationError::TimeBudgetExceeded` display | Same |
| `test_error_internal_variant_display` | `OptimizationError::Internal` display | Same |

#### 3.6.4 Inline tests — constraints.rs (~24 tests)

| Test name | Behavior verified | Regression caught |
|---|---|---|
| `test_feasible_candidate_no_penalty` | Fully feasible candidate → penalty=0 | False penalty applied |
| `test_single_violation_spec_penalty_formula` | Single violation → penalty per spec formula | Penalty formula changed |
| `test_multiple_violations_penalty_sums` | Multiple violations → penalties sum | Penalties not summed |
| `test_violation_magnitude_is_correct` | Magnitude calculation correct | Magnitude formula regression |
| `test_bounds_below_lower_bound_infeasible` | Gene below lower bound → infeasible | Bound check missing |
| `test_bounds_above_upper_bound_infeasible` | Gene above upper bound → infeasible | Same |
| `test_bounds_exact_boundary_feasible` | Gene exactly at bound → feasible | Strict vs. inclusive confusion |
| `test_bounds_no_bounds_always_feasible` | Empty bounds → always feasible | Panic on empty bounds |
| `test_forbidden_zone_pipe_intersects_is_infeasible` | Pipe intersecting forbidden zone → infeasible | Intersection check removed |
| `test_forbidden_zone_pipe_clear_is_feasible` | Clear pipe → feasible | False positive flagging |
| `test_no_forbidden_zones_always_feasible` | No forbidden zones → always feasible | Empty list panic |
| `test_mandatory_route_satisfied_is_feasible` | Mandatory route present → feasible | False mandatory route failure |
| `test_mandatory_route_missing_is_infeasible` | Missing mandatory route → infeasible | Missing route not detected |
| `test_bend_angle_violation_detected` | Bend angle > max → violation | Angle check removed |
| `test_bend_angle_straight_pipe_feasible` | Straight pipe → no bend violation | False bend violation |
| `test_cover_depth_below_min_is_infeasible` | Cover depth below minimum → infeasible | Cover check removed |
| `test_cover_depth_above_min_is_feasible` | Cover depth above minimum → feasible | False cover violation |
| `test_existing_clearance_crossing_insufficient_vertical_infeasible` | Crossing pipe insufficient vertical clearance → infeasible | Clearance check removed |
| `test_existing_clearance_parallel_sufficient_horizontal_feasible` | Parallel pipe with sufficient clearance → feasible | False clearance violation |
| `test_slope_below_min_is_infeasible` | Slope below minimum → infeasible | Slope check removed |
| `test_slope_above_max_is_infeasible` | Slope above maximum → infeasible | Max slope check removed |
| `test_adverse_slope_not_rejected_by_slope_check` | Adverse (uphill) slope allowed by slope check (pump handles it) | Adverse slope rejected |
| `test_aggregate_check_feasible_candidate` | Aggregate check on fully-feasible candidate | Aggregation regression |
| `test_aggregate_check_bounds_infeasible_skips_geometric` | Bounds infeasible → geometric checks skipped (performance) | All checks run always |
| `test_req018_feasibility_rate_at_least_10_percent` | ≥10% of random individuals are feasible (REQ-018) | Constraint over-rejection |

#### 3.6.5 Inline tests — encoding.rs (~15 tests)

| Test name | Behavior verified | Regression caught |
|---|---|---|
| `test_gene_specs_table_completeness` | All 6 solver types have gene specs | Solver type missing from table |
| `test_gene_specs_sewer_bounds` | Sewer gene bounds match spec | Bounds table changed |
| `test_random_individual_within_bounds_sewer` | Random sewer individual within bounds | Initialization out of bounds |
| `test_random_individual_within_bounds_all_types` | All 6 solver types initialize within bounds | Encoding type mismatch |
| `test_decode_integer_gene_rounds` | Integer gene decodes by rounding | Integer gene truncated |
| `test_decode_float_gene_clamps_upper` | Float gene clamped at upper bound | Over-bound value accepted |
| `test_decode_rejects_wrong_length` | Wrong chromosome length → `Err` | Wrong-length accepted |
| `test_encode_decode_roundtrip_sewer` | Sewer encode → decode → same params | Encode/decode asymmetry |
| `test_encode_decode_roundtrip_water_supply` | Water supply roundtrip | Same |
| `test_encode_decode_roundtrip_conveyance` | Conveyance roundtrip | Same |
| `test_encode_decode_roundtrip_distribution` | Distribution roundtrip | Same |
| `test_encode_decode_roundtrip_pump_station` | Pump station roundtrip | Same |
| `test_encode_decode_roundtrip_intake` | Intake roundtrip | Same |
| `test_decode_integer_gene_clamps_below_lower_bound` | Gene below lower → clamped to lower | Gene left below lower bound |
| `test_decode_integer_gene_clamps_above_upper_bound` | Gene above upper → clamped to upper | Gene left above upper bound |

#### 3.6.6 Other inline test groups (abbreviated)

**diagnostics.rs** (~3 tests): `test_optimizer_diagnostics_default_is_zero`, `test_optimizer_diagnostics_fields_writable`, `test_optimizer_diagnostics_clone_is_independent` — verify `OptimizerDiagnostics` zero-initialization, field mutation, and clone independence.

**objective.rs** (~12 tests): `test_pumping_cost_zero_pumps`, `test_pumping_cost_with_pump_stations`, `test_excavation_formula`, `test_cost_formula`, `test_interference_count_zero_existing_networks`, `test_interference_count_matches`, `test_todini_ir_deficit_zero_for_adequate_pressure`, `test_todini_ir_deficit_positive_for_insufficient_head`, `test_gravity_resilience_uses_connectivity`, `test_all_five_objectives_are_nonnegative`, `test_objective_function_is_deterministic`, `test_todini_ir_formula_matches_oracle` — verify each of the 5 NSGA-III objective components individually and collectively.

**results.rs** (~4 tests): `test_best_by_cost_returns_first`, `test_best_by_cost_returns_none_when_empty`, `test_best_balanced_uses_weights`, `test_comparison_table_length_matches_solutions` — verify result extraction and ranking utilities.

**metrics.rs** (~7 tests): `test_hv_single_point_2d`, `test_hv_two_points_2d`, `test_hv_empty_front`, `test_igdplus_perfect_match`, `test_igdplus_front_better_than_reference`, `test_igdplus_empty_front`, `test_igdplus_empty_reference` — verify hypervolume indicator and IGD+ metric computations.

**nsga3/ modules**: Multiple tests across `niching.rs` (`test_perp_distance_aligned_is_zero`, `test_perp_distance_orthogonal_refs`, `test_perp_distance_diagonal_aligned`, `test_associate_picks_nearest_ref`, etc.), `nondom_sort.rs`, `normalize.rs`, `selection.rs`, `reference_points.rs` — verify NSGA-III reference-point niching, non-dominated sorting, normalization, and selection machinery.

**routing/ modules**: Tests in `rdp.rs` (~9), `visibility.rs` (~7), `astar.rs` (~5), `mod.rs` (~7) — verify Ramer-Douglas-Peucker path simplification, visibility graph construction, A* pathfinding, and routing module integration.

**lib.rs** (1 test): `workspace_member_compiles`.

#### 3.6.7 Integration tests

**solver_wiring_smoke.rs**:

| Test name | Behavior verified | Regression caught |
|---|---|---|
| `optimizer_smoke_sewer_feasible` | GeneticOptimizer wrapping SewerSolver finds feasible individual (pop=8, gen=3, seed=42) | Sewer solver wiring broken |
| `optimizer_smoke_water_supply_feasible` | Same for WaterSupplySolver | Water supply wiring broken |
| `optimizer_smoke_conveyance_feasible` | Same for ConveyanceSolver | Conveyance wiring broken |
| `optimizer_smoke_distribution_feasible` | Same for DistributionSolver | Distribution wiring broken |
| `optimizer_smoke_pump_station_feasible` | Same for PumpStationSolver | Pump station wiring broken |
| `optimizer_smoke_intake_feasible` | Same for IntakeSolver | Intake wiring broken |

**pr8f_parity_skeleton.rs** (both `#[ignore]`):

| Test name | Status | Behavior described | Blocker |
|---|---|---|---|
| `parity_hv_igdplus_sewer_basic` | IGNORED | HV within ±10%, IGD+ within ±15% of Python oracle (REQ-016) | Oracle fixture files not yet generated |
| `parity_path_length_sewer_basic` | IGNORED | PathSmoother output length within ±2% of oracle (REQ-012) | Same |

---

### 3.7 hydro-cli

**Source**: `hydro-cli/src/`
**Inline tests**: 2 (`main.rs`: `binary_skeleton_compiles`; `lib.rs`: `lib_skeleton_compiles`)
**Integration tests**: 45 (audit_hash.rs × 4, binary_smoke.rs × 3, cli_error_exit_codes.rs × 1, conveyance_happy_path.rs × 1, distribution_happy_path.rs × 1, intake_happy_path.rs × 1, mapping.rs × 10, pump_station_happy_path.rs × 1, sewer_happy_path.rs × 1, validate_only.rs × 4, validate_request.rs × 23, water_supply_happy_path.rs × 1)

#### 3.7.1 Inline tests

| Test name | Behavior verified | Regression caught |
|---|---|---|
| `binary_skeleton_compiles` | `main.rs` compiles and links | Binary build breakage |
| `lib_skeleton_compiles` | `lib.rs` compiles and links | Library build breakage |

#### 3.7.2 Integration tests — audit_hash.rs

| Test name | Behavior verified | Regression caught |
|---|---|---|
| `test_compute_audit_hash_known_value` | SHA-256 of `(request_json + "|" + seed + "|" + engine_version)` = 64 lowercase hex chars; determinism check (REQ-005) | Hash algorithm or separator changed |
| `test_run_audit_hash_non_empty` | `run()` result carries 64-char lowercase hex audit_hash | Hash field stripped from result |
| `test_audit_hash_determinism` | Two `run()` calls with same input+seed → identical audit_hash | Non-determinism in hash input |
| `test_audit_hash_seed_sensitivity` | Different seeds → different audit_hash (REQ-005) | Seed not included in hash input |

#### 3.7.3 Integration tests — binary_smoke.rs

| Test name | Behavior verified | Regression caught |
|---|---|---|
| `test_binary_exits_zero_for_valid_sewer_json` | Binary exits 0; output JSON has `"success":true` and non-empty solutions | Binary wiring broken |
| `test_binary_exits_one_for_malformed_json_stdin` | Malformed JSON → exit 1 | Error code regression |
| `test_binary_exits_one_validate_only_for_bad_request` | `--validate-only` with missing outlet → exit 1 (REQ-006, REQ-009) | Validate-only flag ignored |

#### 3.7.4 Integration tests — cli_error_exit_codes.rs

| Test name | Behavior verified | Regression caught |
|---|---|---|
| `test_cli_error_exit_codes` | All 4 `CliError` variants map to correct exit codes: ValidationError=1, NoFeasibleSolution=2, NormComplianceFailed=3, InternalError=4; tests `From<serde_json::Error>`, `From<HydroTypesError>`, `From<OptimizationError::AllInfeasible>`, etc. (REQ-003) | Exit code mapping changed |

#### 3.7.5 Integration tests — happy-path tests (6 tests)

| Test name | Solver | Assertions |
|---|---|---|
| `test_run_sewer_dispatch` | SewerSolver | success=true, solutions non-empty, total_cost>0, elapsed>0, audit_hash 64-char hex (REQ-001, REQ-004, WU-8) |
| `test_run_water_supply_dispatch` | WaterSupplySolver | Same assertions; fixture uses demand_points + source + source_head (REQ-001, REQ-004) |
| `test_run_conveyance_dispatch` | ConveyanceSolver | Same assertions; source + outlet (REQ-001, REQ-004) |
| `test_run_distribution_dispatch` | DistributionSolver | Same assertions; fixture has ≥2 demand points (REQ-001, REQ-004) |
| `test_run_pump_station_dispatch` | PumpStationSolver | Same assertions; synthetic 5×5 linear terrain for elevation lookup (REQ-001, REQ-004) |
| `test_run_intake_dispatch` | IntakeSolver | Same assertions; synthetic flat terrain at source_elevation (REQ-001, REQ-004) |

All 6 tests use GA budget floor: `nsga_population_size=20, nsga_generations=10, nsga_max_time_seconds=30`. This verifies `run()` dispatches correctly to each solver and produces a valid `DesignResult` through the full public API path.

#### 3.7.6 Integration tests — mapping.rs (~10 tests)

| Test name | Behavior verified | Regression caught |
|---|---|---|
| `test_build_terrain_model_happy_path` | 5 terrain points → `Ok`, `point_count==5` | Terrain build regression |
| `test_build_terrain_model_empty_returns_err` | (Note: 3-point terrain still succeeds; tests compile path) | Compile path regression |
| `test_build_optimization_config_seed_override_applied` | `seed_override=42` overrides `req.seed=99` | Seed override ignored |
| `test_build_optimization_config_uses_req_seed_when_no_override` | No override → uses `req.seed=99` | Request seed ignored |
| `test_build_optimization_config_defaults_seed_to_42` | No seed anywhere → default 42 | Default seed changed |
| `test_build_optimization_config_maps_population_size` | `nsga_population_size=20` → `cfg.population_size=20` | Field mapping wrong |
| `test_build_optimization_config_maps_generations` | `nsga_generations=10` → `cfg.generations=10` | Same |
| `test_build_optimization_config_maps_max_time` | `nsga_max_time_seconds=30` → `cfg.max_time_seconds=30.0` | i32-to-f64 cast error |
| `test_build_solver_params_design_flow` | `flow_per_service` → `params.design_flow` | Field renamed |
| `test_build_solver_params_grid_resolution` | `grid_resolution` → `params.grid_resolution` | Field renamed (REQ-001, REQ-005) |

#### 3.7.7 Integration tests — validate_only.rs (4 tests)

| Test name | Behavior verified | Regression caught |
|---|---|---|
| `test_validate_only_exits_zero_and_emits_valid_status_json` | Binary exits 0; stdout = `{"status":"valid","project_type":"sewer"}` | Status JSON format changed |
| `test_validate_only_exits_one_and_emits_invalid_status_json` | Exits 1; stdout has `"status":"invalid"` + `"error"` key | Error JSON format changed |
| `test_validate_only_completes_under_100ms_valid` | In-process: JSON parse + validate_request + status JSON emit < 50 ms (REQ-007) | Validation path performance regression |
| `test_validate_only_completes_under_100ms_invalid` | Error path also < 50 ms (REQ-007) | Error path performance regression |

#### 3.7.8 Integration tests — validate_request.rs (23 tests)

Happy-path group (6 tests — one per project type):
`test_validate_accepts_valid_sewer_request`, `test_validate_accepts_valid_water_supply_request`, `test_validate_accepts_valid_conveyance_request`, `test_validate_accepts_valid_distribution_request`, `test_validate_accepts_valid_pump_station_request`, `test_validate_accepts_valid_intake_request` — each asserts `validate_request(&req).is_ok()` for a minimal valid request of each type.

Error-path group (17 tests):

| Test name | Behavior verified | exit_code asserted |
|---|---|---|
| `test_validate_rejects_sewer_without_outlet` | Missing outlet → error | 1 |
| `test_validate_rejects_sewer_without_service_points` | Missing service_points → error | 1 |
| `test_validate_rejects_sewer_with_empty_service_points` | Empty service_points → error | 1 |
| `test_validate_rejects_water_supply_without_source` | Missing source → error | 1 |
| `test_validate_rejects_water_supply_without_service_points` | Missing service_points → error | 1 |
| `test_validate_rejects_conveyance_without_source` | Missing source → error | 1 |
| `test_validate_rejects_conveyance_without_destination` | Missing outlet/destination → error | 1 |
| `test_validate_rejects_distribution_with_one_point` | Only 1 demand point → error (rule: ≥2) | 1 |
| `test_validate_rejects_distribution_with_zero_demand_points` | 0 demand points → error | 1 |
| `test_validate_rejects_distribution_without_source` | Missing source → error | 1 |
| `test_validate_rejects_pump_station_without_source` | Missing source → error | 1 |
| `test_validate_rejects_pump_station_without_outlet` | Missing outlet → error | 1 |
| `test_validate_rejects_intake_without_source` | Missing source → error | 1 |
| `test_validate_rejects_out_of_range_population_size` | `nsga_population_size=0` (< min 20) → error | 1 |

(Three tests from the complete 17 not listed above are additional edge-case variants for sewer, water_supply, and distribution already covered in the happy-path or consolidated above.)

---

## 4. Coverage Matrix

| Requirement | Description | Covered by tests |
|---|---|---|
| REQ-001 | Full optimization dispatch → `DesignResult` | sewer/water_supply/conveyance/distribution/pump_station/intake happy path tests; solver_wiring_smoke.rs |
| REQ-002 | `validate_request` structural + solver-specific validation | validate_request.rs (23 tests) |
| REQ-003 | Typed exit codes: 1=ValidationError, 2=NoFeasible, 3=NormFailed, 4=Internal | cli_error_exit_codes.rs; binary_smoke.rs |
| REQ-004 | `DesignResult` fields: success, solutions, elapsed_seconds | All 6 happy-path tests |
| REQ-005 | SHA-256 audit_hash of request+seed+version | audit_hash.rs (4 tests); all happy-path tests (hash length check) |
| REQ-006 | `--validate-only` CLI flag | binary_smoke.rs (`test_binary_exits_one_validate_only_for_bad_request`) |
| REQ-007 | Validate-only latency < 50 ms | validate_only.rs (`test_validate_only_completes_under_100ms_valid/invalid`) |
| REQ-009 | At least 10% of GA individuals must be feasible | constraints.rs (`test_req018_feasibility_rate_at_least_10_percent`); solver_wiring_smoke.rs |
| REQ-010 | Public API callable without subprocess | All integration tests that call `hydro_cli::run` directly |
| REQ-012 | PathSmoother output length within ±2% of oracle | pr8f_parity_skeleton.rs (`parity_path_length_sewer_basic`) — IGNORED |
| REQ-016 | HV within ±10%, IGD+ within ±15% of Python oracle | pr8f_parity_skeleton.rs (`parity_hv_igdplus_sewer_basic`) — IGNORED |
| REQ-018 | Feasibility rate ≥ 10% | constraints.rs `test_req018_feasibility_rate_at_least_10_percent` |
| WU-1 | hydro-types domain types | hydro-types inline tests (52 tests) |
| WU-2 | validate_request implementation | validate_request.rs |
| WU-3 | Mapping helpers (build_terrain_model, build_optimization_config) | mapping.rs |
| WU-4 | Sewer solver integration | sewer.rs; sewer_happy_path.rs |
| WU-5 | All 6 solver dispatches from run() | 6 happy-path tests |
| WU-6 | NSGA-III operators and encoding | operators.rs; encoding.rs; constraints.rs |
| WU-7 | Oracle parity for all 6 solvers | T-5.x integration test series |
| WU-8 | SHA-256 audit hash (64-char lowercase hex) | audit_hash.rs; all happy-path audit_hash assertions |

---

## 5. Gaps and Risks

**Gap 1 — REQ-012 and REQ-016 not executed (HIGH risk)**
`hydro-optimizer/tests/pr8f_parity_skeleton.rs` contains the two tests for HV/IGD+ parity and PathSmoother length parity, both marked `#[ignore]` with note "fixture-missing". Until these fixtures are generated and the tests un-ignored, there is no automated validation that the Pareto-front quality (HV, IGD+) and path smoothing output match the Python oracle within the specified tolerances. Any regression in `PathSmoother` or the HV/IGD+ computation would go undetected.

**Gap 2 — No `nsga_num_workers` concurrency test (MEDIUM risk)**
`DesignRequest.nsga_num_workers` is accepted and mapped but no test exercises multi-threaded execution (values > 1). Concurrency-related non-determinism or race conditions are untested.

**Gap 3 — No norm-strict-compliance happy path test (MEDIUM risk)**
`strict_norm_compliance: true` is never used in the happy-path integration tests (all 6 set it to `false`). The `NormComplianceFailed` exit code (exit 3) is only tested synthetically in `cli_error_exit_codes.rs` via `From<OptimizationError>` conversion, not through a full `run()` call with strict mode enabled.

**Gap 4 — Binary smoke tests require compiled binary (MEDIUM risk)**
`binary_smoke.rs` spawns the `hydro-cli` binary via `std::process::Command`. This test will fail in CI if the binary has not been built before the test suite runs. No Cargo integration (e.g., `cargo build` step or `build.rs`) ensures the binary exists. If tests are run with `cargo test --lib` only, these 3 tests are silently excluded.

**Gap 5 — `forbidden_zones` and `mandatory_routes` geometry (LOW risk)**
The integration tests for `forbidden_zones` and `mandatory_routes` are limited to the `hydro-optimizer/src/constraints.rs` unit tests with synthetic geometry. There is no end-to-end integration test that passes non-empty `forbidden_zones` or `mandatory_routes` through `run()` and asserts they affect the produced network.

**Gap 6 — `project_crs` field untested (LOW risk)**
`DesignRequest.project_crs` is declared and round-trips through serde but there are no tests verifying it is used or propagated into the output (it is carried as metadata). If it is ever used for coordinate projection, that code path is uncovered.

**Gap 7 — Alternative route quality not tested for sewer (LOW risk)**
`sewer_solve_via_trait_roundtrip` only tests the happy path and the empty-service-points error. The alternatives path (`num_alternatives > 1`) for the sewer solver is not covered with oracle parity assertions, unlike water_supply, conveyance, distribution, pump_station, and intake which all have `*_alternatives_*` tests.