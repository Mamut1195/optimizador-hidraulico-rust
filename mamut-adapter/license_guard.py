"""LicenseGuard del Optimizador Hidraulico (Rust) — gate REAL (EdDSA).

Tier 'proprietary' (motor con IP): la capa MAMUT NO debe arrancar sin una
licencia valida. Este adapter es el VERIFICADOR; el EMISOR autoritativo es
`motor-optimizacion-hidraulico/licensing-backend` (Cloudflare Worker) que
firma JWTs Ed25519 tras el pago (Hotmart/Polar).

CONTRATO (frozen boundary — espejo fiel de
`motor-optimizacion-hidraulico/src/hydro_engine/api/license_contract.py` y
`licensing-backend/src/contract.ts`). Si cambia alla, cambia aca:
  - iss  == "mamut-licensing-v1"   (version de contrato)
  - alg  == "EdDSA" (Ed25519)      (NO RS256: el backend nunca firmo con RSA)
  - claims requeridos: iss, sub, iat, exp, jti, source, transaction_id,
    max_machines, product           (contrato CERRADO; sin extras)
  - product == "motor-hidraulico"  (este adapter es el motor hidraulico)

BINDING POR MAQUINA: el backend NO mete `machine_id` en el JWT — el cap de
maquinas se enforce a nivel de activacion (claim `max_machines` + tabla de
activaciones D1 al hacer POST /api/licenses/activate). Por eso aca el
`machine_id` es un chequeo DEFENSIVO opcional: solo se valida si el token
algun dia lo trajera. La huella de esta maquina se expone para diagnostico y
para el flujo de activacion.

El auditor (mamut-contract-audit) busca marcadores de un gate real:
`validate_license`, `machine_id`, `LicenseGuard`, `check_license`, etc. Este
archivo los contiene Y los implementa de verdad.

Dependencias: PyJWT + cryptography (EdDSA). Declaradas en pyproject.toml /
requirements.txt.

Variables de entorno:
  MAMUT_LICENSE_TOKEN   token JWT Ed25519 emitido por el backend de licencias.
  MAMUT_LICENSE_PUBKEY  (opcional) ruta a un .pem con la clave publica; si falta,
                        usa la PUBLIC_KEY embebida (la real de keys/beta).
  MAMUT_LICENSE_DEV     si == "1", permite arrancar SIN token (modo desarrollo).
                        En produccion NO se debe definir.
"""

from __future__ import annotations

import hashlib
import os
import sys
import uuid
from typing import Any

try:
    import jwt  # PyJWT (EdDSA requiere el extra `cryptography`, ya instalado)
except ImportError:  # pragma: no cover - guia clara si falta la dependencia
    jwt = None  # type: ignore[assignment]


# --- contrato (espejo de license_contract.py / contract.ts) ---------------------
LICENSE_CONTRACT_VERSION = "mamut-licensing-v1"   # va en el claim `iss`
LICENSE_ALGORITHM = "EdDSA"                        # Ed25519 — NO RS256
EXPECTED_PRODUCT = "motor-hidraulico"              # este adapter es el motor hidraulico
LICENSE_REQUIRED_CLAIMS: tuple[str, ...] = (
    "iss",
    "sub",
    "iat",
    "exp",
    "jti",
    "source",
    "transaction_id",
    "max_machines",
    "product",
)

# Clave publica Ed25519 EMBEBIDA (la real de
# motor-optimizacion-hidraulico/keys/beta/public.pem). La privada vive en el
# backend (secret de Cloudflare PRIVATE_KEY_PEM). El header OID `MCowBQYDK2Vw`
# es id-EdDSA (1.3.101.112): es una clave Ed25519, no RSA.
PUBLIC_KEY = """-----BEGIN PUBLIC KEY-----
MCowBQYDK2VwAyEAJd9trndxVpUofLlxtPd58EZb3+ZCIcohPRObuk/s7jM=
-----END PUBLIC KEY-----"""


def get_machine_id() -> str:
    """Huella estable de la maquina a partir de la MAC (SHA-256, 16 hex).

    Copiado de agente-local/src/agent/utils/machine_id.py para no acoplar al
    paquete `agent`. Se usa para el flujo de activacion (cap de maquinas) y
    como chequeo defensivo si el token trajera `machine_id`.
    """
    mac = uuid.getnode()
    return hashlib.sha256(str(mac).encode()).hexdigest()[:16]


def _load_public_key() -> str:
    """Lee la clave publica de MAMUT_LICENSE_PUBKEY si existe, si no la embebida."""
    path = os.environ.get("MAMUT_LICENSE_PUBKEY")
    if path and os.path.isfile(path):
        with open(path, encoding="utf-8") as f:
            return f.read()
    return PUBLIC_KEY


