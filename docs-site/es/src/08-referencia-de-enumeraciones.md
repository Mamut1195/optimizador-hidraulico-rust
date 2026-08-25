# Referencia de enumeraciones

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

