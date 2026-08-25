"""Capa MAMUT del Optimizador Hidraulico (Rust) — adapter en Python.

El optimizador es un BINARIO CLI (NSGA-III): lee un DesignRequest JSON por stdin
y emite un Solution/DesignResult JSON por stdout. NO tiene /health, /manifest ni
anuncio MAMUT. Este adapter pone esa capa por delante (molde: mamut_server.py de
Civil3D + scaffolder --kind motor):

  - escribe el anuncio en %LOCALAPPDATA%/MAMUT/plugins/hydro_engine.json
    (atomico tmp+replace), puerto DINAMICO (bind 127.0.0.1:0), heartbeat 30s;
  - sirve GET /health -> {"status":"ok"} y GET /manifest (forma 'features');
  - al recibir una design feature, CORRE el binario Rust por subprocess
    (DesignRequest JSON por stdin -> Solution JSON por stdout) y mapea los exit
    codes del binario a HTTP.

Principio MAMUT (seccion 18): la capa MAMUT/MCP va en Python (reusable); el
motor pesado (Rust) es solo el ejecutor. Se hablan por subprocess. Zero-dep en
el core (stdlib), y desde que el gate de licencia se retiro NO hay ningun import
externo: este adapter es stdlib puro.

GATE DE LICENCIA RETIRADO (MAM-236, 2026-08-25). `validate_license()` corria
ANTES de abrir el socket, con tier 'proprietary'. Los motores MAMUT son open
source (Apache-2.0) y lo que se cobra es el gateway, no el motor -- asi que el
gate contradecia la licencia del propio repo. Y tenia un efecto que ningun
ingeniero podia diagnosticar: sin MAMUT_LICENSE_TOKEN el socket no abria, el
anuncio no se escribia, y el harness no podia DESCUBRIR el motor. No fallaba al
llamarlo: no existia. `license_guard.py` sigue en el repo, sin invocarse, por si
el gateway lo reusa.

Arranque:  python server.py     (desde mamut-adapter/)
           o:  python -m server
"""

from __future__ import annotations

import json
import os
import socket
import subprocess
import sys
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

from mcp_translate import design_result_to_mcp

NAME = "hydro_engine"
DISPLAY = "Motor de Optimizacion Hidraulica (Rust)"

# Endpoints reales que sirve este adapter.
DESIGN_ENDPOINT = "/api/hydro_engine/design"
SOLVERS_ENDPOINT = "/api/hydro_engine/solvers"

# Timeout (s) para una corrida de optimizacion NSGA-III. Generoso (matches el
# timeout=600 del manifest de referencia).
ENGINE_TIMEOUT = 600

# project_type validos (espejo de hydro-types/src/request.rs ProjectTypeStr::VALID).
VALID_PROJECT_TYPES = (
    "sewer",
    "water_supply",
    "conveyance",
    "distribution",
    "pump_station",
    "intake",
)

# Exit codes del binario (hydro-cli/src/main.rs) -> HTTP.
#   0 ok | 1 validacion | 2 sin solucion factible | 3 falla de norma | 4 interno/IO
_EXIT_TO_HTTP = {1: 422, 2: 422, 3: 422, 4: 500}


# --- features MAMUT (forma 'features', description >= 20 chars). -----------------
# Descripciones reusadas LITERALMENTE de
# motor-optimizacion-hidraulico/src/hydro_engine/api/manifest_data.py
# (no se inventan): las 6 design features + el listado de solvers.

def _design_feature(name: str, project_type: str, description: str) -> dict:
    return {
        "name": name,
        "endpoint": DESIGN_ENDPOINT,
        "method": "POST",
        "description": description,
        "params": [
            {
                "name": "project_type",
                "type": "string",
                "description": f"Fijo: {project_type!r} para esta feature",
                "default": project_type,
                "enum": [project_type],
                "required": True,
            },
            {
                "name": "terrain_points",
                "type": "list[dict]",
                "description": "Puntos de terreno [{x,y,z}, ...] (REQUERIDO, minimo 3 puntos)",
                "required": True,
            },
            {
                "name": "service_points",
                "type": "list[dict]",
                "description": "Puntos de servicio a conectar [{x,y}, ...]",
                "required": False,
            },
            {
                "name": "outlet",
                "type": "dict",
                "description": "Punto de descarga {x,y} para alcantarillado",
                "required": False,
            },
            {
                "name": "source",
                "type": "dict",
                "description": "Punto de fuente/tanque {x,y} para agua potable",
                "required": False,
            },
            {
                "name": "num_alternatives",
                "type": "int",
                "description": "Numero de alternativas a generar (1-10, default 3)",
                "required": False,
            },
            {
                "name": "norm",
                "type": "string",
                "description": "Norma de diseno: CONAGUA, EPA, custom (default CONAGUA)",
                "required": False,
            },
        ],
        "timeout": ENGINE_TIMEOUT,
    }