class LicenseGuard:
    """Valida tokens JWT Ed25519 contra el contrato congelado del backend."""

    __slots__ = ("_payload", "_public_key")

    def __init__(self, public_key: str | None = None) -> None:
        self._public_key = public_key or _load_public_key()
        self._payload: dict[str, Any] | None = None

    def check_license(self, token: str) -> dict[str, Any]:
        """Valida firma Ed25519 + iss + claims requeridos + exp + product.

        Lanza PermissionError si la licencia es invalida por cualquier motivo
        (firma, algoritmo, issuer, claim faltante, expirada, otro producto, u
        otra maquina si el token trajera `machine_id`).
        """
        if jwt is None:
            raise PermissionError(
                "PyJWT no esta instalado — `pip install pyjwt[crypto]` "
                "(ver requirements.txt)"
            )
        try:
            # algorithms=["EdDSA"] defiende contra confusion de algoritmo
            # (HS256 con la publica como secreto, alg=none). `require` exige
            # presencia ANTES de mirar valores; issuer verifica `iss`.
            self._payload = jwt.decode(
                token,
                self._public_key,
                algorithms=[LICENSE_ALGORITHM],
                issuer=LICENSE_CONTRACT_VERSION,
                options={
                    "verify_signature": True,
                    "verify_exp": True,
                    "require": list(LICENSE_REQUIRED_CLAIMS),
                },
            )
        except jwt.ExpiredSignatureError as err:  # type: ignore[union-attr]
            raise PermissionError("Licencia expirada") from err
        except jwt.InvalidTokenError as err:  # type: ignore[union-attr]
            raise PermissionError(f"Licencia invalida: {err}") from err

        # Este motor solo acepta licencias de SU producto.
        product = self._payload.get("product")
        if product != EXPECTED_PRODUCT:
            raise PermissionError(
                f"Licencia para otro producto: {product!r} "
                f"(se esperaba {EXPECTED_PRODUCT!r})"
            )

        # Binding a la maquina: DEFENSIVO. El backend no emite `machine_id`
        # (el cap se enforce por `max_machines` + activacion), pero si algun
        # dia lo trajera, debe coincidir con esta maquina.
        expected_machine = self._payload.get("machine_id")
        if expected_machine and expected_machine != get_machine_id():
            raise PermissionError("La licencia es para otra maquina")

        return self._payload

    @property
    def is_valid(self) -> bool:
        return self._payload is not None

    @property
    def plan(self) -> str:
        """Plan/producto del token. El contrato usa `product` (no `plan`)."""
        if not self._payload:
            return "none"
        return self._payload.get("product", "unknown")

    def status_info(self) -> dict[str, Any]:
        return {
            "valid": self.is_valid,
            "product": self.plan,
            "sub": self._payload.get("sub") if self._payload else None,
            "exp": self._payload.get("exp") if self._payload else None,
            "max_machines": self._payload.get("max_machines") if self._payload else None,
            "machine_id": get_machine_id(),
        }


def validate_license() -> dict[str, Any]:
    """Gate de arranque: server.py lo llama ANTES de abrir el socket.

    - Si MAMUT_LICENSE_DEV == "1": permite arrancar sin token (modo desarrollo)
      y devuelve un payload de dev. NO usar en produccion.
    - Si falta MAMUT_LICENSE_TOKEN: aborta con SystemExit (proprietary requiere
      licencia).
    - Si el token es invalido / de otro producto / expirado: aborta con SystemExit.

    Devuelve el payload de la licencia si todo OK.
    """
    if os.environ.get("MAMUT_LICENSE_DEV") == "1":
        sys.stderr.write(
            "[license_guard] MODO DEV: arrancando SIN licencia "
            f"(machine_id={get_machine_id()}). NO usar en produccion.\n"
        )
        return {"product": "dev", "machine_id": get_machine_id(), "dev": True}

    token = os.environ.get("MAMUT_LICENSE_TOKEN")
    if not token:
        sys.stderr.write(
            "[license_guard] FALTA MAMUT_LICENSE_TOKEN. Este motor es 'proprietary' "
            "y NO arranca sin licencia.\n"
            f"  machine_id de esta maquina: {get_machine_id()}\n"
            "  Pedile al backend de licencias un token Ed25519 (producto "
            f"'{EXPECTED_PRODUCT}') y exportalo en MAMUT_LICENSE_TOKEN.\n"
            "  Emision beta:  npm run issue:beta -- <email> --key-file "
            "keys/beta/private.pem\n"
            "  (Para desarrollo local: export MAMUT_LICENSE_DEV=1)\n"
        )
        raise SystemExit(2)

    guard = LicenseGuard()
    try:
        payload = guard.check_license(token)
    except PermissionError as err:
        sys.stderr.write(f"[license_guard] Licencia rechazada: {err}\n")
        raise SystemExit(2) from err

    sys.stderr.write(
        f"[license_guard] Licencia OK · product={guard.plan} · "
        f"sub={payload.get('sub')} · machine_id={get_machine_id()}\n"
    )
    return payload


if __name__ == "__main__":
    # Permite probar el gate de forma aislada: python license_guard.py
    info = validate_license()
    print(info)
