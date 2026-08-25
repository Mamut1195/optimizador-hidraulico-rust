# Exit codes

| Code | Meaning                                | Typical cause                                                                                            |
| ---- | -------------------------------------- | -------------------------------------------------------------------------------------------------------- |
| `0`  | Success                                | Solver completed and emitted at least one feasible solution                                              |
| `1`  | Validation error                       | Malformed JSON, missing required field, value out of range, unknown enum variant, cross-field violation  |
| `2`  | No feasible solution                   | GA finished but every candidate violated a hard constraint                                               |
| `3`  | No norm-compliant solution             | At least one feasible solution exists, but none passes the active norm with `strict_norm_compliance: true` |
| `4`  | Internal engine error                  | I/O failure, serialization failure, solver panic, terrain model construction error                       |

Exit code 0 plus `result.success == true` is the only "all good" signal.
Anything else, treat as a failure.

---

