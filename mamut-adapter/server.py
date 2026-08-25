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

# Un endpoint POR kernel, no uno solo. El binding `mamut-http` del harness resuelve la
# URL desde el nombre de la feature y despues postea SOLO los argumentos: el nombre no
# viaja en el cuerpo. Con una ruta compartida los siete kernels llegarian
# indistinguibles, asi que el nombre va en la ruta y el handler lo lee de ahi.
KERNEL_ENDPOINT_PREFIX = "/api/hydro_engine/kernel/"

# Timeout (s) para una corrida de optimizacion NSGA-III. Generoso (matches el
# timeout=600 del manifest de referencia).
ENGINE_TIMEOUT = 600

# Los kernels de verificacion son una llamada a funcion, no una corrida NSGA-III: el
# costo entero es arrancar el proceso. Un timeout de 600 s aca convertiria un binario
# colgado en diez minutos de espera por un numero que tarda milisegundos.
KERNEL_TIMEOUT = 30

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


# Nombres de kernel que publica el binario `hydro-kernels` (espejo de su tabla
# KERNELS). La feature se llama igual que el kernel a proposito: el campo `call`
# del descriptor nombra la feature, y un segundo vocabulario en el medio seria un
# lugar mas donde desincronizarse.
def _kernel_feature(name: str, description: str, params: list[tuple]) -> dict:
    return {
        "name": name,
        "endpoint": KERNEL_ENDPOINT_PREFIX + name,
        "method": "POST",
        "description": description,
        "params": [
            {
                "name": pname,
                "type": ptype,
                "description": pdesc,
                "required": True,
            }
            for pname, ptype, pdesc in params
        ],
        "timeout": KERNEL_TIMEOUT,
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
    # --- kernels de verificacion (hydro-hydraulics) ---------------------------
    # NO son diseno: no corren NSGA-III, no eligen nada y no escriben. Son las
    # funciones de libreria del crate `hydro-hydraulics` -- Manning en canal
    # rectangular, Darcy-Weisbach, Hazen-Williams -- que ya estaban escritas y no
    # salian por ningun lado porque el binario `hydro-cli` solo entiende un
    # DesignRequest. Las sirve el binario `hydro-kernels`, que es un shell JSON
    # sobre esas mismas funciones y no calcula nada por su cuenta.
    _kernel_feature(
        "rectangular_channel_flow",
        ("Flujo uniforme en un canal rectangular por Manning: area, perimetro "
         "mojado, radio hidraulico, velocidad media, caudal, numero de Froude y "
         "regimen (subcritico / critico / supercritico). Verificacion, no diseno."),
        [
            ("width_m", "number", "Ancho de fondo b del canal (m)"),
            ("depth_m", "number", "Tirante de escurrimiento y (m)"),
            ("slope", "number", "Pendiente de fondo S (m/m); el signo se ignora"),
            ("roughness_n", "number", "Coeficiente n de Manning"),
        ],
    ),
    _kernel_feature(
        "darcy_head_loss",
        ("Perdida de carga por friccion segun Darcy-Weisbach, con el factor de "
         "friccion por la aproximacion de Swamee-Jain. El Reynolds lo deriva la "
         "propia funcion desde la velocidad y no se devuelve."),
        [
            ("velocity_m_s", "number", "Velocidad de flujo V (m/s)"),
            ("diameter_m", "number", "Diametro interno de la tuberia D (m)"),
            ("length_m", "number", "Longitud de la tuberia L (m)"),
            ("roughness_mm", "number", "Rugosidad absoluta epsilon (mm)"),
        ],
    ),
    _kernel_feature(
        "darcy_friction_factor",
        ("Factor de friccion de Darcy-Weisbach por la aproximacion de "
         "Swamee-Jain. El Reynolds es un DATO que aporta quien llama: este motor "
         "no lo calcula ni decide que hacer en la zona critica."),
        [
            ("reynolds", "number", "Numero de Reynolds (adimensional)"),
            ("roughness_mm", "number", "Rugosidad absoluta epsilon (mm)"),
            ("diameter_m", "number", "Diametro interno de la tuberia D (m)"),
        ],
    ),
    _kernel_feature(
        "hazen_williams_head_loss",
        ("Perdida de carga por friccion segun Hazen-Williams en tuberia a "
         "presion. El coeficiente C es un dato: sacalo de "
         "hazen_williams_coefficient o de la norma que declaro la obra."),
        [
            ("flow_m3s", "number", "Caudal Q (m3/s)"),
            ("diameter_m", "number", "Diametro interno de la tuberia D (m)"),
            ("length_m", "number", "Longitud de la tuberia L (m)"),
            ("c", "number", "Coeficiente C de Hazen-Williams (adimensional)"),
        ],
    ),
    _kernel_feature(
        "hazen_williams_velocity",
        ("Velocidad media en una tuberia circular llena, a partir del caudal y "
         "el diametro. Devuelve 0 cuando el diametro es cero."),
        [
            ("flow_m3s", "number", "Caudal Q (m3/s)"),
            ("diameter_m", "number", "Diametro interno de la tuberia D (m)"),
        ],
    ),
    _kernel_feature(
        "hazen_williams_coefficient",
        ("Coeficiente C de la tabla del motor para un material de tuberia, con "
         "la tabla entera en la respuesta. OJO: un nombre que no esta en la "
         "tabla devuelve 150.0 sin avisar -- compara contra known_materials."),
        [
            ("material", "string",
             "Nombre del material: PVC, PEAD, Acero, Fierro fundido, Concreto, "
             "Asbesto cemento (o STEEL, CONCRETE, CAST_IRON, HDPE)"),
        ],
    ),
    _kernel_feature(
        "hazen_williams_required_diameter",
        ("El menor diametro comercial de la lista cuya perdida de carga por "
         "Hazen-Williams no supera la carga disponible. Si ninguno cumple "
         "devuelve el mayor de la lista, sin avisar que no cumple."),
        [
            ("flow_m3s", "number", "Caudal de diseno Q (m3/s)"),
            ("length_m", "number", "Longitud de la tuberia L (m)"),
            ("available_head_m", "number", "Carga disponible (m)"),
            ("c", "number", "Coeficiente C de Hazen-Williams (adimensional)"),
            ("available_diameters_m", "list[number]",
             "Diametros comerciales candidatos (m), lista no vacia"),
        ],
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

def _candidate_binaries(stem: str = "hydro-cli", env_var: str = "HYDRO_CLI_BIN") -> list[Path]:
    """Rutas candidatas a un binario del workspace, en orden de preferencia.

    1. la variable de entorno (ruta explicita)
    2. <repo>/target/release/<stem>.exe
    3. <repo>/target/debug/<stem>.exe

    Parametrizado por `stem` desde que el repo publica DOS binarios: `hydro-cli`
    corre el optimizador NSGA-III y `hydro-kernels` expone las funciones de
    verificacion de `hydro-hydraulics`. Buscarlos con la misma escalera evita que
    uno se encuentre y el otro no por un motivo distinto en cada maquina.
    """
    here = Path(__file__).resolve().parent          # .../mamut-adapter/
    repo = here.parent                              # .../optimizador-hidraulico-rust/
    exe = f"{stem}.exe" if os.name == "nt" else stem
    cands: list[Path] = []
    env_bin = os.environ.get(env_var)
    if env_bin:
        cands.append(Path(env_bin))
    cands.append(repo / "target" / "release" / exe)
    cands.append(repo / "target" / "debug" / exe)
    return cands


def _resolve_binary(stem: str = "hydro-cli", env_var: str = "HYDRO_CLI_BIN") -> Path | None:
    for c in _candidate_binaries(stem, env_var):
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


KERNEL_NAMES = frozenset(
    feature["name"]
    for feature in FEATURES
    if feature["endpoint"].startswith(KERNEL_ENDPOINT_PREFIX)
)
"""Los kernels que este adapter acepta, derivados del manifest y no escritos al lado.

Una segunda lista literal se desincronizaria del manifest en el primer agregado, y el
sintoma seria una feature publicada que devuelve 404 -- el peor de los dos errores,
porque el harness ya la anuncio como disponible.
"""


def _run_kernel(kernel: str, arguments: dict) -> tuple[int, bytes]:
    """Corre el binario hydro-kernels: {"kernel", ...args} por stdin -> JSON por stdout.

    El nombre del kernel viaja en la RUTA (el binding postea solo los argumentos) y se
    inyecta aca en el cuerpo que lee el binario. Un argumento que el modelo ya haya
    mandado con la clave `kernel` se descarta: la ruta es la autoridad, y dejar que el
    cuerpo la contradiga seria una feature que ejecuta otra cosa que la que se pidio.
    """
    binary = _resolve_binary("hydro-kernels", "HYDRO_KERNELS_BIN")
    if binary is None:
        body = {
            "error": "binario hydro-kernels no encontrado",
            "detail": ("compila el motor con `cargo build --release` o exporta "
                       "HYDRO_KERNELS_BIN apuntando al ejecutable"),
            "searched": [
                str(c) for c in _candidate_binaries("hydro-kernels", "HYDRO_KERNELS_BIN")
            ],
        }
        return 503, json.dumps(body).encode()

    request = {k: v for k, v in arguments.items() if k != "kernel"}
    request["kernel"] = kernel
    try:
        payload = json.dumps(request).encode("utf-8")
    except (TypeError, ValueError) as e:
        return 400, json.dumps({"error": f"argumentos no serializables: {e}"}).encode()

    try:
        proc = subprocess.run(
            [str(binary)],
            input=payload,
            capture_output=True,
            timeout=KERNEL_TIMEOUT,
        )
    except subprocess.TimeoutExpired:
        return 504, json.dumps(
            {"error": f"el kernel excedio {KERNEL_TIMEOUT}s y fue abortado"}
        ).encode()
    except OSError as e:
        return 500, json.dumps({"error": f"no se pudo ejecutar el binario: {e}"}).encode()

    if proc.returncode == 0:
        # Sin traduccion a MCP: un kernel de verificacion devuelve numeros, no
        # geometria que dibujar. Adjuntar `mcp_instructions` vacias aca haria creer
        # que hay algo que mandar a Civil 3D.
        return 200, proc.stdout

    http = _EXIT_TO_HTTP.get(proc.returncode, 500)
    detail = proc.stderr.decode("utf-8", "ignore").strip() or         proc.stdout.decode("utf-8", "ignore").strip()
    body = {
        "error": f"el kernel '{kernel}' rechazo la llamada",
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

    def _body(self) -> dict | None:
        """El cuerpo POST como objeto, o None si ya se contesto el error."""
        n = int(self.headers.get("Content-Length", 0))
        raw = self.rfile.read(n) if n else b""
        try:
            req = json.loads(raw) if raw else {}
        except json.JSONDecodeError as e:
            self._send(400, json.dumps({"error": f"body no es JSON: {e}"}).encode())
            return None
        if not isinstance(req, dict):
            self._send(400, json.dumps({"error": "body debe ser un objeto JSON"}).encode())
            return None
        return req

    def do_POST(self):
        if self.path.startswith(KERNEL_ENDPOINT_PREFIX):
            kernel = self.path[len(KERNEL_ENDPOINT_PREFIX):]
            if kernel not in KERNEL_NAMES:
                return self._send(404, json.dumps({
                    "error": f"kernel no publicado: {kernel!r}",
                    "valid": sorted(KERNEL_NAMES),
                }).encode())
            req = self._body()
            if req is None:
                return None
            code, body = _run_kernel(kernel, req)
            return self._send(code, body)

        if self.path != DESIGN_ENDPOINT:
            return self._send(
                404, json.dumps({"error": f"ruta no encontrada: {self.path}"}).encode()
            )
        req = self._body()
        if req is None:
            return None

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
