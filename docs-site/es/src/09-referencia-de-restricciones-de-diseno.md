# Referencia de restricciones de diseño

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

