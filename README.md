# optimizador-hidraulico-rust

Pure-compute Rust port of the hydraulic optimization **engine** (engine only — no API, no GUI).

Reads a `DesignRequest` JSON, runs multi-objective design optimization (NSGA-III) over terrain,
hydraulics, norms and solvers, and emits a `Solution` JSON (geometry + diagnostics). A separate
bridge/API is responsible for any Civil 3D integration.

## Status

v1 in progress. Faithful 1:1 port of the Python engine, validated against it as a differential
oracle (exact parity for hydraulics/norms; statistical parity for solver topology and NSGA-III).

## Workspace

| Crate | Responsibility |
|-------|----------------|
| `hydro-types` | Domain types: nodes, pipes, network, constraints, request/solution contracts |
| `hydro-hydraulics` | Darcy-Weisbach, Hazen-Williams, Manning, open-channel, pump curves |
| `hydro-terrain` | KDTree, grid interpolation, graph, Steiner tree |
| `hydro-norms` | Norm profiles (CONAGUA, EPA, …) and validation |
| `hydro-solvers` | Sewer, water supply, conveyance, distribution, pump station, intake |
| `hydro-optimizer` | NSGA-III, encoding, objectives, constraints |
| `hydro-cli` | Thin JSON-in / JSON-out command-line boundary |

## Build & test

```sh
cargo build
cargo test
cargo clippy -- -D warnings
cargo fmt --check
```

## Out of scope (v1)

WASM, C-ABI FFI, API, GUI, Civil 3D bridge, EPANET/SWMM runtime (deferred).
