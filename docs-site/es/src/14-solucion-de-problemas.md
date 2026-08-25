# Solución de problemas

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

