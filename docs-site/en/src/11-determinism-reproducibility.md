# Determinism & reproducibility

The engine is deterministic when the seed is fixed:

```
seed_resolution = --seed CLI flag (if set) > request.seed (if set) > random
audit_hash      = lowercase_hex(SHA-256(
                    serde_json::to_string(&request)
                    || "|"
                    || seed_decimal_string
                    || "|"
                    || engine_version_string
                  ))
```

Properties:

- Same `request` JSON + same `seed` + same engine version → identical
  `result.solutions` (byte-for-byte under JSON round-trip).
- The hash is sensitive to seed changes: flipping the seed changes the hash.
- The hash is sensitive to engine version (`CARGO_PKG_VERSION`); upgrading
  the binary intentionally changes the hash even with identical inputs.

Use the hash to:

- Tag stored result documents (audit trail).
- Detect input drift in regression tests (input changed → new hash).
- Pin a "known-good run" for engineering sign-off.

The engine's parallel work runs on `rayon`. The seed propagates into the
GA's RNG, and the deterministic graph layer (introduced in Phase 7.5)
guarantees byte-identical topological order across runs.

---

