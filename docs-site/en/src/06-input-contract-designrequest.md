# Input contract — `DesignRequest`

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
| `material`                  | string    | `"PVC"`        | `PVC`, `CONCRETE`, `HDPE`, `STEEL`, `CAST_IRON` (Spanish aliases accepted — see [§8 Enums](./08-enums-reference.md)) |
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
| `constraints`       | object (`DesignConstraintOverrides`) | All fields optional. Merges over the per-norm defaults. See [§9 Design constraints reference](./09-design-constraints-reference.md). |
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

