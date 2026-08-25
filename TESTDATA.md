# TESTDATA — optimizador-hidraulico-rust

> Pointer al sistema central de datos de prueba. Detalle completo en:
> `../../ARCHIVOS-DE-PREUBA-PARA-APPS/TEST-DATA-INDEX.md`

> ⚠️ **ESTO ES UNA GUÍA, NO LA FUENTE DE VERDAD.** Antes de usar cualquier dataset,
> abrí e inspeccioná los archivos reales y confirmá que sirven para el flujo que vas a
> probar. El mapeo es una inferencia inicial; los datos mandan. Si un dataset no sirve o
> falta uno, registralo y actualizá `../../ARCHIVOS-DE-PREUBA-PARA-APPS/APP-TEST-MAPPING.md`.

## Datasets candidatos para probar esta app

Port Rust del motor hidráulico (solo cómputo: lee un `DesignRequest` JSON sobre terreno,
hidráulica y normas; emite `Solution` JSON).

> ⚠️ La app consume **JSON `DesignRequest`**, no los archivos crudos. Probablemente necesites
> **derivar** un `DesignRequest` desde estos datasets (topografía + restricciones) antes de
> usarlos. Verificá el formato que espera la app.

| Dataset | Ruta | Para qué flujo |
|---------|------|----------------|
| Bombeo | `../../ARCHIVOS-DE-PREUBA-PARA-APPS/_Inf_Dev/Bombeo` | Caso real de bombeo para construir un DesignRequest |
| GIS | `../../ARCHIVOS-DE-PREUBA-PARA-APPS/_Inf_Dev/GIS` | Terreno para el input de optimización |

Estado del mapeo: por confirmar
