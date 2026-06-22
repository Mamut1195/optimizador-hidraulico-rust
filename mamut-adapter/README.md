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
  (`DesignRequest` por stdin -> `Solution` por stdout) y mapea los exit codes a HTTP.
- Tier `proprietary`: `validate_license()` corre **ANTES** de abrir el socket.

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

El motor NO arranca sin licencia. Variables de entorno:

- `MAMUT_LICENSE_TOKEN` — token JWT (RS256) emitido por el backend, ligado al
  `machine_id` de esta maquina.
- `MAMUT_LICENSE_PUBKEY` — (opcional) ruta a un `.pem` con la clave publica.
  Si falta, usa la `PUBLIC_KEY` embebida (placeholder de dev).
- `MAMUT_LICENSE_DEV=1` — modo desarrollo: arranca SIN token. **No usar en produccion.**

Para conocer el `machine_id` de esta maquina:

```sh
python license_guard.py        # imprime el payload e incluye machine_id
```

> El `PUBLIC_KEY` embebido es un placeholder. Sin la clave privada del backend
> no se puede emitir un token RS256 valido. En desarrollo local usar
> `MAMUT_LICENSE_DEV=1`.

## Arranque

```sh
pip install -r requirements.txt        # solo PyJWT (el core es stdlib)
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
license gate) esta completo y verificado. La traduccion del `Solution` Rust a
**instrucciones MCP para Civil 3D** (feature `execute` de la referencia Python)
queda como siguiente paso: hoy el adapter devuelve el `DesignResult` crudo del
motor.
