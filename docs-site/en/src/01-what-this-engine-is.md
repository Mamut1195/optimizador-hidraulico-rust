# What this engine is

A standalone hydraulic optimization engine that:

- Takes a single `DesignRequest` JSON document describing terrain, demand,
  constraints, and optimizer settings.
- Runs an NSGA-III multi-objective genetic optimizer (5 objectives:
  cost, excavation, pumping, interference, resilience) against the appropriate
  domain solver (sewer, water supply, conveyance, distribution, pump station,
  or intake).
- Returns a `DesignResult` JSON document with ranked Pareto-optimal pipe
  networks, scores, diagnostics, and an audit hash.

The engine is **a binary, not a library**. The CLI is the only stable contract.
Any downstream consumer — REST server, GUI, Civil 3D plugin, automation script —
talks to it through stdin/stdout or files.

Out of scope for v1: WASM build, C-ABI FFI, embedded HTTP server, EPANET/SWMM
interop, GUI. Each of these would be a future phase that wraps this binary.

---

