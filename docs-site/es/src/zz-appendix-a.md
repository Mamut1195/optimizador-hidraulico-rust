# Apéndice A — Layout de archivos para referencia

```
hydro-cli/
├── src/
│   ├── main.rs           # entrypoint del binario (shell clap)
│   └── lib.rs            # run(), validate_request(), CliError, helpers
└── tests/                # 50 tests de integración (uno por tipo de proyecto + features)

hydro-types/               # El contrato: DesignRequest, DesignResult, enums, constraints
├── src/
│   ├── request.rs        # DesignRequest, validate()
│   ├── response.rs       # DesignResult, Solution, Diagnostics, PipeNetwork
│   ├── enums.rs          # ProjectType, NodeType, PipeMaterial, FlowType, Severity
│   ├── constraints.rs    # DesignConstraints con defaults por norma
│   ├── network.rs        # NodeId, PipeId, NetworkNode, NetworkPipe, PipeNetwork
│   └── error.rs          # HydroTypesError

hydro-solvers/             # 6 solvers de dominio + helper SolverGraph
hydro-optimizer/           # Optimizador genético NSGA-III
hydro-terrain/             # Interpolación de terreno
hydro-hydraulics/          # Primitivas hidráulicas
hydro-norms/               # Reglas normativas (CONAGUA_MX, etc.)
```

