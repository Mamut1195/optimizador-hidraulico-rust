# Patrones de integración

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

