# Invocation patterns

### Files (most common)

```bash
hydro-cli --input request.json --output result.json
hydro-cli -i request.json -o result.json
```

### Pipes (scripting / unix composition)

```bash
cat request.json | hydro-cli > result.json
hydro-cli < request.json > result.json
```

### Mixed

```bash
hydro-cli --input request.json > result.json     # input from file, output to stdout
cat request.json | hydro-cli --output result.json # input from stdin, output to file
```

### Validation only (no optimization)

```bash
hydro-cli --input request.json --validate-only
# stdout: {"status":"valid","project_type":"sewer"} on success
# stdout: {"status":"invalid","error":"..."} on failure
# Wall time: <50 ms (no GA, no solver, no terrain model)
```

### Deterministic run (reproducible)

```bash
hydro-cli --input request.json --output result.json --seed 42
# Or pass seed in the request body; --seed overrides if both are present.
```

### Human-readable output

```bash
hydro-cli --input request.json --output result.json --pretty
```

---

