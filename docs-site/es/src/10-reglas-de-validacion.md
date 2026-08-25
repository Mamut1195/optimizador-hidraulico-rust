# Reglas de validación

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

