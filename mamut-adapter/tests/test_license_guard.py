"""Tests del gate de licencia EdDSA (license_guard.py).

Usa un keypair Ed25519 EFIMERO generado en el test (no la clave real de prod)
para firmar tokens y verificar la logica del contrato sin exponer secretos.
"""

from __future__ import annotations

import os
import sys
import time

import jwt
from cryptography.hazmat.primitives import serialization
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from license_guard import (  # noqa: E402
    EXPECTED_PRODUCT,
    LICENSE_ALGORITHM,
    LICENSE_CONTRACT_VERSION,
    LicenseGuard,
    get_machine_id,
)

_NOW = int(time.time())  # reloj real: PyJWT valida exp contra wall-clock


def _keypair() -> tuple[str, str]:
    priv = Ed25519PrivateKey.generate()
    priv_pem = priv.private_bytes(
        serialization.Encoding.PEM,
        serialization.PrivateFormat.PKCS8,
        serialization.NoEncryption(),
    ).decode()
    pub_pem = priv.public_key().public_bytes(
        serialization.Encoding.PEM,
        serialization.PublicFormat.SubjectPublicKeyInfo,
    ).decode()
    return priv_pem, pub_pem


def _claims(**overrides) -> dict:
    base = {
        "iss": LICENSE_CONTRACT_VERSION,
        "sub": "pilot@example.com",
        "iat": _NOW,
        "exp": _NOW + 86400,
        "jti": "lic_test0001",
        "source": "polar",
        "transaction_id": "BETA-TEST-001",
        "max_machines": 1,
        "product": EXPECTED_PRODUCT,
    }
    base.update(overrides)
    return base


def _sign(priv_pem: str, claims: dict) -> str:
    return jwt.encode(claims, priv_pem, algorithm=LICENSE_ALGORITHM)


def test_token_valido_pasa():
    priv, pub = _keypair()
    token = _sign(priv, _claims())
    payload = LicenseGuard(public_key=pub).check_license(token)
    assert payload["product"] == EXPECTED_PRODUCT
    assert payload["sub"] == "pilot@example.com"


def test_token_expirado_rechaza():
    priv, pub = _keypair()
    token = _sign(priv, _claims(iat=_NOW - 200000, exp=_NOW - 100000))
    _assert_rejected(pub, token, "expirada")


def test_otro_producto_rechaza():
    priv, pub = _keypair()
    token = _sign(priv, _claims(product="otro-motor"))
    _assert_rejected(pub, token, "otro producto")


def test_issuer_equivocado_rechaza():
    priv, pub = _keypair()
    token = _sign(priv, _claims(iss="otro-emisor"))
    _assert_rejected(pub, token)


def test_claim_faltante_rechaza():
    priv, pub = _keypair()
    incompleto = _claims()
    del incompleto["max_machines"]
    token = _sign(priv, incompleto)
    _assert_rejected(pub, token)


def test_firma_de_otra_clave_rechaza():
    priv_a, _ = _keypair()
    _, pub_b = _keypair()
    token = _sign(priv_a, _claims())  # firmado con A, validado con B
    _assert_rejected(pub_b, token)


def test_machine_id_defensivo_otra_maquina_rechaza():
    priv, pub = _keypair()
    token = _sign(priv, _claims(machine_id="ffffffffffffffff"))
    _assert_rejected(pub, token, "otra maquina")


def test_machine_id_defensivo_misma_maquina_pasa():
    priv, pub = _keypair()
    token = _sign(priv, _claims(machine_id=get_machine_id()))
    payload = LicenseGuard(public_key=pub).check_license(token)
    assert payload["machine_id"] == get_machine_id()


def test_algoritmo_es_eddsa_no_rs256():
    # Defensa explicita: el contrato es EdDSA, no RS256.
    assert LICENSE_ALGORITHM == "EdDSA"


def _assert_rejected(pub: str, token: str, expect_substr: str | None = None) -> None:
    try:
        LicenseGuard(public_key=pub).check_license(token)
    except PermissionError as err:
        if expect_substr:
            assert expect_substr.lower() in str(err).lower(), f"mensaje inesperado: {err}"
        return
    raise AssertionError("se esperaba PermissionError y no se lanzo")


if __name__ == "__main__":
    import traceback

    fns = [v for k, v in sorted(globals().items())
           if k.startswith("test_") and callable(v)]
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
