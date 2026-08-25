# Referencia de banderas de CLI

| Bandera             | Corta | Tipo     | Defecto  | Comportamiento                                                          |
| ------------------- | ----- | -------- | -------- | ----------------------------------------------------------------------- |
| `--input <PATH>`    | `-i`  | PathBuf  | (stdin)  | Lee el JSON `DesignRequest` desde este archivo. Si falta, lee de stdin. |
| `--output <PATH>`   | `-o`  | PathBuf  | (stdout) | Escribe el JSON `DesignResult` a este archivo. Si falta, escribe stdout.|
| `--seed <N>`        |       | u64      | ninguno  | Sobrescribe `DesignRequest.seed` antes de correr el optimizador.        |
| `--pretty`          |       | bandera  | off      | Imprime el JSON de salida con sangría de 2 espacios. Defecto: compacto. |
| `--validate-only`   |       | bandera  | off      | Ejecuta solo `validate_request()`. No corre el optimizador.             |
| `--help` / `-h`     |       | bandera  | —        | Muestra ayuda y sale con código 0.                                      |
| `--version` / `-V`  |       | bandera  | —        | Muestra la versión y sale con código 0.                                 |

Los errores de E/S (archivo no encontrado, permiso denegado, tubería rota en
stdout) salen con código **4** y un mensaje en stderr. Los errores a nivel
del motor se enrutan a través de `CliError` y emergen como códigos de salida
1 a 4 (ver siguiente sección).

---