FEATURES = [
    _design_feature(
        "sewer_design",
        "sewer",
        ("Disena una red de alcantarillado sanitario o pluvial optimizada con "
         "NSGA-III. Calcula diametros, pendientes, velocidades y capacidad con "
         "ecuaciones de Manning. Retorna geometria, profundidades e instrucciones "
         "MCP para dibujar en Civil 3D."),
    ),
    _design_feature(
        "water_supply_design",
        "water_supply",
        ("Disena una red de agua potable optimizada con NSGA-III. Calcula "
         "diametros y presiones con Hazen-Williams. Retorna geometria, "
         "diametros e instrucciones MCP para dibujar en Civil 3D."),
    ),
    _design_feature(
        "conveyance_design",
        "conveyance",
        ("Disena una linea de conduccion (transporte por gravedad o bombeo) "
         "optimizada con NSGA-III. Calcula trazado, diametros y pendientes. "
         "Retorna geometria e instrucciones MCP para Civil 3D."),
    ),
    _design_feature(
        "distribution_design",
        "distribution",
        ("Disena una red de distribucion (mallas/ramales con multiples nodos "
         "de consumo) optimizada con NSGA-III. Calcula diametros y presiones. "
         "Retorna geometria e instrucciones MCP para Civil 3D."),
    ),
    _design_feature(
        "pump_station_design",
        "pump_station",
        ("Disena una estacion de bombeo (curva, potencia, eficiencia, OPEX de "
         "ciclo de vida) optimizada con NSGA-III. Retorna selecciones de bomba "
         "e instrucciones MCP para Civil 3D."),
    ),
    _design_feature(
        "intake_design",
        "intake",
        ("Disena una obra de toma (captacion de fuente superficial o "
         "subterranea) optimizada con NSGA-III. Retorna geometria de la "
         "estructura e instrucciones MCP para Civil 3D."),
    ),
    {
        "name": "solvers",
        "endpoint": SOLVERS_ENDPOINT,
        "method": "GET",
        "description": ("Lista los tipos de solver disponibles (alcantarillado, agua potable, "
                        "conduccion, distribucion, bombeo, obra de toma) y sus clases."),
        "params": [],
        "timeout": 30,
    },
]


def manifest() -> dict:
    return {
        "protocol": "mamut-plugin",
        "protocol_version": "1.0",
        "name": NAME,
        "display_name": DISPLAY,
        "required": False,
        "features": FEATURES,
    }


# --- localizacion del binario Rust ----------------------------------------------

def _candidate_binaries() -> list[Path]:
    """Rutas candidatas al binario hydro-cli, en orden de preferencia.

    1. env HYDRO_CLI_BIN (ruta explicita)
    2. <repo>/target/release/hydro-cli.exe
    3. <repo>/target/debug/hydro-cli.exe
    """
    here = Path(__file__).resolve().parent          # .../mamut-adapter/
    repo = here.parent                              # .../optimizador-hidraulico-rust/
    exe = "hydro-cli.exe" if os.name == "nt" else "hydro-cli"
    cands: list[Path] = []
    env_bin = os.environ.get("HYDRO_CLI_BIN")
    if env_bin:
        cands.append(Path(env_bin))
    cands.append(repo / "target" / "release" / exe)
    cands.append(repo / "target" / "debug" / exe)
    return cands


def _resolve_binary() -> Path | None:
    for c in _candidate_binaries():
        if c.is_file():
            return c
    return None


# --- ejecucion del motor por subprocess -----------------------------------------

def _run_engine(design_request: dict) -> tuple[int, bytes]:
    """Corre el binario Rust: DesignRequest JSON (stdin) -> Solution JSON (stdout).

    Comando exacto (sin flags -> lee stdin / escribe stdout):
        hydro-cli.exe < DesignRequest.json > Solution.json

    Devuelve (codigo_http, cuerpo_bytes). Mapea los exit codes del binario a HTTP.
    """
    binary = _resolve_binary()
    if binary is None:
        body = {
            "error": "binario hydro-cli no encontrado",
            "detail": ("compila el motor con `cargo build --release` o exporta "
                       "HYDRO_CLI_BIN apuntando al ejecutable"),
            "searched": [str(c) for c in _candidate_binaries()],
        }
        return 503, json.dumps(body).encode()

    try:
        payload = json.dumps(design_request).encode("utf-8")
    except (TypeError, ValueError) as e:
        return 400, json.dumps({"error": f"DesignRequest no serializable: {e}"}).encode()

    try:
        proc = subprocess.run(
            [str(binary)],
            input=payload,
            capture_output=True,
            timeout=ENGINE_TIMEOUT,
        )
    except subprocess.TimeoutExpired:
        return 504, json.dumps(
            {"error": f"el motor excedio {ENGINE_TIMEOUT}s y fue abortado"}
        ).encode()
    except OSError as e:
        return 500, json.dumps({"error": f"no se pudo ejecutar el binario: {e}"}).encode()

    if proc.returncode == 0:
        # El binario Rust emite geometria CRUDA, sin instrucciones MCP (spec
        # S-23). Esta capa MAMUT traduce esa geometria a las llamadas que
        # entiende el plugin de Civil 3D y las adjunta como `mcp_instructions`.
        try:
            result = json.loads(proc.stdout)
        except (json.JSONDecodeError, ValueError):
            # Si el stdout no es JSON parseable, devolvemos el crudo tal cual.
            return 200, proc.stdout
        if isinstance(result, dict):
            result["mcp_instructions"] = design_result_to_mcp(result)
        return 200, json.dumps(result).encode()

    # Error: mapear exit code -> HTTP y devolver stderr (o stdout) como detalle.
    http = _EXIT_TO_HTTP.get(proc.returncode, 500)
    detail = proc.stderr.decode("utf-8", "ignore").strip() or \
        proc.stdout.decode("utf-8", "ignore").strip()
    body = {
        "error": "el motor reporto un fallo",
        "engine_exit_code": proc.returncode,
        "detail": detail,
    }
    return http, json.dumps(body).encode()


