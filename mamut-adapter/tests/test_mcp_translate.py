"""Tests del traductor DesignResult(Rust) -> instrucciones MCP de Civil 3D.

Contrato verificado contra el plugin C# (PipeNetworkHandlers.cs +
PlanProductionLayoutExportHandlers.cs): todas las claves de body son camelCase.
"""

from __future__ import annotations

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from mcp_translate import (  # noqa: E402
    EP_ADD_PIPE,
    EP_ADD_STRUCTURE,
    EP_CREATE,
    EP_SET_DIAMETER,
    EP_SET_INVERT,
    design_result_to_mcp,
)


def _sample_design_result() -> dict:
    """DesignResult representativo del binario Rust (schema response.rs)."""
    return {
        "success": True,
        "best_solution": {
            "rank": 1,
            "network": {
                "name": "Red1",
                "nodes": [
                    {"id": "n1", "x": 0.0, "y": 0.0, "z": 100.0,
                     "node_type": "MANHOLE", "rim_elevation": 100.0,
                     "sump_elevation": 97.5, "demand": 5.0},
                    {"id": "n2", "x": 50.0, "y": 0.0, "z": 99.0,
                     "node_type": "OUTLET", "rim_elevation": 99.0,
                     "sump_elevation": 96.0, "demand": 0.0},
                ],
                "pipes": [
                    {"id": "p1", "start_node_id": "n1", "end_node_id": "n2",
                     "length": 50.0, "diameter": 0.3, "material": "PVC",
                     "start_invert": 97.5, "end_invert": 96.5, "slope": 0.02,
                     "waypoints": []},
                    # huerfana: nX no existe -> debe saltarse en TODO
                    {"id": "p_orphan", "start_node_id": "n1", "end_node_id": "nX",
                     "length": 10.0, "diameter": 0.2, "material": "PVC",
                     "start_invert": 97.0, "end_invert": 96.0, "slope": 0.1,
                     "waypoints": []},
                ],
            },
        },
    }


def test_emite_create_primero():
    ins = design_result_to_mcp(_sample_design_result())
    assert ins[0]["endpoint"] == EP_CREATE
    assert ins[0]["body"] == {"name": "Red1"}


def test_una_estructura_por_nodo_con_claves_camelcase():
    ins = design_result_to_mcp(_sample_design_result())
    structs = [i for i in ins if i["endpoint"] == EP_ADD_STRUCTURE]
    assert len(structs) == 2
    body = structs[0]["body"]
    assert set(body.keys()) == {"network", "x", "y", "rimElevation", "sumpElevation"}
    assert body["network"] == "Red1"
    assert body["rimElevation"] == 100.0
    assert body["sumpElevation"] == 97.5


def test_add_pipe_usa_coordenadas_de_nodos_y_camelcase():
    ins = design_result_to_mcp(_sample_design_result())
    pipes = [i for i in ins if i["endpoint"] == EP_ADD_PIPE]
    assert len(pipes) == 1  # la huerfana se salta
    body = pipes[0]["body"]
    assert set(body.keys()) == {"network", "startX", "startY", "endX", "endY"}
    assert (body["startX"], body["startY"]) == (0.0, 0.0)
    assert (body["endX"], body["endY"]) == (50.0, 0.0)


def test_huerfana_no_aparece_en_ningun_endpoint():
    ins = design_result_to_mcp(_sample_design_result())
    for i in ins:
        body = i.get("body") or {}
        assert body.get("pipeName") != "p_orphan"


def test_set_invert_camelcase():
    ins = design_result_to_mcp(_sample_design_result())
    inv = [i for i in ins if i["endpoint"] == EP_SET_INVERT]
    assert len(inv) == 1
    body = inv[0]["body"]
    assert set(body.keys()) == {"network", "pipeName", "startInvert", "endInvert"}
    assert body["pipeName"] == "p1"
    assert body["startInvert"] == 97.5
    assert body["endInvert"] == 96.5


def test_set_diameter_convierte_metros_a_mm():
    ins = design_result_to_mcp(_sample_design_result())
    diam = [i for i in ins if i["endpoint"] == EP_SET_DIAMETER]
    assert len(diam) == 1
    body = diam[0]["body"]
    assert set(body.keys()) == {"network", "pipeName", "diameter"}
    # Rust emite 0.3 m -> el plugin espera mm -> 300
    assert body["diameter"] == 300.0


def test_total_instrucciones():
    ins = design_result_to_mcp(_sample_design_result())
    # 1 create + 2 estructuras + 1 pipe + 1 invert + 1 diameter = 6
    assert len(ins) == 6


def test_usa_solutions_si_no_hay_best_solution():
    dr = _sample_design_result()
    dr["solutions"] = [dr.pop("best_solution")]
    ins = design_result_to_mcp(dr)
    assert ins[0]["endpoint"] == EP_CREATE


def test_sin_solucion_devuelve_vacio():
    assert design_result_to_mcp({"success": False}) == []
    assert design_result_to_mcp({"best_solution": {"network": None}}) == []


if __name__ == "__main__":
    import traceback

    fns = [v for k, v in sorted(globals().items()) if k.startswith("test_") and callable(v)]
    failed = 0
    for fn in fns:
        try:
            fn()
            print(f"PASS {fn.__name__}")
        except Exception:  # noqa: BLE001
            failed += 1
            print(f"FAIL {fn.__name__}")
            traceback.print_exc()
    print(f"\n{len(fns) - failed}/{len(fns)} passed")
    sys.exit(1 if failed else 0)
