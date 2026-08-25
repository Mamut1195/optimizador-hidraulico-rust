# Troubleshooting

### "validation error: terrain_points must have at least 3 points"

You sent fewer than 3 entries in `terrain_points`. The engine needs at least 3
to build a terrain interpolation surface.

### "Unknown field `foo` at line N column M"

You sent a field not present in `DesignRequest`. `deny_unknown_fields` is
strict by design — extras are caught early so they cannot silently no-op.

### "Field `nsga_population_size` value 8 is outside valid range [20, 500]"

The validation floor is `pop ≥ 20`, `gen ≥ 10`, `max_time ≥ 30`. Internal
smoke tests bypass this; external callers must pass values at or above the
floor.

### Exit 2 — "no feasible solution found"

The GA explored the space but every individual violated a hard constraint.
Typical causes:

- Terrain is too steep / too flat for the slope envelope.
- Service points are unreachable from outlet/source given `max_depth` /
  `min_cover`.
- `min_velocity` is too high for the available diameters.
- Too few generations to find a feasible region.

Mitigations: widen `constraints` ranges, increase `nsga_generations`, raise
`nsga_max_time_seconds`, or revisit the terrain inputs.

### Exit 3 — "norm compliance failed"

There were feasible solutions, but none cleared every hard rule of the active
norm. Options: set `strict_norm_compliance: false` (you'll get exit 0 with
non-zero `norm_violations` in the result), or relax the inputs.

### Exit 4 — "internal error: …"

The binary itself or a solver dispatched an unrecoverable error. Reproduce
with the same seed, capture stderr, file an issue. The `audit_hash` will
pin the exact input.

### `--validate-only` reports "valid" but the full run exits 2 or 3

That is correct behavior. `--validate-only` checks **schema and bounds**,
not whether the optimizer can find a solution. The optimizer needs the GA
to actually run.

### Different machines produce different `audit_hash`

If `request`, `seed`, and engine version are identical, hashes must match.
If they don't:

- Re-check JSON byte-for-byte (whitespace doesn't matter, but field order
  inside the request body could if you serialized differently — the engine
  serializes via serde's stable order).
- Confirm the binary version (`hydro-cli --version`) matches across machines.

### Output JSON has missing fields you expected

Look for `#[serde(default)]` in `hydro-types/src/response.rs`. Fields that
default to empty objects or empty arrays are omitted on serialization only
when `skip_serializing_if` is set; check the relevant field.

---

