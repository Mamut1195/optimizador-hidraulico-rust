# CLI Hardening Findings

- `hydro-cli/src/lib.rs` contains post-validation `unwrap()` calls for source, outlet, and service-point extraction.
- The current comments rely on validation order rather than making the invariant explicit in the type/extraction boundary.
- A generic `required_field` helper can preserve the existing validation error contract and eliminate every production `unwrap()` without changing solver dispatch behavior.
