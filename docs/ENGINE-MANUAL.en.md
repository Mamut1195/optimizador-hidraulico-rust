# Engine Manual — `optimizador-hidraulico-rust`

**Reference version**: v1 engine boundary (main at `203c5c1`)
**Binary**: `hydro-cli`
**Contract**: JSON in / JSON out
**Audience**: integrators, downstream consumers, future maintainers, AI agents

---

## Table of contents

1. [What this engine is](#1-what-this-engine-is)
2. [Build & install](#2-build--install)
3. [Invocation patterns](#3-invocation-patterns)
4. [CLI flag reference](#4-cli-flag-reference)
5. [Exit codes](#5-exit-codes)
6. [Input contract — `DesignRequest`](#6-input-contract--designrequest)
7. [Output contract — `DesignResult`](#7-output-contract--designresult)
8. [Enums reference](#8-enums-reference)
9. [Design constraints reference](#9-design-constraints-reference)
10. [Validation rules](#10-validation-rules)
11. [Determinism & reproducibility](#11-determinism--reproducibility)
12. [Worked examples by project type](#12-worked-examples-by-project-type)
13. [Integration patterns](#13-integration-patterns)
14. [Troubleshooting](#14-troubleshooting)

---

## 1. What this engine is

A standalone hydraulic optimization engine that:

- Takes a single `DesignRequest` JSON document describing terrain, demand,
  constraints, and optimizer settings.
- Runs an NSGA-III multi-objective genetic optimizer (5 objectives:
  cost, excavation, pumping, interference, resilience) against the appropriate
  domain solver (sewer, water supply, conveyance, distribution, pump station,
  or intake).
- Returns a `DesignResult` JSON document with ranked Pareto-optimal pipe
  networks, scores, diagnostics, and an audit hash.

The engine is **a binary, not a library**. The CLI is the only stable contract.
Any downstream consumer — REST server, GUI, Civil 3D plugin, automation script —
talks to it through stdin/stdout or files.

Out of scope for v1: WASM build, C-ABI FFI, embedded HTTP server, EPANET/SWMM
interop, GUI. Each of these would be a future phase that wraps this binary.

---

## 2. Build & install

```bash
# Clone & build (release)
git clone <repo>
cd optimizador-hidraulico-rust
cargo build --release -p hydro-cli

# The binary lands at:
# Windows:   target/release/hydro-cli.exe
# Unix-like: target/release/hydro-cli
```

Rust toolchain: **1.95.0** (pinned in CI). Workspace deps include `petgraph`,
`rayon`, `clap`, `serde_json`, `sha2`. The release binary is a single file
with no runtime dependencies.

Verify:

```bash
./target/release/hydro-cli --version
./target/release/hydro-cli --help
```

---

## 3. Invocation patterns

### Files (most common)

```bash
hydro-cli --input request.json --output result.json
hydro-cli -i request.json -o result.json
```

### Pipes (scripting / unix composition)

```bash
cat request.json | hydro-cli > result.json
hydro-cli < request.json > result.json
```

### Mixed

```bash
hydro-cli --input request.json > result.json     # input from file, output to stdout
cat request.json | hydro-cli --output result.json # input from stdin, output to file
```

### Validation only (no optimization)

```bash
hydro-cli --input request.json --validate-only
# stdout: {"status":"valid","project_type":"sewer"} on success
# stdout: {"status":"invalid","error":"..."} on failure
# Wall time: <50 ms (no GA, no solver, no terrain model)
```

### Deterministic run (reproducible)

```bash
hydro-cli --input request.json --output result.json --seed 42
# Or pass seed in the request body; --seed overrides if both are present.
```

### Human-readable output

```bash
hydro-cli --input request.json --output result.json --pretty
```

---

## 4. CLI flag reference

| Flag                | Short | Type     | Default  | Behavior                                                            |
| ------------------- | ----- | -------- | -------- | ------------------------------------------------------------------- |
| `--input <PATH>`    | `-i`  | PathBuf  | (stdin)  | Read `DesignRequest` JSON from this file. Absent → read stdin.      |
| `--output <PATH>`   | `-o`  | PathBuf  | (stdout) | Write `DesignResult` JSON to this file. Absent → write stdout.      |
| `--seed <N>`        |       | u64      | none     | Override `DesignRequest.seed` before running the optimizer.         |
| `--pretty`          |       | flag     | off      | Pretty-print the JSON output (2-space indent). Default: compact.    |
| `--validate-only`   |       | flag     | off      | Run `validate_request()` only. Skip optimizer. Emits short status.  |
| `--help` / `-h`     |       | flag     | —        | Print help and exit 0.                                              |
| `--version` / `-V`  |       | flag     | —        | Print version and exit 0.                                           |

I/O errors (file-not-found, permission denied, broken pipe on stdout) exit with
code **4**, with a message on stderr. Engine-level errors are routed through
`CliError` and surface as exit codes 1–4 (see next section).

---

## 5. Exit codes

| Code | Meaning                                | Typical cause                                                                                            |
| ---- | -------------------------------------- | -------------------------------------------------------------------------------------------------------- |
| `0`  | Success                                | Solver completed and emitted at least one feasible solution                                              |
| `1`  | Validation error                       | Malformed JSON, missing required field, value out of range, unknown enum variant, cross-field violation  |
| `2`  | No feasible solution                   | GA finished but every candidate violated a hard constraint                                               |
| `3`  | No norm-compliant solution             | At least one feasible solution exists, but none passes the active norm with `strict_norm_compliance: true` |
| `4`  | Internal engine error                  | I/O failure, serialization failure, solver panic, terrain model construction error                       |

Exit code 0 plus `result.success == true` is the only "all good" signal.
Anything else, treat as a failure.

---

## 6. Input contract — `DesignRequest`

The full schema lives at `hydro-types/src/request.rs`. Strict deserialization
(`#[serde(deny_unknown_fields)]`) — any extra field fails with exit 1.

### Minimum valid request

```json
{
  "project_type": "sewer",
  "terrain_points": [
    { "x": 0.0, "y": 0.0, "z": 10.0 },
    { "x": 10.0, "y": 0.0, "z": 9.0 },
    { "x": 10.0, "y": 10.0, "z": 8.5 }
  ]
}
```

Everything else has a default.

### Required fields

| Field            | Type                    | Constraint                                                            |
| ---------------- | ----------------------- | --------------------------------------------------------------------- |
| `project_type`   | string                  | One of `sewer`, `water_supply`, `conveyance`, `distribution`, `pump_station`, `intake`. Case-insensitive on input; normalized to lowercase snake_case. |
| `terrain_points` | array of `{x, y, z}`    | At least 3 points, all coordinates finite (no NaN, no infinity)       |

### Optional fields with defaults

| Field                       | Type      | Default        | Valid range / values                            |
| --------------------------- | --------- | -------------- | ----------------------------------------------- |
| `project_name`              | string    | `"Unnamed"`    | any                                             |
| `material`                  | string    | `"PVC"`        | `PVC`, `CONCRETE`, `HDPE`, `STEEL`, `CAST_IRON` (Spanish aliases accepted — see §8) |
| `num_alternatives`          | u32       | `3`            | `[1, 10]`                                       |
| `norm`                      | string    | `"CONAGUA_MX"` | any (uppercased + underscored on input)         |
| `strict_norm_compliance`    | bool      | `true`         | when `true`, no norm-violating solution is returned (exit 3 if none compliant) |
| `flow_per_service`          | f64       | `0.002`        | `(0.0, 10.0]` m³/s                              |
| `grid_resolution`           | f64       | `20.0`         | `[1.0, 500.0]` m                                |
| `seed`                      | u64?      | `None`         | `[0, 2_147_483_647]` when present               |
| `nsga_population_size`      | u32       | `100`          | `[20, 500]`                                     |
| `nsga_generations`          | u32       | `50`           | `[10, 500]`                                     |
| `nsga_max_time_seconds`     | u32       | `300`          | `[30, 7200]`                                    |
| `nsga_num_workers`          | u32?      | `None`         | `[1, 256]` when present (rayon thread cap)      |
| `enforce_cover_depth`       | bool      | `false`        |                                                 |
| `enforce_existing_clearance`| bool      | `false`        |                                                 |
| `enforce_segment_slopes`    | bool      | `false`        |                                                 |
| `enable_path_smoothing`     | bool      | `false`        |                                                 |
| `weight_cost`               | f64       | `0.30`         | `[0.0, 1.0]`                                    |
| `weight_excavation`         | f64       | `0.25`         | `[0.0, 1.0]`                                    |
| `weight_pumping`            | f64       | `0.15`         | `[0.0, 1.0]`                                    |
| `weight_interference`       | f64       | `0.10`         | `[0.0, 1.0]`                                    |
| `weight_resilience`         | f64       | `0.20`         | `[0.0, 1.0]`                                    |
| `min_vertical_separation`   | f64       | `0.3`          | `[0.0, 10.0]` m                                 |
| `min_horizontal_separation` | f64       | `1.0`          | `[0.0, 20.0]` m                                 |
| `project_crs`               | string?   | `None`         | informational only (not used in computation)    |

### Geometric inputs

| Field               | Type                            | Required when                          | Constraint                              |
| ------------------- | ------------------------------- | -------------------------------------- | --------------------------------------- |
| `service_points`    | array of `{x, y}`               | sewer, water_supply, distribution      | demand nodes (the network must reach them) |
| `outlet`            | `{x, y}`                        | sewer, conveyance                      | discharge point                         |
| `source`            | `{x, y}`                        | water_supply, intake, conveyance       | water source location                   |
| `source_head`       | f64                             | water_supply (often), intake           | available head at source (m)            |

The optimizer derives `z` (elevation) at any `(x, y)` by interpolating
`terrain_points`. You do not have to provide it explicitly for service points,
outlets, or sources.

### Constraint inputs

| Field               | Type                              | Notes                                                          |
| ------------------- | --------------------------------- | -------------------------------------------------------------- |
| `constraints`       | object (`DesignConstraintOverrides`) | All fields optional. Merges over the per-norm defaults. See §9. |
| `forbidden_zones`   | array of polygons                 | Each polygon: `{coords: [{x,y}, ...], buffer?: f64}` — `coords` ≥ 3, `buffer ∈ [0, 200]` m |
| `mandatory_routes`  | array of routes                   | Each route: `{coords: [{x,y}, ...], corridor_width?: f64}` — `coords` ≥ 2, `corridor_width ∈ [0, 500]` m |
| `existing_networks` | array of polylines                | Each: `{coords: [{x,y}, ...], depth?: f64, setback?: f64}` — `coords` ≥ 2, `depth ∈ [0, 30]` m, `setback ∈ [0, 200]` m |

### Field-by-field: input ranges enforced by `DesignRequest::validate()`

These are bound checks; violating any of them returns exit 1 with a
`HydroTypesError::OutOfRange` diagnostic on stderr.

```
terrain_points.len()      >= 3
num_alternatives          in 1..=10
flow_per_service          in (0.0, 10.0]
grid_resolution           in [1.0, 500.0]
seed (when present)       in 0..=2_147_483_647
nsga_population_size      in 20..=500
nsga_generations          in 10..=500
nsga_max_time_seconds     in 30..=7200
nsga_num_workers (when present)  in 1..=256
weight_*                  in [0.0, 1.0]
min_vertical_separation   in [0.0, 10.0]
min_horizontal_separation in [0.0, 20.0]
forbidden_zones[*].buffer       in [0.0, 200.0]
mandatory_routes[*].corridor_width in [0.0, 500.0]
existing_networks[*].depth      in [0.0, 30.0]
existing_networks[*].setback    in [0.0, 200.0]
```

---

## 7. Output contract — `DesignResult`

Full schema at `hydro-types/src/response.rs`. The engine **never** emits
`mcp_instructions` (spec S-23).

### Top-level shape

```json
{
  "success": true,
  "project_name": "Unnamed",
  "project_type": "sewer",
  "elapsed_seconds": 12.5,
  "seed": 42,
  "norm": "CONAGUA_MX",
  "solutions": [ /* ranked array of Solution */ ],
  "best_solution": { /* alias for solutions[0] */ },
  "diagnostics": { /* see below */ }
}
```

### `Solution`

```json
{
  "rank": 1,
  "network": { /* PipeNetwork — see below */ },
  "score": {
    "total_cost": 125000.0,
    "total_length": 850.0,
    "total_excavation": 320.0,
    "norm_violations": 0,
    "pump_count": 0,
    "details": { /* per-solver opaque object, preserved for traceability */ }
  },
  "metadata": {
    "fitness": [125000.0, 320.0, 0.0, 12.5, 0.85],
    "genetic_params": { /* per-individual GA parameters */ },
    "norm_validation": { /* per-element validation results */ }
  }
}
```

- `rank` — 1-based, sorted ascending by `total_cost`. `rank == 1` is the cheapest.
- `score.total_cost` — currency units (project-defined, typically MXN or USD).
- `score.norm_violations` — count of soft/warning violations even when
  `strict_norm_compliance` allowed the solution through.
- `metadata.fitness` — the raw NSGA-III 5-objective fitness vector:
  `[cost, excavation, pumping, interference, resilience]`.

### `PipeNetwork`

```json
{
  "name": "Unnamed",
  "nodes": [
    {
      "id": "g0",
      "x": 0.0,
      "y": 0.0,
      "z": 10.0,
      "node_type": "MANHOLE",
      "rim_elevation": 10.0,
      "sump_elevation": 8.5,
      "demand": 0.0,
      "metadata": {}
    }
  ],
  "pipes": [
    {
      "id": "p-001",
      "start_node_id": "g0",
      "end_node_id": "g1",
      "length": 10.0,
      "effective_length": 10.0,
      "diameter": 0.3,
      "material": "PVC",
      "roughness": 0.009,
      "start_invert": 9.5,
      "end_invert": 8.5,
      "slope": 0.1,
      "design_flow": 0.001,
      "waypoints": [],
      "metadata": {}
    }
  ],
  "total_length": 10.0
}
```

Notes:

- `id` fields are arbitrary strings (round-trip exact through JSON — `"g12"`,
  `"Pipe - (3)"`). The engine does not enforce a numeric format.
- `node_type` uses `SCREAMING_SNAKE_CASE`: `MANHOLE`, `JUNCTION`, `OUTLET`,
  `INLET`, `PUMP`, `VALVE`, `HYDRANT`, `TANK`, `RESERVOIR`, `SERVICE`.
- `material` uses canonical UPPER strings: `PVC`, `CONCRETE`, `HDPE`, `STEEL`,
  `CAST_IRON`.
- `roughness` is Manning's `n`. Values per material in §8.
- `slope` is dimensionless; positive = downhill in the flow direction.
- `design_flow` is in m³/s.
- `waypoints` is intermediate `[x, y]` pairs for polyline routing; usually empty.
- `node.metadata` can carry keys recognized by the validator:
  - `pressure_mca: f64` — static pressure at the node in m.c.a.
- `pipe.metadata` recognized keys:
  - `bypassed_by_pump: bool` — when true, slope rules are skipped for this pipe
  - `flow_m3s: f64` — design flow for pressure velocity computation

### `Diagnostics`

```json
{
  "total_solutions": 100,
  "feasible_count": 87,
  "infeasible_count": 13,
  "pareto_count": 24,
  "norm_compliant_count": 18,
  "generations_run": 50,
  "population_size": 100,
  "elapsed_seconds": 12.5,
  "compute_backend": "rayon-native",
  "audit_hash": "a3f7b2…(64 hex chars total, lowercase)",
  "convergence": [
    { "generation": 1, "min_cost": 180000.0, "min_excavation": 400.0, "min_resilience_deficit": 0.25 },
    { "generation": 2, "min_cost": 165000.0, "min_excavation": 380.0, "min_resilience_deficit": 0.20 }
  ]
}
```

- `compute_backend` is always `"rayon-native"`.
- `audit_hash` is `SHA-256(serialized_request + "|" + seed_decimal + "|" + engine_version)`,
  emitted as a lowercase 64-character hex string. Same input + same seed +
  same engine version → identical hash. See §11.
- `convergence` contains one record per generation, useful for plotting GA
  progress.

### Validate-only output

When invoked with `--validate-only`, the engine emits a short status object
instead of a full `DesignResult`:

```json
{ "status": "valid",   "project_type": "sewer" }
{ "status": "invalid", "error":        "validation error: terrain_points must have at least 3 points" }
```

Wall time: < 50 ms on any reference fixture.

---

## 8. Enums reference

### `project_type`

| Value           | Domain                                                  |
| --------------- | ------------------------------------------------------- |
| `sewer`         | Gravity sewer collection network                        |
| `water_supply`  | Pressurized distribution to service points              |
| `conveyance`    | Long-distance pressurized transport                     |
| `distribution`  | Pressurized network with meshes / loops                 |
| `pump_station`  | Single pumping plant sizing                             |
| `intake`        | Surface water intake with screens / weir                |

Input is case-insensitive and tolerant of spaces / hyphens
(`"Water Supply"`, `"water-supply"`, `"WATER_SUPPLY"` all map to `water_supply`).
Output is always lowercase snake_case.

### `material`

| Canonical     | Spanish aliases accepted on input | Manning's `n` |
| ------------- | --------------------------------- | ------------- |
| `PVC`         | —                                 | `0.009`       |
| `HDPE`        | `PEAD`                            | `0.009`       |
| `CONCRETE`    | `CONCRETO`                        | `0.013`       |
| `STEEL`       | `ACERO`                           | `0.012`       |
| `CAST_IRON`   | `FIERRO_FUNDIDO`, `CASTIRON`      | `0.013`       |

Alias resolution is case-insensitive and normalizes spaces / hyphens to
underscores. Output is always canonical UPPER.

### `node_type` (output only)

`MANHOLE`, `JUNCTION`, `OUTLET`, `INLET`, `PUMP`, `VALVE`, `HYDRANT`,
`TANK`, `RESERVOIR`, `SERVICE`.

### Other enums (output only)

- `FlowType`: `GRAVITY`, `PRESSURE`
- `Severity` (norm validation): `hard`, `soft`, `warning` (lowercase)

---

## 9. Design constraints reference

`DesignConstraints` carry per-norm engineering defaults. You override them
through the optional `constraints` object in `DesignRequest`. All overrides
are optional; absent fields keep the per-norm default.

| Field                           | Default                                                 | Notes                              |
| ------------------------------- | ------------------------------------------------------- | ---------------------------------- |
| `min_slope`                     | `0.005`                                                 | dimensionless                      |
| `max_slope`                     | `0.05`                                                  | dimensionless                      |
| `min_cover`                     | `1.2` m                                                 | minimum earth cover                |
| `max_depth`                     | `5.0` m                                                 | maximum trench depth               |
| `min_velocity`                  | `0.6` m/s                                               | self-cleaning floor                |
| `max_velocity`                  | `5.0` m/s                                               | erosion ceiling                    |
| `min_diameter`                  | `0.2` m                                                 |                                    |
| `max_diameter`                  | `1.2` m                                                 |                                    |
| `available_diameters`           | `[0.2, 0.25, 0.3, 0.38, 0.45, 0.61, 0.76, 0.91, 1.07, 1.22]` (m) | commercial picks; must be all positive |
| `max_manhole_spacing`           | `100.0` m                                               |                                    |
| `manhole_at_direction_change`   | `true`                                                  |                                    |
| `manhole_at_slope_change`       | `true`                                                  |                                    |
| `manhole_at_diameter_change`    | `true`                                                  |                                    |
| `min_pressure`                  | `15.0` mca                                              | water supply service pressure floor |
| `max_pressure`                  | `50.0` mca                                              | service pressure ceiling           |
| `default_material`              | `"PVC"`                                                 |                                    |
| `default_roughness`             | `0.009`                                                 | Manning's `n` matching `default_material` |
| `max_bend_angle`                | `None`                                                  | degrees, `None` = unlimited        |

**Cross-field rules** (enforced on validation; violating yields exit 1 with
`HydroTypesError::CrossFieldViolation`):

- `min_slope <= max_slope`
- `min_velocity <= max_velocity`
- `min_diameter <= max_diameter`
- `min_pressure <= max_pressure`
- every entry of `available_diameters` is `> 0.0`

---

## 10. Validation rules

Two validation stages run before the GA starts:

### Stage 1 — Deserialization (serde)

| Failure mode                              | Effect            |
| ----------------------------------------- | ----------------- |
| Malformed JSON                            | exit 1            |
| Unknown field at any level                | exit 1            |
| Missing required field (`terrain_points`) | exit 1            |
| Wrong type (string where number expected) | exit 1            |
| Unknown `project_type` variant            | exit 1            |
| Unknown `material` variant or alias       | exit 1            |

### Stage 2 — Semantic validation (`DesignRequest::validate()`)

| Failure mode                                            | Error variant           | Effect |
| ------------------------------------------------------- | ----------------------- | ------ |
| `terrain_points.len() < 3`                              | `MissingRequired`       | exit 1 |
| Any `terrain_points[*]` coordinate non-finite           | `OutOfRange`            | exit 1 |
| Numeric field out of its documented range               | `OutOfRange`            | exit 1 |
| Cross-field constraint violated                         | `CrossFieldViolation`   | exit 1 |
| `available_diameters` contains non-positive value       | `InvalidDiameter`       | exit 1 |
| Polygon / route / network with too few coordinates      | `TooFewCoords`          | exit 1 |

`--validate-only` runs both stages and exits, never proceeding to the GA.

### Stage 3 — Engine runtime (during/after GA)

| Failure mode                            | Effect                          |
| --------------------------------------- | ------------------------------- |
| No feasible solution found              | exit 2, `result.success == false` |
| Feasible solutions found, none norm-compliant + `strict_norm_compliance == true` | exit 3 |
| Solver panic, terrain construction error, internal serialization failure | exit 4 |

---

## 11. Determinism & reproducibility

The engine is deterministic when the seed is fixed:

```
seed_resolution = --seed CLI flag (if set) > request.seed (if set) > random
audit_hash      = lowercase_hex(SHA-256(
                    serde_json::to_string(&request)
                    || "|"
                    || seed_decimal_string
                    || "|"
                    || engine_version_string
                  ))
```

Properties:

- Same `request` JSON + same `seed` + same engine version → identical
  `result.solutions` (byte-for-byte under JSON round-trip).
- The hash is sensitive to seed changes: flipping the seed changes the hash.
- The hash is sensitive to engine version (`CARGO_PKG_VERSION`); upgrading
  the binary intentionally changes the hash even with identical inputs.

Use the hash to:

- Tag stored result documents (audit trail).
- Detect input drift in regression tests (input changed → new hash).
- Pin a "known-good run" for engineering sign-off.

The engine's parallel work runs on `rayon`. The seed propagates into the
GA's RNG, and the deterministic graph layer (introduced in Phase 7.5)
guarantees byte-identical topological order across runs.

---

## 12. Worked examples by project type

### 12.1 Sewer (gravity collection)

```json
{
  "project_type": "sewer",
  "project_name": "Red Norte Tlaxcala",
  "terrain_points": [
    { "x": 0.0,   "y": 0.0,   "z": 100.0 },
    { "x": 100.0, "y": 0.0,   "z": 99.0  },
    { "x": 200.0, "y": 0.0,   "z": 98.0  },
    { "x": 0.0,   "y": 100.0, "z": 100.5 },
    { "x": 100.0, "y": 100.0, "z": 99.5  },
    { "x": 200.0, "y": 100.0, "z": 98.5  }
  ],
  "service_points": [
    { "x": 50.0,  "y": 50.0 },
    { "x": 150.0, "y": 50.0 }
  ],
  "outlet": { "x": 200.0, "y": 0.0 },
  "flow_per_service": 0.002,
  "seed": 42
}
```

Required inputs: `terrain_points`, `service_points`, `outlet`.
Sewer designs gravity-flow networks routed from each service point to the
outlet, respecting cover depth and minimum velocity.

### 12.2 Water supply

```json
{
  "project_type": "water_supply",
  "project_name": "Distribución sur",
  "terrain_points": [ /* ≥3 finite (x,y,z) */ ],
  "service_points": [ { "x": 50.0, "y": 50.0 } ],
  "source":         { "x": 0.0,  "y": 0.0 },
  "source_head":    35.0,
  "flow_per_service": 0.001,
  "constraints": { "min_pressure": 12.0, "max_pressure": 45.0 },
  "seed": 42
}
```

Required: `terrain_points`, `service_points`, `source`, ideally `source_head`.
Designs a pressurized branch network from the source to each service point,
respecting `min_pressure` / `max_pressure` and `min_velocity` / `max_velocity`.

### 12.3 Distribution (looped / meshed)

```json
{
  "project_type": "distribution",
  "terrain_points": [ /* ≥3 */ ],
  "service_points": [
    { "x": 50.0,  "y": 50.0 },
    { "x": 150.0, "y": 50.0 }
  ],
  "source":      { "x": 0.0, "y": 0.0 },
  "source_head": 30.0,
  "seed": 42
}
```

Requires **at least 2 service points** (mesh topology pre-check). Builds a
looped network (Kruskal MST + extra loop edges) for redundancy.

### 12.4 Conveyance

```json
{
  "project_type": "conveyance",
  "terrain_points": [ /* ≥3 */ ],
  "source":  { "x": 0.0,   "y": 0.0   },
  "outlet":  { "x": 1000.0, "y": 500.0 },
  "seed": 42
}
```

Long-distance transport line between `source` and `outlet`. No service points
required.

### 12.5 Pump station

```json
{
  "project_type": "pump_station",
  "terrain_points": [ /* ≥3 */ ],
  "source": { "x": 0.0, "y": 0.0 },
  "seed": 42
}
```

Sizing of a single pumping plant. Engineering constants for pump curves, wet
well geometry, and pipe defaults are baked in at the dispatch layer (this is a
v1 limitation — see Known v1 limitations below).

### 12.6 Intake

```json
{
  "project_type": "intake",
  "terrain_points": [ /* ≥3 */ ],
  "source": { "x": 0.0, "y": 0.0 },
  "seed": 42
}
```

Surface water intake with screens / weir / channel. Engineering constants
(weir type, channel slope, screen geometry) are baked in at the dispatch
layer for v1.

### Known v1 limitations

- `pump_station` and `intake` dispatch carry 9–13 hardcoded engineering
  constants because `DesignRequest` does not yet have fields for them in v1.
  Two of them differ from oracle fixtures intentionally as conservative
  defaults (`intake.weir_type = 1` vs oracle `0`; `intake.channel_slope =
  0.001` vs oracle `0.005`).
- `DesignRequest` has no `source_type` field in v1. Intake's
  `VALID_SOURCE_TYPES` pre-validation is moot at the CLI layer; if a runtime
  guard rejects, exit code is 4.
- `TerrainModel.elevation_at(x, y)` is infallible (returns f64 without Option).

---

## 13. Integration patterns

The engine is a single binary. Wrap it as you like.

### REST wrapper (sketch)

```python
# Python FastAPI sketch
@app.post("/design")
def design(req: DesignRequest):
    proc = subprocess.run(
        ["hydro-cli"],
        input=json.dumps(req).encode("utf-8"),
        capture_output=True,
        timeout=600,
    )
    if proc.returncode == 0:
        return JSONResponse(json.loads(proc.stdout))
    raise HTTPException(
        status_code={1: 400, 2: 422, 3: 422, 4: 500}[proc.returncode],
        detail=proc.stderr.decode("utf-8", errors="replace"),
    )
```

Map exit codes:

| Exit | HTTP equivalent | Reason                      |
| ---- | --------------- | --------------------------- |
| 0    | 200             | success                     |
| 1    | 400             | bad request (validation)    |
| 2    | 422             | no feasible solution        |
| 3    | 422             | norm-compliance failure     |
| 4    | 500             | internal engine error       |

### GUI / desktop (sketch)

Spawn `hydro-cli` as a child process, pipe a `DesignRequest` to stdin, read
`DesignResult` from stdout. The GUI never owns the optimizer — it always
delegates to the binary.

### Civil 3D / GIS plugin (sketch)

1. Export terrain mesh to `terrain_points`.
2. Export service-point layer to `service_points`.
3. Spawn `hydro-cli` with the JSON.
4. Parse `result.best_solution.network`; render nodes/pipes back into the
   drawing as native objects.

### Batch processing (CI / scheduled runs)

```bash
for fixture in fixtures/*.json; do
  hydro-cli -i "$fixture" -o "results/$(basename "$fixture" .json).result.json" \
            --seed 42 --pretty
done
```

The `audit_hash` in every result lets you confirm reproducibility across
machines and times.

---

## 14. Troubleshooting

### "validation error: terrain_points must have at least 3 points"

You sent fewer than 3 entries in `terrain_points`. The engine needs at least 3
to build a terrain interpolation surface.

### "Unknown field `foo` at line N column M"

You sent a field not present in `DesignRequest`. `deny_unknown_fields` is
strict by design — extras are caught early so they cannot silently no-op.

### "Field `nsga_population_size` value 8 is outside valid range [20, 500]"

The validation floor is `pop ≥ 20`, `gen ≥ 10`, `max_time ≥ 30`. Internal
smoke tests bypass this; external callers must pass values at or above the
floor.

### Exit 2 — "no feasible solution found"

The GA explored the space but every individual violated a hard constraint.
Typical causes:

- Terrain is too steep / too flat for the slope envelope.
- Service points are unreachable from outlet/source given `max_depth` /
  `min_cover`.
- `min_velocity` is too high for the available diameters.
- Too few generations to find a feasible region.

Mitigations: widen `constraints` ranges, increase `nsga_generations`, raise
`nsga_max_time_seconds`, or revisit the terrain inputs.

### Exit 3 — "norm compliance failed"

There were feasible solutions, but none cleared every hard rule of the active
norm. Options: set `strict_norm_compliance: false` (you'll get exit 0 with
non-zero `norm_violations` in the result), or relax the inputs.

### Exit 4 — "internal error: …"

The binary itself or a solver dispatched an unrecoverable error. Reproduce
with the same seed, capture stderr, file an issue. The `audit_hash` will
pin the exact input.

### `--validate-only` reports "valid" but the full run exits 2 or 3

That is correct behavior. `--validate-only` checks **schema and bounds**,
not whether the optimizer can find a solution. The optimizer needs the GA
to actually run.

### Different machines produce different `audit_hash`

If `request`, `seed`, and engine version are identical, hashes must match.
If they don't:

- Re-check JSON byte-for-byte (whitespace doesn't matter, but field order
  inside the request body could if you serialized differently — the engine
  serializes via serde's stable order).
- Confirm the binary version (`hydro-cli --version`) matches across machines.

### Output JSON has missing fields you expected

Look for `#[serde(default)]` in `hydro-types/src/response.rs`. Fields that
default to empty objects or empty arrays are omitted on serialization only
when `skip_serializing_if` is set; check the relevant field.

---

## Appendix A — File layout for reference

```
hydro-cli/
├── src/
│   ├── main.rs           # binary entrypoint (clap shell)
│   └── lib.rs            # run(), validate_request(), CliError, helpers
└── tests/                # 50 integration tests (one per project type + features)

hydro-types/               # The contract: DesignRequest, DesignResult, enums, constraints
├── src/
│   ├── request.rs        # DesignRequest, validate()
│   ├── response.rs       # DesignResult, Solution, Diagnostics, PipeNetwork
│   ├── enums.rs          # ProjectType, NodeType, PipeMaterial, FlowType, Severity
│   ├── constraints.rs    # DesignConstraints with per-norm defaults
│   ├── network.rs        # NodeId, PipeId, NetworkNode, NetworkPipe, PipeNetwork
│   └── error.rs          # HydroTypesError

hydro-solvers/             # 6 domain solvers + SolverGraph helper
hydro-optimizer/           # NSGA-III genetic optimizer
hydro-terrain/             # Terrain interpolation
hydro-hydraulics/          # Hydraulic primitives
hydro-norms/               # Normative rules (CONAGUA_MX, etc.)
```

## Appendix B — Engine version stamp

The engine version embedded in `audit_hash` is `env!("CARGO_PKG_VERSION")`
of the `hydro-cli` crate. Bumping it changes every hash for the same inputs.
Use this intentionally to invalidate downstream caches when behavior changes.
