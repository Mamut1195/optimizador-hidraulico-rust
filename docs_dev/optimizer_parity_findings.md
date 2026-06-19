# Optimizer Parity Findings

- Both ignored tests in `hydro-optimizer/tests/pr8f_parity_skeleton.rs` are executable placeholders, not finished tests waiting only for files.
- The HV/IGD+ test currently uses a fabricated front and duplicated local metric helpers; production hypervolume supports only two objectives while the documented sewer gate expects five objectives.
- The path parity test uses constant lengths and cannot call `PathSmoother` because the integration test sees neither the crate-private routing module nor its crate-private type.
- Existing committed oracle infrastructure lives under `tests/oracle/fixtures/`; the stale skeleton instead references the absent `contracts/examples/sewer_basic/` directory.
- Closing this gap honestly requires reproducible fixtures plus a deliberate test seam. Deleting the ignored tests or merely removing `#[ignore]` would create false confidence.
- The sibling Python oracle exists at `../motor-optimizacion-hidraulico`; its optimizer is in `src/hydro_engine/optimization/genetic.py` and its smoother in `optimization/routing.py`.
- No generator currently emits optimizer-quality or path-smoothing fixtures under `tests/oracle/gen/`.
- For the canonical blocking square, the Python oracle returns buffered waypoints `(6.9, 1.9)` and `(13.1, 1.9)` and total length `21.328780519261954`.
- Rust visibility nodes currently use the original polygon vertices while positive-clearance validation rejects edges incident to those vertices. A* can therefore return no path and the smoother silently falls back to the blocked direct segment of length `20.0`.
- The existing Rust obstacle test misses this defect because it checks only that returned waypoints are outside the polygon; the direct fallback endpoints satisfy that weak assertion.
