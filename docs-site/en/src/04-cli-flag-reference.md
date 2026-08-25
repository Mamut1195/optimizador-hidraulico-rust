# CLI flag reference

| Flag                | Short | Type     | Default  | Behavior                                                            |
| ------------------- | ----- | -------- | -------- | ------------------------------------------------------------------- |
| `--input <PATH>`    | `-i`  | PathBuf  | (stdin)  | Read `DesignRequest` JSON from this file. Absent → read stdin.      |
| `--output <PATH>`   | `-o`  | PathBuf  | (stdout) | Write `DesignResult` JSON to this file. Absent → write stdout.      |
| `--seed <N>`        |       | u64      | none     | Override `DesignRequest.seed` before running the optimizer.         |
| `--pretty`          |       | flag     | off      | Pretty-print the JSON output (2-space indent). Default: compact.    |
| `--validate-only`   |       | flag     | off      | Run `validate_request()` only. Skip optimizer. Emits short status.  |
| `--help` / `-h`     |       | flag     | —        | Print help and exit 0.                                              |
| `--version` / `-V`  |       | flag     | —        | Print version and exit 0.                                           |

I/O errors (file-not-found, permission denied, broken pipe on stdout) exit with
code **4**, with a message on stderr. Engine-level errors are routed through
`CliError` and surface as exit codes 1–4 (see next section).

---

