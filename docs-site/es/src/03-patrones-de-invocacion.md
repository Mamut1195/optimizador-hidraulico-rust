# Patrones de invocación

### Con archivos (lo más común)

```bash
hydro-cli --input request.json --output result.json
hydro-cli -i request.json -o result.json
```

### Con tuberías (scripting / composición unix)

```bash
cat request.json | hydro-cli > result.json
hydro-cli < request.json > result.json
```

### Mixto

```bash
hydro-cli --input request.json > result.json     # entrada desde archivo, salida a stdout
cat request.json | hydro-cli --output result.json # entrada desde stdin, salida a archivo
```

### Solo validación (sin optimización)

```bash
hydro-cli --input request.json --validate-only
# stdout en éxito:  {"status":"valid","project_type":"sewer"}
# stdout en error:  {"status":"invalid","error":"..."}
# Tiempo de pared: <50 ms (sin GA, sin solver, sin modelo de terreno)
```

### Corrida determinista (reproducible)

```bash
hydro-cli --input request.json --output result.json --seed 42
# O pasar el seed en el cuerpo del request; --seed prevalece si ambos están presentes.
```

### Salida legible por humanos

```bash
hydro-cli --input request.json --output result.json --pretty
```

---

