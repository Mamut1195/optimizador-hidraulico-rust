# Appendix A — File layout for reference

```
hydro-cli/
├── src/
│   ├── main.rs           # binary entrypoint (clap shell)
│   └── lib.rs            # run(), validate_request(), CliError, helpers
└── tests/                # 50 integration tests (one per project type + features)

hydro-types/               # The contract: DesignRequest, DesignResult, enums, constraints
├── src/
│   ├── request.rs        # DesignRequest, validate()
│   ├── response.rs       # DesignResult, Solution, Diagnostics, PipeNetwork
│   ├── enums.rs          # ProjectType, NodeType, PipeMaterial, FlowType, Severity
│   ├── constraints.rs    # DesignConstraints with per-norm defaults
│   ├── network.rs        # NodeId, PipeId, NetworkNode, NetworkPipe, PipeNetwork
│   └── error.rs          # HydroTypesError

hydro-solvers/             # 6 domain solvers + SolverGraph helper
hydro-optimizer/           # NSGA-III genetic optimizer
hydro-terrain/             # Terrain interpolation
hydro-hydraulics/          # Hydraulic primitives
hydro-norms/               # Normative rules (CONAGUA_MX, etc.)
```

