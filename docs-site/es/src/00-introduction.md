# Manual del motor — `optimizador-hidraulico-rust`

**Versión de referencia**: frontera v1 del motor (main en `203c5c1`)
**Binario**: `hydro-cli`
**Contrato**: JSON de entrada / JSON de salida
**Audiencia**: integradores, consumidores aguas abajo, mantenedores futuros, agentes de IA

---

Este manual documenta el contrato completo de v1 del binario `hydro-cli`
— la única superficie estable del motor de optimización hidráulica. Usá la
barra lateral de la izquierda para navegar; la búsqueda de texto completo
está en la barra superior; el conmutador de tema está arriba a la izquierda.

Empezá en [§1 — Qué es este motor](./01-que-es-este-motor.md) si nunca
viste el motor antes. Saltá a [§6 — Contrato de entrada](./06-contrato-de-entrada-designrequest.md)
o [§7 — Contrato de salida](./07-contrato-de-salida-designresult.md) si
estás armando una integración. Saltá a [§12 — Ejemplos resueltos](./12-ejemplos-resueltos-por-tipo-de-proyecto.md)
si querés un request JSON funcional para copiar y pegar.
