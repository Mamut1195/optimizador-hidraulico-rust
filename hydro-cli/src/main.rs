//! hydro-cli — Thin JSON in/out shell for the hydraulic optimization engine.
//!
//! Reads a DesignRequest JSON from `--input <path>` or stdin,
//! runs the optimizer, and emits a Solution JSON on stdout.
//!
//! Exit codes:
//!   0 — success
//!   1 — validation error (bad DesignRequest)
//!   2 — no feasible solution found
//!   3 — no norm-compliant solution (strict_norm_compliance gate)
//!   4 — internal engine error

fn main() {
    // Placeholder — full implementation lives in PR-7 (hydro-cli).
    eprintln!("hydro-cli: engine not yet implemented (scaffold only)");
    std::process::exit(4);
}

#[cfg(test)]
mod tests {
    #[test]
    fn binary_skeleton_compiles() {
        // The binary must at minimum compile and link.
        // The test passing confirms compilation succeeded.
    }
}
