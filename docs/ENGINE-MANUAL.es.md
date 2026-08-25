# Manual del motor — `optimizador-hidraulico-rust`

**Versión de referencia**: frontera v1 del motor (main en `203c5c1`)
**Binario**: `hydro-cli`
**Contrato**: JSON de entrada / JSON de salida
**Audiencia**: integradores, consumidores aguas abajo, mantenedores futuros, agentes de IA

---

## Índice

1. [Qué es este motor](#1-qué-es-este-motor)
2. [Compilación e instalación](#2-compilación-e-instalación)
3. [Patrones de invocación](#3-patrones-de-invocación)
4. [Referencia de banderas de CLI](#4-referencia-de-banderas-de-cli)
5. [Códigos de salida](#5-códigos-de-salida)
6. [Contrato de entrada — `DesignRequest`](#6-contrato-de-entrada--designrequest)
7. [Contrato de salida — `DesignResult`](#7-contrato-de-salida--designresult)
8. [Referencia de enumeraciones](#8-referencia-de-enumeraciones)
9. [Referencia de restricciones de diseño](#9-referencia-de-restricciones-de-diseño)
10. [Reglas de validación](#10-reglas-de-validación)
11. [Determinismo y reproducibilidad](#11-determinismo-y-reproducibilidad)
12. [Ejemplos resueltos por tipo de proyecto](#12-ejemplos-resueltos-por-tipo-de-proyecto)
13. [Patrones de integración](#13-patrones-de-integración)
14. [Solución de problemas](#14-solución-de-problemas)

---

## 1. Qué es este motor

Un motor autónomo de optimización hidráulica que:

- Toma un único documento JSON `DesignRequest` que describe el terreno, la
  demanda, las restricciones y los parámetros del optimizador.
- Ejecuta un optimizador genético multi-objetivo NSGA-III (5 objetivos:
  costo, excavación, bombeo, interferencia, resiliencia) sobre el solver
  de dominio correspondiente (alcantarillado, agua potable, conducción,
  distribución, estación de bombeo o captación).
- Devuelve un documento JSON `DesignResult` con redes Pareto-óptimas
  jerarquizadas, sus puntajes, diagnósticos y un hash de auditoría.

El motor **es un binario, no una biblioteca**. La CLI es el único contrato
estable. Cualquier consumidor aguas abajo — servidor REST, GUI, plugin de
Civil 3D, script de automatización — habla con él mediante stdin/stdout o
archivos.

Fuera del alcance de v1: compilación WASM, FFI con C-ABI, servidor HTTP
incrustado, interoperabilidad con EPANET/SWMM, GUI. Cada uno de estos sería
una fase futura que envuelva este binario.

---

## 2. Compilación e instalación

```bash
# Clonar y compilar (release)
git clone <repo>
cd optimizador-hidraulico-rust
cargo build --release -p hydro-cli

# El binario queda en:
# Windows:    target/release/hydro-cli.exe
# Tipo Unix:  target/release/hydro-cli
```

Toolchain de Rust: **1.95.0** (fijada en CI). Las dependencias del workspace
incluyen `petgraph`, `rayon`, `clap`, `serde_json`, `sha2`. El binario release
es un único archivo sin dependencias en tiempo de ejecución.

Verificar:

```bash
./target/release/hydro-cli --version
./target/release/hydro-cli --help
```

---

## 3. Patrones de invocación

### Con archivos (lo más común)

```bash
hydro-cli --input request.json --output result.json
hydro-cli -i request.json -o result.json
```

### Con tuberías (scripting / composición unix)

```bash
cat request.json | hydro-cli > result.json
hydro-cli < request.json > result.json
```

### Mixto

```bash
hydro-cli --input request.json > result.json     # entrada desde archivo, salida a stdout
cat request.json | hydro-cli --output result.json # entrada desde stdin, salida a archivo
```

### Solo validación (sin optimización)

```bash
hydro-cli --input request.json --validate-only
# stdout en éxito:  {"status":"valid","project_type":"sewer"}
# stdout en error:  {"status":"invalid","error":"..."}
# Tiempo de pared: <50 ms (sin GA, sin solver, sin modelo de terreno)
```

### Corrida determinista (reproducible)

```bash
hydro-cli --input request.json --output result.json --seed 42
# O pasar el seed en el cuerpo del request; --seed prevalece si ambos están presentes.
```

### Salida legible por humanos

```bash
hydro-cli --input request.json --output result.json --pretty
```

---

## 4. Referencia de banderas de CLI

| Bandera             | Corta | Tipo     | Defecto  | Comportamiento                                                          |
| ------------------- | ----- | -------- | -------- | ----------------------------------------------------------------------- |
| `--input <PATH>`    | `-i`  | PathBuf  | (stdin)  | Lee el JSON `DesignRequest` desde este archivo. Si falta, lee de stdin. |
| `--output <PATH>`   | `-o`  | PathBuf  | (stdout) | Escribe el JSON `DesignResult` a este archivo. Si falta, escribe stdout.|
| `--seed <N>`        |       | u64      | ninguno  | Sobrescribe `DesignRequest.seed` antes de correr el optimizador.        |
| `--pretty`          |       | bandera  | off      | Imprime el JSON de salida con sangría de 2 espacios. Defecto: compacto. |
| `--validate-only`   |       | bandera  | off      | Ejecuta solo `validate_request()`. No corre el optimizador.             |
| `--help` / `-h`     |       | bandera  | —        | Muestra ayuda y sale con código 0.                                      |
| `--version` / `-V`  |       | bandera  | —        | Muestra la versión y sale con código 0.                                 |

Los errores de E/S (archivo no encontrado, permiso denegado, tubería rota en
stdout) salen con código **4** y un mensaje en stderr. Los errores a nivel
del motor se enrutan a través de `CliError` y emergen como códigos de salida
1 a 4 (ver siguiente sección).

---

## 5. Códigos de salida

| Código | Significado                            | Causa típica                                                                                            |
| ------ | -------------------------------------- | ------------------------------------------------------------------------------------------------------- |
| `0`    | Éxito                                  | El solver completó y emitió al menos una solución factible                                              |
| `1`    | Error de validación                    | JSON mal formado, campo requerido faltante, valor fuera de rango, variante de enum desconocida, violación de restricción cruzada |
| `2`    | Sin solución factible                  | El GA terminó pero cada candidato violó una restricción dura                                            |
| `3`    | Sin solución conforme a norma          | Existe al menos una solución factible, pero ninguna pasa la norma activa con `strict_norm_compliance: true` |
| `4`    | Error interno del motor                | Fallo de E/S, fallo de serialización, pánico del solver, error al construir el modelo de terreno        |

El código de salida 0 junto con `result.success == true` es la única señal de
"todo en orden". Cualquier otra cosa, tratar como fallo.

---

## 6. Contrato de entrada — `DesignRequest`

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
| `material`                  | string    | `"PVC"`        | `PVC`, `CONCRETE`, `HDPE`, `STEEL`, `CAST_IRON` (se aceptan alias en español — ver §8) |
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
| `constraints`       | objeto (`DesignConstraintOverrides`) | Todos los campos opcionales. Se fusionan sobre los valores por defecto de la norma. Ver §9. |
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

## 7. Contrato de salida — `DesignResult`

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
- `roughness` es el coeficiente `n` de Manning. Valores por material en §8.
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
  emitido como string hexadecimal en minúscula de 64 caracteres. Mismo input + mismo seed + misma versión del motor → hash idéntico. Ver §11.
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

## 8. Referencia de enumeraciones

### `project_type`

| Valor           | Dominio                                                  |
| --------------- | -------------------------------------------------------- |
| `sewer`         | Red de alcantarillado por gravedad                       |
| `water_supply`  | Distribución presurizada a puntos de servicio            |
| `conveyance`    | Conducción presurizada a larga distancia                 |
| `distribution`  | Red presurizada con mallas / loops                       |
| `pump_station`  | Dimensionamiento de una planta de bombeo                 |
| `intake`        | Captación de agua superficial con rejas / vertedero      |

La entrada es insensible a mayúsculas y tolera espacios / guiones
(`"Water Supply"`, `"water-supply"`, `"WATER_SUPPLY"` todos mapean a
`water_supply`). La salida siempre es snake_case en minúscula.

### `material`

| Canónico      | Alias en español aceptados en la entrada | `n` de Manning |
| ------------- | ---------------------------------------- | -------------- |
| `PVC`         | —                                        | `0.009`        |
| `HDPE`        | `PEAD`                                   | `0.009`        |
| `CONCRETE`    | `CONCRETO`                               | `0.013`        |
| `STEEL`       | `ACERO`                                  | `0.012`        |
| `CAST_IRON`   | `FIERRO_FUNDIDO`, `CASTIRON`             | `0.013`        |

La resolución de alias es insensible a mayúsculas y normaliza espacios /
guiones a guiones bajos. La salida siempre es la forma canónica en MAYÚSCULA.

### `node_type` (solo salida)

`MANHOLE`, `JUNCTION`, `OUTLET`, `INLET`, `PUMP`, `VALVE`, `HYDRANT`,
`TANK`, `RESERVOIR`, `SERVICE`.

### Otras enumeraciones (solo salida)

- `FlowType`: `GRAVITY`, `PRESSURE`
- `Severity` (validación de norma): `hard`, `soft`, `warning` (minúscula)

---

## 9. Referencia de restricciones de diseño

`DesignConstraints` lleva los valores por defecto de ingeniería por norma.
Se sobrescriben mediante el objeto opcional `constraints` en `DesignRequest`.
Todos los overrides son opcionales; los campos ausentes mantienen el valor
por defecto de la norma.

| Campo                           | Defecto                                                 | Notas                              |
| ------------------------------- | ------------------------------------------------------- | ---------------------------------- |
| `min_slope`                     | `0.005`                                                 | adimensional                       |
| `max_slope`                     | `0.05`                                                  | adimensional                       |
| `min_cover`                     | `1.2` m                                                 | recubrimiento de tierra mínimo     |
| `max_depth`                     | `5.0` m                                                 | profundidad máxima de zanja        |
| `min_velocity`                  | `0.6` m/s                                               | piso autolimpieza                  |
| `max_velocity`                  | `5.0` m/s                                               | techo de erosión                   |
| `min_diameter`                  | `0.2` m                                                 |                                    |
| `max_diameter`                  | `1.2` m                                                 |                                    |
| `available_diameters`           | `[0.2, 0.25, 0.3, 0.38, 0.45, 0.61, 0.76, 0.91, 1.07, 1.22]` (m) | catálogo comercial; todos deben ser positivos |
| `max_manhole_spacing`           | `100.0` m                                               |                                    |
| `manhole_at_direction_change`   | `true`                                                  |                                    |
| `manhole_at_slope_change`       | `true`                                                  |                                    |
| `manhole_at_diameter_change`    | `true`                                                  |                                    |
| `min_pressure`                  | `15.0` mca                                              | piso de presión de servicio (agua potable) |
| `max_pressure`                  | `50.0` mca                                              | techo de presión de servicio       |
| `default_material`              | `"PVC"`                                                 |                                    |
| `default_roughness`             | `0.009`                                                 | `n` de Manning correspondiente a `default_material` |
| `max_bend_angle`                | `None`                                                  | grados, `None` = sin límite        |

**Reglas cruzadas** (exigidas en la validación; violarlas devuelve código 1
con `HydroTypesError::CrossFieldViolation`):

- `min_slope <= max_slope`
- `min_velocity <= max_velocity`
- `min_diameter <= max_diameter`
- `min_pressure <= max_pressure`
- cada entrada de `available_diameters` es `> 0.0`

---

## 10. Reglas de validación

Antes de que arranque el GA corren dos etapas de validación:

### Etapa 1 — Deserialización (serde)

| Modo de falla                                | Efecto      |
| -------------------------------------------- | ----------- |
| JSON mal formado                             | código 1    |
| Campo desconocido en cualquier nivel         | código 1    |
| Campo requerido faltante (`terrain_points`)  | código 1    |
| Tipo incorrecto (string donde se espera número) | código 1 |
| Variante desconocida de `project_type`       | código 1    |
| Variante o alias de `material` desconocido   | código 1    |

### Etapa 2 — Validación semántica (`DesignRequest::validate()`)

| Modo de falla                                                  | Variante de error      | Efecto    |
| -------------------------------------------------------------- | ---------------------- | --------- |
| `terrain_points.len() < 3`                                     | `MissingRequired`      | código 1  |
| Cualquier coordenada de `terrain_points[*]` no finita          | `OutOfRange`           | código 1  |
| Campo numérico fuera de su rango documentado                   | `OutOfRange`           | código 1  |
| Restricción cruzada violada                                    | `CrossFieldViolation`  | código 1  |
| `available_diameters` contiene valor no positivo               | `InvalidDiameter`      | código 1  |
| Polígono / ruta / red con muy pocas coordenadas                | `TooFewCoords`         | código 1  |

`--validate-only` corre ambas etapas y sale, sin nunca proceder al GA.

### Etapa 3 — Tiempo de ejecución del motor (durante/después del GA)

| Modo de falla                                  | Efecto                            |
| ---------------------------------------------- | --------------------------------- |
| No se encontró solución factible               | código 2, `result.success == false` |
| Hay soluciones factibles pero ninguna conforme a norma + `strict_norm_compliance == true` | código 3 |
| Pánico del solver, error en construcción del terreno, fallo interno de serialización | código 4 |

---

## 11. Determinismo y reproducibilidad

El motor es determinista cuando el seed está fijo:

```
seed_resuelto = bandera --seed (si fue dada) > request.seed (si fue dado) > aleatorio
audit_hash    = hex_minuscula(SHA-256(
                  serde_json::to_string(&request)
                  || "|"
                  || seed_en_decimal
                  || "|"
                  || version_motor
                ))
```

Propiedades:

- Mismo JSON `request` + mismo `seed` + misma versión del motor → mismas
  `result.solutions` (byte por byte bajo round-trip JSON).
- El hash es sensible a cambios de seed: cambiar el seed cambia el hash.
- El hash es sensible a la versión del motor (`CARGO_PKG_VERSION`); actualizar
  el binario cambia el hash intencionalmente aun con entradas idénticas.

Usá el hash para:

- Etiquetar documentos de resultado almacenados (rastro de auditoría).
- Detectar deriva de input en tests de regresión (entrada cambió → nuevo hash).
- Fijar una "corrida buena conocida" para la firma de ingeniería.

El trabajo paralelo del motor corre sobre `rayon`. El seed se propaga al RNG
del GA, y la capa de grafo determinista (introducida en la Fase 7.5) garantiza
orden topológico idéntico byte por byte entre corridas.

---

## 12. Ejemplos resueltos por tipo de proyecto

### 12.1 Alcantarillado (`sewer`, recolección por gravedad)

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

Entradas requeridas: `terrain_points`, `service_points`, `outlet`.
El módulo de alcantarillado diseña redes de flujo por gravedad enrutadas
desde cada punto de servicio hasta la descarga, respetando recubrimiento y
velocidad mínima.

### 12.2 Agua potable (`water_supply`)

```json
{
  "project_type": "water_supply",
  "project_name": "Distribución sur",
  "terrain_points": [ /* ≥3 (x,y,z) finitas */ ],
  "service_points": [ { "x": 50.0, "y": 50.0 } ],
  "source":         { "x": 0.0,  "y": 0.0 },
  "source_head":    35.0,
  "flow_per_service": 0.001,
  "constraints": { "min_pressure": 12.0, "max_pressure": 45.0 },
  "seed": 42
}
```

Requeridas: `terrain_points`, `service_points`, `source`, idealmente
`source_head`. Diseña una red ramificada presurizada desde la fuente hasta
cada punto de servicio, respetando `min_pressure` / `max_pressure` y
`min_velocity` / `max_velocity`.

### 12.3 Distribución mallada (`distribution`)

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

Requiere **al menos 2 puntos de servicio** (chequeo previo de topología de
malla). Construye una red mallada (MST de Kruskal + aristas adicionales de
loop) para redundancia.

### 12.4 Conducción (`conveyance`)

```json
{
  "project_type": "conveyance",
  "terrain_points": [ /* ≥3 */ ],
  "source":  { "x": 0.0,   "y": 0.0   },
  "outlet":  { "x": 1000.0, "y": 500.0 },
  "seed": 42
}
```

Línea de transporte a larga distancia entre `source` y `outlet`. No hace
falta `service_points`.

### 12.5 Estación de bombeo (`pump_station`)

```json
{
  "project_type": "pump_station",
  "terrain_points": [ /* ≥3 */ ],
  "source": { "x": 0.0, "y": 0.0 },
  "seed": 42
}
```

Dimensionamiento de una sola planta de bombeo. Las constantes de ingeniería
para curvas de bomba, geometría del cárcamo húmedo y defaults de tubería
están horneadas en la capa de dispatch (es una limitación de v1 — ver
Limitaciones conocidas de v1 abajo).

### 12.6 Captación (`intake`)

```json
{
  "project_type": "intake",
  "terrain_points": [ /* ≥3 */ ],
  "source": { "x": 0.0, "y": 0.0 },
  "seed": 42
}
```

Captación de agua superficial con rejas / vertedero / canal. Las constantes
de ingeniería (tipo de vertedero, pendiente del canal, geometría de las
rejas) están horneadas en la capa de dispatch para v1.

### Limitaciones conocidas de v1

- El dispatch de `pump_station` e `intake` lleva 9–13 constantes de
  ingeniería hardcodeadas porque `DesignRequest` aún no tiene campos para
  ellas en v1. Dos de ellas difieren de los fixtures del oráculo de forma
  intencional como defaults conservadores (`intake.weir_type = 1` vs
  oráculo `0`; `intake.channel_slope = 0.001` vs oráculo `0.005`).
- `DesignRequest` no tiene campo `source_type` en v1. La pre-validación de
  `VALID_SOURCE_TYPES` de intake no aplica en la capa CLI; si una guarda en
  tiempo de ejecución rechaza, el código de salida es 4.
- `TerrainModel.elevation_at(x, y)` es infalible (devuelve f64 sin Option).

---

## 13. Patrones de integración

El motor es un único binario. Lo envolvés como quieras.

### Envoltorio REST (bosquejo)

```python
# Bosquejo en Python FastAPI
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

Mapeo de códigos de salida:

| Salida | Equivalente HTTP | Razón                       |
| ------ | ---------------- | --------------------------- |
| 0      | 200              | éxito                       |
| 1      | 400              | solicitud inválida (validación) |
| 2      | 422              | sin solución factible       |
| 3      | 422              | fallo de conformidad a norma |
| 4      | 500              | error interno del motor     |

### GUI / desktop (bosquejo)

Lanzá `hydro-cli` como proceso hijo, mandá un `DesignRequest` por stdin y leé
el `DesignResult` desde stdout. La GUI nunca dueña del optimizador: siempre
delega en el binario.

### Plugin Civil 3D / GIS (bosquejo)

1. Exportar la malla de terreno a `terrain_points`.
2. Exportar la capa de puntos de servicio a `service_points`.
3. Lanzar `hydro-cli` con el JSON.
4. Parsear `result.best_solution.network`; renderizar nodos/tuberías de
   vuelta al dibujo como objetos nativos.

### Procesamiento por lotes (CI / corridas programadas)

```bash
for fixture in fixtures/*.json; do
  hydro-cli -i "$fixture" -o "results/$(basename "$fixture" .json).result.json" \
            --seed 42 --pretty
done
```

El `audit_hash` en cada resultado te deja confirmar reproducibilidad entre
máquinas y momentos.

---

## 14. Solución de problemas

### "validation error: terrain_points must have at least 3 points"

Mandaste menos de 3 entradas en `terrain_points`. El motor necesita al menos
3 para construir una superficie de interpolación de terreno.

### "Unknown field `foo` at line N column M"

Mandaste un campo que no existe en `DesignRequest`. `deny_unknown_fields` es
estricto por diseño — los extras se atrapan temprano para que no se queden
silenciosamente sin efecto.

### "Field `nsga_population_size` value 8 is outside valid range [20, 500]"

El piso de validación es `pop ≥ 20`, `gen ≥ 10`, `max_time ≥ 30`. Los smoke
tests internos saltan esto; los llamadores externos tienen que pasar valores
iguales o por encima del piso.

### Código 2 — "no feasible solution found"

El GA exploró el espacio pero cada individuo violó una restricción dura.
Causas típicas:

- El terreno es demasiado empinado / demasiado plano para el envelope de pendientes.
- Los puntos de servicio son inalcanzables desde la descarga/fuente dados
  `max_depth` / `min_cover`.
- `min_velocity` es demasiado alta para los diámetros disponibles.
- Muy pocas generaciones para encontrar una región factible.

Mitigaciones: ampliar los rangos de `constraints`, aumentar
`nsga_generations`, subir `nsga_max_time_seconds` o revisar las entradas de
terreno.

### Código 3 — "norm compliance failed"

Hubo soluciones factibles, pero ninguna superó todas las reglas duras de la
norma activa. Opciones: poner `strict_norm_compliance: false` (vas a tener
código 0 con `norm_violations` distinto de cero en el resultado), o relajar
las entradas.

### Código 4 — "internal error: …"

El binario mismo o un solver despachó un error irrecuperable. Reproducí con
el mismo seed, capturá stderr, abrí un issue. El `audit_hash` va a fijar la
entrada exacta.

### `--validate-only` reporta "valid" pero la corrida completa sale con código 2 o 3

Es el comportamiento correcto. `--validate-only` verifica **esquema y
límites**, no si el optimizador puede encontrar una solución. El optimizador
necesita que el GA realmente corra.

### Máquinas distintas producen `audit_hash` distintos

Si `request`, `seed` y versión del motor son idénticos, los hashes tienen
que coincidir. Si no:

- Re-verificá el JSON byte a byte (los espacios en blanco no importan, pero
  el orden de los campos dentro del cuerpo del request sí podría si lo
  serializaste de forma diferente — el motor serializa con el orden estable
  de serde).
- Confirmá que la versión del binario (`hydro-cli --version`) coincide entre
  máquinas.

### El JSON de salida tiene campos faltantes que esperabas

Mirá `#[serde(default)]` en `hydro-types/src/response.rs`. Los campos que
default a objetos vacíos o arreglos vacíos se omiten en la serialización
solo cuando `skip_serializing_if` está puesto; revisá el campo relevante.

---

## Apéndice A — Layout de archivos para referencia

```
hydro-cli/
├── src/
│   ├── main.rs           # entrypoint del binario (shell clap)
│   └── lib.rs            # run(), validate_request(), CliError, helpers
└── tests/                # 50 tests de integración (uno por tipo de proyecto + features)

hydro-types/               # El contrato: DesignRequest, DesignResult, enums, constraints
├── src/
│   ├── request.rs        # DesignRequest, validate()
│   ├── response.rs       # DesignResult, Solution, Diagnostics, PipeNetwork
│   ├── enums.rs          # ProjectType, NodeType, PipeMaterial, FlowType, Severity
│   ├── constraints.rs    # DesignConstraints con defaults por norma
│   ├── network.rs        # NodeId, PipeId, NetworkNode, NetworkPipe, PipeNetwork
│   └── error.rs          # HydroTypesError

hydro-solvers/             # 6 solvers de dominio + helper SolverGraph
hydro-optimizer/           # Optimizador genético NSGA-III
hydro-terrain/             # Interpolación de terreno
hydro-hydraulics/          # Primitivas hidráulicas
hydro-norms/               # Reglas normativas (CONAGUA_MX, etc.)
```

## Apéndice B — Sello de versión del motor

La versión del motor embebida en `audit_hash` es `env!("CARGO_PKG_VERSION")`
del crate `hydro-cli`. Subirla cambia todos los hashes para las mismas
entradas. Usalo intencionalmente para invalidar cachés aguas abajo cuando
el comportamiento cambia.
