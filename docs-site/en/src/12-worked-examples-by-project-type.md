# Worked examples by project type

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

