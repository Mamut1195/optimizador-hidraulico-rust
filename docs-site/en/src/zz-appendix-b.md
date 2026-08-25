# Appendix B — Engine version stamp

The engine version embedded in `audit_hash` is `env!("CARGO_PKG_VERSION")`
of the `hydro-cli` crate. Bumping it changes every hash for the same inputs.
Use this intentionally to invalidate downstream caches when behavior changes.