def _solvers_payload() -> bytes:
    """Listado estatico de solvers disponibles (espejo de project_type validos)."""
    labels = {
        "sewer": "Alcantarillado sanitario/pluvial",
        "water_supply": "Agua potable",
        "conveyance": "Linea de conduccion",
        "distribution": "Red de distribucion",
        "pump_station": "Estacion de bombeo",
        "intake": "Obra de toma",
    }
    solvers = [{"project_type": pt, "label": labels[pt]} for pt in VALID_PROJECT_TYPES]
    return json.dumps({"solvers": solvers}).encode()


# --- lifecycle MAMUT (molde Civil3D) --------------------------------------------

def free_port() -> int:
    s = socket.socket()
    s.bind(("127.0.0.1", 0))          # 127.0.0.1:0 -> el OS asigna un puerto libre
    port = s.getsockname()[1]
    s.close()
    return port


def _announcement_path() -> Path:
    d = Path(os.environ["LOCALAPPDATA"]) / "MAMUT" / "plugins"
    return d / f"{NAME}.json"


def write_announcement(port: int) -> None:
    path = _announcement_path()
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_suffix(".json.tmp")
    tmp.write_text(
        json.dumps({"name": NAME, "port": port, "pid": os.getpid()}),
        encoding="utf-8",
    )
    os.replace(tmp, path)             # swap atomico


def _heartbeat(port: int) -> None:
    while True:
        time.sleep(30)
        try:
            write_announcement(port)
        except OSError:
            pass


class Handler(BaseHTTPRequestHandler):
    def _send(self, code: int, raw: bytes) -> None:
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(raw)))
        self.end_headers()
        self.wfile.write(raw)

    def do_GET(self):
        if self.path == "/health":
            return self._send(200, json.dumps({"status": "ok"}).encode())
        if self.path == "/manifest":
            return self._send(200, json.dumps(manifest()).encode())
        if self.path == SOLVERS_ENDPOINT:
            return self._send(200, _solvers_payload())
        self._send(404, json.dumps({"error": f"ruta no encontrada: {self.path}"}).encode())

    def do_POST(self):
        if self.path != DESIGN_ENDPOINT:
            return self._send(
                404, json.dumps({"error": f"ruta no encontrada: {self.path}"}).encode()
            )
        n = int(self.headers.get("Content-Length", 0))
        raw = self.rfile.read(n) if n else b""
        try:
            req = json.loads(raw) if raw else {}
        except json.JSONDecodeError as e:
            return self._send(400, json.dumps({"error": f"body no es JSON: {e}"}).encode())
        if not isinstance(req, dict):
            return self._send(400, json.dumps({"error": "body debe ser un objeto JSON"}).encode())

        pt = req.get("project_type")
        if pt not in VALID_PROJECT_TYPES:
            return self._send(422, json.dumps({
                "error": "project_type invalido o ausente",
                "valid": list(VALID_PROJECT_TYPES),
            }).encode())

        code, body = _run_engine(req)
        self._send(code, body)

    def log_message(self, *a):
        pass


def main() -> None:
    port = free_port()
    httpd = ThreadingHTTPServer(("127.0.0.1", port), Handler)
    write_announcement(port)
    threading.Thread(target=_heartbeat, args=(port,), daemon=True).start()

    binary = _resolve_binary()
    bin_note = str(binary) if binary else "NO ENCONTRADO (design devolvera 503)"
    sys.stderr.write(
        f"[hydro_engine] capa MAMUT en http://127.0.0.1:{port} "
        f"-> subprocess al binario: {bin_note} (anunciado)\n"
    )
    try:
        httpd.serve_forever()
    finally:
        p = _announcement_path()
        if p.exists():
            p.unlink()


if __name__ == "__main__":
    main()
