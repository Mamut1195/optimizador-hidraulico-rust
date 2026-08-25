# Integration patterns

The engine is a single binary. Wrap it as you like.

### REST wrapper (sketch)

```python
# Python FastAPI sketch
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

Map exit codes:

| Exit | HTTP equivalent | Reason                      |
| ---- | --------------- | --------------------------- |
| 0    | 200             | success                     |
| 1    | 400             | bad request (validation)    |
| 2    | 422             | no feasible solution        |
| 3    | 422             | norm-compliance failure     |
| 4    | 500             | internal engine error       |

### GUI / desktop (sketch)

Spawn `hydro-cli` as a child process, pipe a `DesignRequest` to stdin, read
`DesignResult` from stdout. The GUI never owns the optimizer — it always
delegates to the binary.

### Civil 3D / GIS plugin (sketch)

1. Export terrain mesh to `terrain_points`.
2. Export service-point layer to `service_points`.
3. Spawn `hydro-cli` with the JSON.
4. Parse `result.best_solution.network`; render nodes/pipes back into the
   drawing as native objects.

### Batch processing (CI / scheduled runs)

```bash
for fixture in fixtures/*.json; do
  hydro-cli -i "$fixture" -o "results/$(basename "$fixture" .json).result.json" \
            --seed 42 --pretty
done
```

The `audit_hash` in every result lets you confirm reproducibility across
machines and times.

---

