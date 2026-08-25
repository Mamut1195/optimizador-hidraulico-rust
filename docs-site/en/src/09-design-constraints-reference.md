# Design constraints reference

`DesignConstraints` carry per-norm engineering defaults. You override them
through the optional `constraints` object in `DesignRequest`. All overrides
are optional; absent fields keep the per-norm default.

| Field                           | Default                                                 | Notes                              |
| ------------------------------- | ------------------------------------------------------- | ---------------------------------- |
| `min_slope`                     | `0.005`                                                 | dimensionless                      |
| `max_slope`                     | `0.05`                                                  | dimensionless                      |
| `min_cover`                     | `1.2` m                                                 | minimum earth cover                |
| `max_depth`                     | `5.0` m                                                 | maximum trench depth               |
| `min_velocity`                  | `0.6` m/s                                               | self-cleaning floor                |
| `max_velocity`                  | `5.0` m/s                                               | erosion ceiling                    |
| `min_diameter`                  | `0.2` m                                                 |                                    |
| `max_diameter`                  | `1.2` m                                                 |                                    |
| `available_diameters`           | `[0.2, 0.25, 0.3, 0.38, 0.45, 0.61, 0.76, 0.91, 1.07, 1.22]` (m) | commercial picks; must be all positive |
| `max_manhole_spacing`           | `100.0` m                                               |                                    |
| `manhole_at_direction_change`   | `true`                                                  |                                    |
| `manhole_at_slope_change`       | `true`                                                  |                                    |
| `manhole_at_diameter_change`    | `true`                                                  |                                    |
| `min_pressure`                  | `15.0` mca                                              | water supply service pressure floor |
| `max_pressure`                  | `50.0` mca                                              | service pressure ceiling           |
| `default_material`              | `"PVC"`                                                 |                                    |
| `default_roughness`             | `0.009`                                                 | Manning's `n` matching `default_material` |
| `max_bend_angle`                | `None`                                                  | degrees, `None` = unlimited        |

**Cross-field rules** (enforced on validation; violating yields exit 1 with
`HydroTypesError::CrossFieldViolation`):

- `min_slope <= max_slope`
- `min_velocity <= max_velocity`
- `min_diameter <= max_diameter`
- `min_pressure <= max_pressure`
- every entry of `available_diameters` is `> 0.0`

---

