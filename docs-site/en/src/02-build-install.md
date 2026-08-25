# Build & install

```bash
# Clone & build (release)
git clone <repo>
cd optimizador-hidraulico-rust
cargo build --release -p hydro-cli

# The binary lands at:
# Windows:   target/release/hydro-cli.exe
# Unix-like: target/release/hydro-cli
```

Rust toolchain: **1.95.0** (pinned in CI). Workspace deps include `petgraph`,
`rayon`, `clap`, `serde_json`, `sha2`. The release binary is a single file
with no runtime dependencies.

Verify:

```bash
./target/release/hydro-cli --version
./target/release/hydro-cli --help
```

---

