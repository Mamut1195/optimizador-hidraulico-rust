# Findings: Full Audit

## Automated checks run
- `cargo test --workspace`: passed. 2 ignored optimizer parity tests plus 1 ignored doc-test.
- `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- `cargo fmt --all -- --check`: passed.
- `cargo tree -d`: duplicate versions in rand/getrandom/hashbrown/itertools, mostly due dev dependencies/proptest/insta plus geo/spade/petgraph transitive graph.
- `cargo audit`: blocked because cargo-audit is not installed; attempted install timed out before completion.

## Prior audit context
- Previous post-v1 audit found 508/510 passing tests and 2 ignored parity fixtures.
- Solver NodeIndex migration has since been merged to main in PR #45.

## Working tree note
- Pre-existing dirty `.atl` files and untracked docs/docs-site/TESTDATA were present before this audit.
- This audit created docs_dev/audit_* files and docs_dev/audit_scan_patterns.txt.
## Additional security checks
- `cargo-audit v0.22.2`: passed, 163 Cargo.lock dependencies scanned, 1134 advisories loaded, no vulnerabilities reported.
- Secret-pattern scan: see docs_dev/audit_secret_scan.txt.

## 2026-06-19 — Work unit 1 findings
- `cargo-deny` initially failed because the generated template allowed no licenses and `wildcards = deny` treats workspace path dependencies as wildcard dependencies.
- Final policy allows common permissive licenses used by the current graph and keeps duplicate dependency versions as warnings for the later dependency-hygiene slice.
