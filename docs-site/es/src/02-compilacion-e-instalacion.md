# Compilación e instalación

```bash
# Clonar y compilar (release)
git clone <repo>
cd optimizador-hidraulico-rust
cargo build --release -p hydro-cli

# El binario queda en:
# Windows:    target/release/hydro-cli.exe
# Tipo Unix:  target/release/hydro-cli
```

Toolchain de Rust: **1.95.0** (fijada en CI). Las dependencias del workspace
incluyen `petgraph`, `rayon`, `clap`, `serde_json`, `sha2`. El binario release
es un único archivo sin dependencias en tiempo de ejecución.

Verificar:

```bash
./target/release/hydro-cli --version
./target/release/hydro-cli --help
```

---

