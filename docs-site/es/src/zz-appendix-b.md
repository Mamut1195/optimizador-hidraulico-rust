# Apéndice B — Sello de versión del motor

La versión del motor embebida en `audit_hash` es `env!("CARGO_PKG_VERSION")`
del crate `hydro-cli`. Subirla cambia todos los hashes para las mismas
entradas. Usalo intencionalmente para invalidar cachés aguas abajo cuando
el comportamiento cambia.
