# Códigos de salida

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

