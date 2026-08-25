# Validation rules

Two validation stages run before the GA starts:

### Stage 1 — Deserialization (serde)

| Failure mode                              | Effect            |
| ----------------------------------------- | ----------------- |
| Malformed JSON                            | exit 1            |
| Unknown field at any level                | exit 1            |
| Missing required field (`terrain_points`) | exit 1            |
| Wrong type (string where number expected) | exit 1            |
| Unknown `project_type` variant            | exit 1            |
| Unknown `material` variant or alias       | exit 1            |

### Stage 2 — Semantic validation (`DesignRequest::validate()`)

| Failure mode                                            | Error variant           | Effect |
| ------------------------------------------------------- | ----------------------- | ------ |
| `terrain_points.len() < 3`                              | `MissingRequired`       | exit 1 |
| Any `terrain_points[*]` coordinate non-finite           | `OutOfRange`            | exit 1 |
| Numeric field out of its documented range               | `OutOfRange`            | exit 1 |
| Cross-field constraint violated                         | `CrossFieldViolation`   | exit 1 |
| `available_diameters` contains non-positive value       | `InvalidDiameter`       | exit 1 |
| Polygon / route / network with too few coordinates      | `TooFewCoords`          | exit 1 |

`--validate-only` runs both stages and exits, never proceeding to the GA.

### Stage 3 — Engine runtime (during/after GA)

| Failure mode                            | Effect                          |
| --------------------------------------- | ------------------------------- |
| No feasible solution found              | exit 2, `result.success == false` |
| Feasible solutions found, none norm-compliant + `strict_norm_compliance == true` | exit 3 |
| Solver panic, terrain construction error, internal serialization failure | exit 4 |

---

