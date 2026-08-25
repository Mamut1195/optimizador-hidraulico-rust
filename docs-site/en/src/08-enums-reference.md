# Enums reference

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

