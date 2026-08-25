# mamut-adapter — Capa MAMUT del Optimizador Hidraulico (Rust)

El optimizador (`hydro-cli`) es un **binario CLI** (NSGA-III): lee un
`DesignRequest` JSON por **stdin** y emite un `DesignResult`/`Solution` JSON por
**stdout**. No tiene `/health`, `/manifest` ni anuncio MAMUT.

Este adapter en Python pone esa capa por delante (molde: `mamut_server.py` de
Civil3D). Es la pieza que el agente MAMUT descubre y consume.

## Que hace

- Escribe el anuncio en `%LOCALAPPDATA%/MAMUT/plugins/hydro_engine.json`
  (atomico tmp+replace), **puerto dinamico** (`bind 127.0.0.1:0`), heartbeat 30s.
- Sirve `GET /health` -> `{"status":"ok"}` y `GET /manifest` (forma `features`).
- Al recibir una design feature, **corre el binario Rust por subprocess**
  (`DesignRequest` por stdin -> `DesignResult` por stdout) y mapea los exit codes a HTTP.
- **Traduce** el `DesignResult` a `mcp_instructions` para Civil 3D y las adjunta a
  la respuesta (el binario emite geometria cruda; spec S-23). Ver `mcp_translate.py`.
- Tier `open`: **el gate de licencia se retiro** (MAM-236, 2026-08-25). `validate_license()`
  corria antes de abrir el socket; contradecia la Apache-2.0 del repo y, peor, dejaba al
  harness **sin poder descubrir el motor** en una PC sin `MAMUT_LICENSE_TOKEN` -- no fallaba
  al llamarlo, no existia. `license_guard.py` sigue aca, sin invocarse.

## Features

| name                  | endpoint                      | method | project_type |
|-----------------------|-------------------------------|--------|--------------|
| sewer_design          | `/api/hydro_engine/design`    | POST   | sewer        |
| water_supply_design   | `/api/hydro_engine/design`    | POST   | water_supply |
| conveyance_design     | `/api/hydro_engine/design`    | POST   | conveyance   |
| distribution_design   | `/api/hydro_engine/design`    | POST   | distribution |
| pump_station_design   | `/api/hydro_engine/design`    | POST   | pump_station |
| intake_design         | `/api/hydro_engine/design`    | POST   | intake       |
| solvers               | `/api/hydro_engine/solvers`   | GET    | —            |

Las 6 design comparten el endpoint `design`; `project_type` discrimina.

## Comando exacto del subprocess

```sh
# Sin flags -> lee stdin / escribe stdout:
hydro-cli.exe < DesignRequest.json > Solution.json
# equivalente:
cat DesignRequest.json | hydro-cli.exe
```

Resolucion del binario (orden):
1. env `HYDRO_CLI_BIN` (ruta explicita al ejecutable)
2. `../target/release/hydro-cli.exe`
3. `../target/debug/hydro-cli.exe`

Si no compilaste el binario: `cargo build --release` en la raiz del repo.

### Exit codes del binario -> HTTP

| exit | significado                  | HTTP |
|------|------------------------------|------|
| 0    | OK                           | 200  |
| 1    | error de validacion          | 422  |
| 2    | sin solucion factible        | 422  |
| 3    | falla de norma               | 422  |
| 4    | error interno / IO           | 500  |

(Binario ausente -> 503; timeout -> 504.)

## Licencia (tier proprietary)

El motor NO arranca sin licencia. El contrato es **EdDSA (Ed25519)**, alineado al
backend autoritativo (`motor-optimizacion-hidraulico/licensing-backend`). Claims:
`iss=mamut-licensing-v1` + `sub/iat/exp/jti/source/transaction_id/max_machines` +
`product=motor-hidraulico`. El binding por maquina lo hace la **activacion**
(`max_machines` + tabla del backend), NO un claim `machine_id` en el JWT.

Variables de entorno:

- `MAMUT_LICENSE_TOKEN` — token JWT **EdDSA** emitido por el backend de licencias.
- `MAMUT_LICENSE_PUBKEY` — (opcional) ruta a un `.pem` con la clave publica.
  Si falta, usa la `PUBLIC_KEY` real embebida (la de `keys/beta/public.pem`).
- `MAMUT_LICENSE_DEV=1` — modo desarrollo: arranca SIN token. **No usar en produccion.**

Emitir un token (beta), desde el repo del backend:

```sh
cd ../../motor-optimizacion-hidraulico/licensing-backend
npm run issue:beta -- <email> --key-file ../keys/beta/private.pem [--days 365]
```

Para conocer el `machine_id` de esta maquina (diagnostico / activacion):

```sh
python license_guard.py        # imprime el payload e incluye machine_id
```

## Arranque

```sh
pip install -r requirements.txt        # pyjwt[crypto] (EdDSA); el core es stdlib
# desarrollo:
MAMUT_LICENSE_DEV=1 python server.py
# produccion:
export MAMUT_LICENSE_TOKEN="<jwt>"
python server.py
```

## Prueba rapida (smoke)

```sh
# en una terminal:
MAMUT_LICENSE_DEV=1 python server.py
# leer el puerto del anuncio:
PORT=$(python -c "import json,os;print(json.load(open(os.path.expandvars('%LOCALAPPDATA%/MAMUT/plugins/hydro_engine.json')))['port'])")
curl http://127.0.0.1:$PORT/health
curl http://127.0.0.1:$PORT/manifest
curl -X POST http://127.0.0.1:$PORT/api/hydro_engine/design \
     -H 'Content-Type: application/json' --data @DesignRequest.json
```

## Estado / siguiente paso

El contrato MAMUT del adapter (anuncio + `/health` + `/manifest` + subprocess +
license gate EdDSA + traduccion `DesignResult` -> `mcp_instructions`) esta
**completo y verificado** (18 tests + end-to-end con el binario real). El contrato
de las instrucciones MCP esta verificado contra el plugin C# de Civil3D
(`PipeNetworkHandlers.cs`, body camelCase).

Pendiente operativo: emitir el `MAMUT_LICENSE_TOKEN` por maquina y, si se quiere el
cap real por maquina, cablear el flujo de activacion cliente->backend. Limitacion
conocida: los `waypoints` del tramo no se dibujan (el `add-pipe` del plugin solo
hace recta start->end); para la polilinea exacta usar `/api/drawing/polyline`.
