# Ejemplos resueltos por tipo de proyecto

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

