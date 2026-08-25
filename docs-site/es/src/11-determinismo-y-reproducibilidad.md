# Determinismo y reproducibilidad

El motor es determinista cuando el seed está fijo:

```
seed_resuelto = bandera --seed (si fue dada) > request.seed (si fue dado) > aleatorio
audit_hash    = hex_minuscula(SHA-256(
                  serde_json::to_string(&request)
                  || "|"
                  || seed_en_decimal
                  || "|"
                  || version_motor
                ))
```

Propiedades:

- Mismo JSON `request` + mismo `seed` + misma versión del motor → mismas
  `result.solutions` (byte por byte bajo round-trip JSON).
- El hash es sensible a cambios de seed: cambiar el seed cambia el hash.
- El hash es sensible a la versión del motor (`CARGO_PKG_VERSION`); actualizar
  el binario cambia el hash intencionalmente aun con entradas idénticas.

Usá el hash para:

- Etiquetar documentos de resultado almacenados (rastro de auditoría).
- Detectar deriva de input en tests de regresión (entrada cambió → nuevo hash).
- Fijar una "corrida buena conocida" para la firma de ingeniería.

El trabajo paralelo del motor corre sobre `rayon`. El seed se propaga al RNG
del GA, y la capa de grafo determinista (introducida en la Fase 7.5) garantiza
orden topológico idéntico byte por byte entre corridas.

---

