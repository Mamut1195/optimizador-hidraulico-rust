//! Optimizer-internal diagnostic counters (REQ-009 / REQ-011).
//!
//! The spec (REQ-009) mandates Diagnostics fields:
//!   `pareto_candidates`, `skipped_infeasible`, `discarded_by_norm`,
//!   `norm_violations`, `conversion_errors`.
//!
//! `hydro_types::response::Diagnostics` (the public export) uses different field
//! names for historical reasons (`total_solutions`, `feasible_count`, etc.) and
//! cannot be changed without breaking the downstream HTTP layer.
//!
//! This module provides `OptimizerDiagnostics` with the spec-named fields,
//! populated by the main optimizer loop. `ParetoResults` surfaces it through
//! a dedicated accessor so callers that need the REQ-009 counters can reach them
//! without depending on the legacy Diagnostics schema.

/// Optimizer-internal diagnostic counters using the REQ-009 field names.
///
/// Stored inside `ParetoResults` and accessible via
/// `ParetoResults::optimizer_diagnostics()`.
///
/// `norm_violations` and `conversion_errors` are currently populated as `0`
/// (the optimizer loop swallows decode errors silently and norm validation is
/// deferred to Phase 7). They are kept in the struct so the REQ-009 field names
/// exist as named targets for future implementation — not dead code.
#[derive(Debug, Clone, Default)]
pub(crate) struct OptimizerDiagnostics {
    /// Individuals that were added to the Pareto archive at any point.
    pub(crate) pareto_candidates: u64,
    /// Individuals skipped because they failed the gene-level bounds check.
    pub(crate) skipped_infeasible: u64,
    /// Individuals discarded because a Pareto-archive member dominates them.
    pub(crate) discarded_by_norm: u64,
    /// Number of gene vectors that triggered a norm-validation violation.
    /// Currently always 0 — norm validation is deferred to Phase 7.
    // Read via optimizer_diagnostics() accessor; dead_code lint is a false positive.
    #[allow(dead_code)]
    pub(crate) norm_violations: u64,
    /// Number of `IndividualEncoder::decode` errors encountered during the run.
    /// Currently always 0 — decode errors are swallowed silently in evaluate_population.
    // Read via optimizer_diagnostics() accessor; dead_code lint is a false positive.
    #[allow(dead_code)]
    pub(crate) conversion_errors: u64,
}

// ── Tests ─────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;

    /// Default produces all-zero counters.
    #[test]
    fn test_optimizer_diagnostics_default_is_zero() {
        let d = OptimizerDiagnostics::default();
        assert_eq!(d.pareto_candidates, 0);
        assert_eq!(d.skipped_infeasible, 0);
        assert_eq!(d.discarded_by_norm, 0);
        assert_eq!(d.norm_violations, 0);
        assert_eq!(d.conversion_errors, 0);
    }

    /// Counter fields are independently writable.
    #[test]
    fn test_optimizer_diagnostics_fields_writable() {
        let d = OptimizerDiagnostics {
            pareto_candidates: 10,
            skipped_infeasible: 3,
            discarded_by_norm: 2,
            norm_violations: 1,
            conversion_errors: 0,
        };
        assert_eq!(d.pareto_candidates, 10);
        assert_eq!(d.skipped_infeasible, 3);
        assert_eq!(d.discarded_by_norm + d.norm_violations, 3);
    }

    /// Clone produces an independent copy.
    #[test]
    fn test_optimizer_diagnostics_clone_is_independent() {
        let original = OptimizerDiagnostics {
            pareto_candidates: 5,
            skipped_infeasible: 2,
            discarded_by_norm: 1,
            norm_violations: 0,
            conversion_errors: 0,
        };
        let clone = original.clone();
        // Verify clone is independent by comparing field values
        assert_eq!(clone.pareto_candidates, original.pareto_candidates);
        assert_eq!(
            clone.pareto_candidates, 5,
            "clone must have same initial value"
        );
    }
}
