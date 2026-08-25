# Contrato de salida — `DesignResult`

Esquema completo en `hydro-types/src/response.rs`. El motor **nunca** emite
`mcp_instructions` (spec S-23).

### Forma de nivel superior

```json
{
  "success": true,
  "project_name": "Unnamed",
  "project_type": "sewer",
  "elapsed_seconds": 12.5,
  "seed": 42,
  "norm": "CONAGUA_MX",
  "solutions": [ /* arreglo jerarquizado de Solution */ ],
  "best_solution": { /* alias de solutions[0] */ },
  "diagnostics": { /* ver abajo */ }
}
```

### `Solution`

```json
{
  "rank": 1,
  "network": { /* PipeNetwork — ver abajo */ },
  "score": {
    "total_cost": 125000.0,
    "total_length": 850.0,
    "total_excavation": 320.0,
    "norm_violations": 0,
    "pump_count": 0,
    "details": { /* objeto opaco por solver, preservado para trazabilidad */ }
  },
  "metadata": {
    "fitness": [125000.0, 320.0, 0.0, 12.5, 0.85],
    "genetic_params": { /* parámetros GA por individuo */ },
    "norm_validation": { /* resultados de validación por elemento */ }
  }
}
```

- `rank` — base 1, ordenado ascendente por `total_cost`. `rank == 1` es el más barato.
- `score.total_cost` — unidades de moneda (definidas por el proyecto, típicamente MXN o USD).
- `score.norm_violations` — recuento de violaciones blandas/de advertencia aun cuando `strict_norm_compliance` permitió pasar la solución.
- `metadata.fitness` — el vector crudo de fitness 5-objetivo de NSGA-III: `[costo, excavación, bombeo, interferencia, resiliencia]`.

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

Notas:

- Los campos `id` son strings arbitrarios (round-trip exacto a través de JSON
  — `"g12"`, `"Pipe - (3)"`). El motor no impone un formato numérico.
- `node_type` usa `SCREAMING_SNAKE_CASE`: `MANHOLE`, `JUNCTION`, `OUTLET`,
  `INLET`, `PUMP`, `VALVE`, `HYDRANT`, `TANK`, `RESERVOIR`, `SERVICE`.
- `material` usa strings canónicos en MAYÚSCULA: `PVC`, `CONCRETE`, `HDPE`,
  `STEEL`, `CAST_IRON`.
- `roughness` es el coeficiente `n` de Manning. Valores por material en [§8 Enumeraciones](./08-referencia-de-enumeraciones.md).
- `slope` es adimensional; positivo = pendiente descendente en el sentido del flujo.
- `design_flow` está en m³/s.
- `waypoints` son pares `[x, y]` intermedios para enrutamiento por polilínea;
  usualmente vacío.
- `node.metadata` puede llevar claves reconocidas por el validador:
  - `pressure_mca: f64` — presión estática en el nodo en m.c.a.
- Claves reconocidas en `pipe.metadata`:
  - `bypassed_by_pump: bool` — cuando es `true`, las reglas de pendiente se saltan para esta tubería
  - `flow_m3s: f64` — caudal de diseño para el cálculo de velocidad de presión

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
  "audit_hash": "a3f7b2…(64 caracteres hex en total, minúscula)",
  "convergence": [
    { "generation": 1, "min_cost": 180000.0, "min_excavation": 400.0, "min_resilience_deficit": 0.25 },
    { "generation": 2, "min_cost": 165000.0, "min_excavation": 380.0, "min_resilience_deficit": 0.20 }
  ]
}
```

- `compute_backend` siempre es `"rayon-native"`.
- `audit_hash` es `SHA-256(request_serializado + "|" + seed_decimal + "|" + version_motor)`,
  emitido como string hexadecimal en minúscula de 64 caracteres. Mismo input + mismo seed + misma versión del motor → hash idéntico. Ver [§11 Determinismo](./11-determinismo-y-reproducibilidad.md).
- `convergence` contiene un registro por generación, útil para graficar el progreso del GA.

### Salida en modo solo-validación

Cuando se invoca con `--validate-only`, el motor emite un objeto de estado
corto en vez de un `DesignResult` completo:

```json
{ "status": "valid",   "project_type": "sewer" }
{ "status": "invalid", "error":        "validation error: terrain_points must have at least 3 points" }
```

Tiempo de pared: < 50 ms en cualquier fixture de referencia.

---

