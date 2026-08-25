# Contrato de entrada — `DesignRequest`

El esquema completo vive en `hydro-types/src/request.rs`. La deserialización
es estricta (`#[serde(deny_unknown_fields)]`): cualquier campo extra falla
con código de salida 1.

### Request mínimo válido

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

Todo lo demás tiene un valor por defecto.

### Campos requeridos

| Campo            | Tipo                    | Restricción                                                                |
| ---------------- | ----------------------- | -------------------------------------------------------------------------- |
| `project_type`   | string                  | Uno de `sewer`, `water_supply`, `conveyance`, `distribution`, `pump_station`, `intake`. Insensible a mayúsculas en la entrada; se normaliza a snake_case en minúscula. |
| `terrain_points` | arreglo de `{x, y, z}`  | Al menos 3 puntos, todas las coordenadas finitas (sin NaN, sin infinito)   |

### Campos opcionales con valor por defecto

| Campo                       | Tipo      | Defecto        | Rango / valores válidos                          |
| --------------------------- | --------- | -------------- | ------------------------------------------------ |
| `project_name`              | string    | `"Unnamed"`    | cualquiera                                       |
| `material`                  | string    | `"PVC"`        | `PVC`, `CONCRETE`, `HDPE`, `STEEL`, `CAST_IRON` (se aceptan alias en español — ver [§8 Enumeraciones](./08-referencia-de-enumeraciones.md)) |
| `num_alternatives`          | u32       | `3`            | `[1, 10]`                                        |
| `norm`                      | string    | `"CONAGUA_MX"` | cualquiera (se normaliza a mayúsculas + guiones bajos) |
| `strict_norm_compliance`    | bool      | `true`         | si es `true`, no se devuelve ninguna solución que viole la norma (código 3 si no hay conformes) |
| `flow_per_service`          | f64       | `0.002`        | `(0.0, 10.0]` m³/s                               |
| `grid_resolution`           | f64       | `20.0`         | `[1.0, 500.0]` m                                 |
| `seed`                      | u64?      | `None`         | `[0, 2_147_483_647]` cuando está presente        |
| `nsga_population_size`      | u32       | `100`          | `[20, 500]`                                      |
| `nsga_generations`          | u32       | `50`           | `[10, 500]`                                      |
| `nsga_max_time_seconds`     | u32       | `300`          | `[30, 7200]`                                     |
| `nsga_num_workers`          | u32?      | `None`         | `[1, 256]` cuando está presente (tope de hilos de rayon) |
| `enforce_cover_depth`       | bool      | `false`        |                                                  |
| `enforce_existing_clearance`| bool      | `false`        |                                                  |
| `enforce_segment_slopes`    | bool      | `false`        |                                                  |
| `enable_path_smoothing`     | bool      | `false`        |                                                  |
| `weight_cost`               | f64       | `0.30`         | `[0.0, 1.0]`                                     |
| `weight_excavation`         | f64       | `0.25`         | `[0.0, 1.0]`                                     |
| `weight_pumping`            | f64       | `0.15`         | `[0.0, 1.0]`                                     |
| `weight_interference`       | f64       | `0.10`         | `[0.0, 1.0]`                                     |
| `weight_resilience`         | f64       | `0.20`         | `[0.0, 1.0]`                                     |
| `min_vertical_separation`   | f64       | `0.3`          | `[0.0, 10.0]` m                                  |
| `min_horizontal_separation` | f64       | `1.0`          | `[0.0, 20.0]` m                                  |
| `project_crs`               | string?   | `None`         | solo informativo (no se usa en el cálculo)       |

### Entradas geométricas

| Campo               | Tipo                            | Requerido cuando                       | Restricción                              |
| ------------------- | ------------------------------- | -------------------------------------- | ---------------------------------------- |
| `service_points`    | arreglo de `{x, y}`             | sewer, water_supply, distribution      | nodos de demanda (la red debe alcanzarlos) |
| `outlet`            | `{x, y}`                        | sewer, conveyance                      | punto de descarga                        |
| `source`            | `{x, y}`                        | water_supply, intake, conveyance       | ubicación de la fuente de agua           |
| `source_head`       | f64                             | water_supply (suele), intake           | carga disponible en la fuente (m)        |

El optimizador deriva la elevación `z` en cualquier `(x, y)` interpolando los
`terrain_points`. No hay que pasarla explícitamente para puntos de servicio,
descargas o fuentes.

### Entradas de restricciones

| Campo               | Tipo                              | Notas                                                                |
| ------------------- | --------------------------------- | -------------------------------------------------------------------- |
| `constraints`       | objeto (`DesignConstraintOverrides`) | Todos los campos opcionales. Se fusionan sobre los valores por defecto de la norma. Ver [§9 Restricciones de diseño](./09-referencia-de-restricciones-de-diseno.md). |
| `forbidden_zones`   | arreglo de polígonos              | Cada polígono: `{coords: [{x,y}, ...], buffer?: f64}` — `coords` ≥ 3, `buffer ∈ [0, 200]` m |
| `mandatory_routes`  | arreglo de rutas                  | Cada ruta: `{coords: [{x,y}, ...], corridor_width?: f64}` — `coords` ≥ 2, `corridor_width ∈ [0, 500]` m |
| `existing_networks` | arreglo de polilíneas             | Cada una: `{coords: [{x,y}, ...], depth?: f64, setback?: f64}` — `coords` ≥ 2, `depth ∈ [0, 30]` m, `setback ∈ [0, 200]` m |

### Campo por campo: rangos de entrada exigidos por `DesignRequest::validate()`

Estas son verificaciones de límites; violar cualquiera devuelve código 1 con
un diagnóstico `HydroTypesError::OutOfRange` en stderr.

```
terrain_points.len()      >= 3
num_alternatives          en 1..=10
flow_per_service          en (0.0, 10.0]
grid_resolution           en [1.0, 500.0]
seed (cuando está presente)       en 0..=2_147_483_647
nsga_population_size      en 20..=500
nsga_generations          en 10..=500
nsga_max_time_seconds     en 30..=7200
nsga_num_workers (cuando está presente)  en 1..=256
weight_*                  en [0.0, 1.0]
min_vertical_separation   en [0.0, 10.0]
min_horizontal_separation en [0.0, 20.0]
forbidden_zones[*].buffer       en [0.0, 200.0]
mandatory_routes[*].corridor_width en [0.0, 500.0]
existing_networks[*].depth      en [0.0, 30.0]
existing_networks[*].setback    en [0.0, 200.0]
```

---

