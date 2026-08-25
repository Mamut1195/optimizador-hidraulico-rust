# Output contract — `DesignResult`

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
- `roughness` is Manning's `n`. Values per material in [§8 Enums](./08-enums-reference.md).
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
  same engine version → identical hash. See [§11 Determinism](./11-determinism-reproducibility.md).
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

