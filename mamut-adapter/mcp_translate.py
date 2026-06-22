"""Traductor Solution(Rust) -> instrucciones MCP para Civil 3D.

El binario Rust (hydro-cli) emite un `DesignResult` JSON con geometria CRUDA y
DELIBERADAMENTE sin instrucciones MCP (spec S-23 del motor lo prohibe: el motor
calcula, no sabe de Civil 3D). Esta capa MAMUT (Python) es la que traduce esa
geometria a las llamadas HTTP que entiende el plugin de Civil 3D.

FUENTE DE VERDAD del contrato (verificado en codigo, NO copiado del motor Python
que esta fuera de arquitectura): el plugin C# de Civil3D-MCP
  CIVIL3D-MCP/src/Civil3DMCP.Plugin/Handlers/PipeNetworkHandlers.cs
  CIVIL3D-MCP/src/Civil3DMCP.Plugin/Handlers/PlanProductionLayoutExportHandlers.cs
Los handlers leen el body con estas CLAVES EXACTAS (todas camelCase):
  POST /api/pipe-networks/create        -> {name}
  POST /api/pipe-networks/add-structure -> {network, x, y, rimElevation, sumpElevation}
  POST /api/pipe-networks/add-pipe      -> {network, startX, startY, endX, endY}
  POST /api/pipe-networks/set-invert    -> {network, pipeName, startInvert, endInvert}
  POST /api/pipe-networks/set-diameter  -> {network, pipeName, diameter}

(OJO: el proxy FastMCP de Civil3D-MCP `pipe_networks/tools.py` manda snake_case
`start_x`/`pipe_name` — eso es un BUG latente de ESE proxy contra su propio
plugin C#. Nosotros emitimos lo que el plugin realmente lee: camelCase.)

DECISIONES de mapeo (documentadas a proposito):
  - diameter: el Rust lo emite en METROS; Civil 3D trabaja en "drawing units
    (typically millimeters)" segun el propio MCP. Emitimos en MILIMETROS (x1000).
  - waypoints: el plugin `add-pipe` solo dibuja un segmento recto start->end
    (LineSegment3d), asi que la forma intermedia (waypoints) NO se representa por
    ahora. Se anota; si se quiere la polilinea exacta habria que usar
    /api/drawing/polyline ademas. No se inventa.
  - pipes huerfanas (start/end node id que no existe en nodes[]) se SALTAN, igual
    que set-invert/set-diameter sobre ellas, para no provocar "pipe not found".

Salida: lista de dicts {method, endpoint, body, description} — el mismo shape del
contrato MCPInstruction. Es JSON puro, stdlib, sin dependencias.
"""

from __future__ import annotations

from typing import Any

# Endpoints del plugin Civil3D-MCP (los que registra McpHttpServer.cs).
EP_CREATE = "/api/pipe-networks/create"
EP_ADD_STRUCTURE = "/api/pipe-networks/add-structure"
EP_ADD_PIPE = "/api/pipe-networks/add-pipe"
EP_SET_INVERT = "/api/pipe-networks/set-invert"
EP_SET_DIAMETER = "/api/pipe-networks/set-diameter"

# El Rust emite diametros en metros; el plugin espera drawing units (mm).
_METERS_TO_MM = 1000.0


def _pick_solution(design_result: dict[str, Any]) -> dict[str, Any] | None:
    """Elige la solucion a dibujar: best_solution, si no la primera de solutions[]."""
    best = design_result.get("best_solution")
    if isinstance(best, dict):
        return best
    sols = design_result.get("solutions")
    if isinstance(sols, list) and sols and isinstance(sols[0], dict):
        return sols[0]
    return None


def design_result_to_mcp(
    design_result: dict[str, Any],
    *,
    solution: dict[str, Any] | None = None,
) -> list[dict[str, Any]]:
    """Traduce un DesignResult del Rust (o una Solution suelta) a instrucciones MCP.

    Args:
        design_result: el JSON que emite hydro-cli por stdout (dict ya parseado).
        solution: opcional, una Solution concreta a dibujar; si no se pasa, se usa
            best_solution (o solutions[0]).

    Returns:
        Lista ordenada de instrucciones {method, endpoint, body, description}.
        Lista vacia si no hay solucion o no hay red.
    """
    sol = solution if solution is not None else _pick_solution(design_result)
    if not isinstance(sol, dict):
        return []

    network = sol.get("network")
    if not isinstance(network, dict):
        return []

    nodes = network.get("nodes") or []
    pipes = network.get("pipes") or []
    net_name = network.get("name") or "HydroNetwork"

    instructions: list[dict[str, Any]] = []

    # 1) crear la red
    instructions.append({
        "method": "POST",
        "endpoint": EP_CREATE,
        "body": {"name": net_name},
        "description": f"Crear red de tuberias '{net_name}'",
    })

    # lookup de nodos por id (para resolver coordenadas de cada tramo)
    by_id: dict[Any, dict[str, Any]] = {}
    for node in nodes:
        if isinstance(node, dict) and "id" in node:
            by_id[node["id"]] = node

    # 2) estructuras (pozos/uniones)
    for node in nodes:
        if not isinstance(node, dict):
            continue
        x = node.get("x", 0.0)
        y = node.get("y", 0.0)
        instructions.append({
            "method": "POST",
            "endpoint": EP_ADD_STRUCTURE,
            "body": {
                "network": net_name,
                "x": x,
                "y": y,
                "rimElevation": node.get("rim_elevation", node.get("z", 0.0)),
                "sumpElevation": node.get("sump_elevation", 0.0),
            },
            "description": f"Estructura {node.get('id')} en ({x:.1f}, {y:.1f})",
        })

    # 3) tramos: solo los que tienen ambos nodos conocidos (las huerfanas se saltan)
    emitted: list[dict[str, Any]] = []
    for pipe in pipes:
        if not isinstance(pipe, dict):
            continue
        start = by_id.get(pipe.get("start_node_id"))
        end = by_id.get(pipe.get("end_node_id"))
        if start is None or end is None:
            continue
        emitted.append(pipe)
        instructions.append({
            "method": "POST",
            "endpoint": EP_ADD_PIPE,
            "body": {
                "network": net_name,
                "startX": start.get("x", 0.0),
                "startY": start.get("y", 0.0),
                "endX": end.get("x", 0.0),
                "endY": end.get("y", 0.0),
            },
            "description": (
                f"Tramo {pipe.get('id')}: "
                f"{pipe.get('start_node_id')} -> {pipe.get('end_node_id')}"
            ),
        })

    # 4) cotas plantilla (invert) — solo de los tramos emitidos
    for pipe in emitted:
        instructions.append({
            "method": "POST",
            "endpoint": EP_SET_INVERT,
            "body": {
                "network": net_name,
                "pipeName": pipe.get("id"),
                "startInvert": pipe.get("start_invert", 0.0),
                "endInvert": pipe.get("end_invert", 0.0),
            },
            "description": (
                f"Cotas {pipe.get('id')}: "
                f"{pipe.get('start_invert', 0.0):.2f} -> {pipe.get('end_invert', 0.0):.2f}"
            ),
        })

    # 5) diametros — Rust en metros -> plugin en mm
    for pipe in emitted:
        diameter_mm = round(float(pipe.get("diameter", 0.0)) * _METERS_TO_MM, 1)
        instructions.append({
            "method": "POST",
            "endpoint": EP_SET_DIAMETER,
            "body": {
                "network": net_name,
                "pipeName": pipe.get("id"),
                "diameter": diameter_mm,
            },
            "description": f"Diametro {pipe.get('id')}: {diameter_mm:.0f}mm",
        })

    return instructions
