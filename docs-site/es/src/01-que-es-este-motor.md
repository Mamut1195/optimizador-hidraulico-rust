# Qué es este motor

Un motor autónomo de optimización hidráulica que:

- Toma un único documento JSON `DesignRequest` que describe el terreno, la
  demanda, las restricciones y los parámetros del optimizador.
- Ejecuta un optimizador genético multi-objetivo NSGA-III (5 objetivos:
  costo, excavación, bombeo, interferencia, resiliencia) sobre el solver
  de dominio correspondiente (alcantarillado, agua potable, conducción,
  distribución, estación de bombeo o captación).
- Devuelve un documento JSON `DesignResult` con redes Pareto-óptimas
  jerarquizadas, sus puntajes, diagnósticos y un hash de auditoría.

El motor **es un binario, no una biblioteca**. La CLI es el único contrato
estable. Cualquier consumidor aguas abajo — servidor REST, GUI, plugin de
Civil 3D, script de automatización — habla con él mediante stdin/stdout o
archivos.

Fuera del alcance de v1: compilación WASM, FFI con C-ABI, servidor HTTP
incrustado, interoperabilidad con EPANET/SWMM, GUI. Cada uno de estos sería
una fase futura que envuelva este binario.

---

