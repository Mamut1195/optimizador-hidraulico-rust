//! RED test — WU-1: CliError exit_code mapping per REQ-003.
//!
//! Asserts that every CliError variant returns the exact exit code specified in
//! the REQ-003 table. This file is committed BEFORE lib.rs is written; it must
//! fail to compile because `CliError` does not yet exist in hydro_cli.

use hydro_cli::CliError;
use hydro_optimizer::OptimizationError;
use hydro_types::error::HydroTypesError;

/// Verify exit_code() for all four CliError variants per REQ-003 table.
#[test]
fn test_cli_error_exit_codes() {
    // --- exit 1: ValidationError ---

    // From<serde_json::Error> wraps into ValidationError → exit 1
    let json_err: serde_json::Error =
        serde_json::from_str::<serde_json::Value>("{bad}").unwrap_err();
    let cli_err = CliError::from(json_err);
    assert_eq!(
        cli_err.exit_code(),
        1,
        "serde_json::Error must map to exit 1"
    );

    // From<HydroTypesError> → ValidationError → exit 1
    let hte = HydroTypesError::MissingRequired { field: "outlet" };
    let cli_err = CliError::from(hte);
    assert_eq!(cli_err.exit_code(), 1, "HydroTypesError must map to exit 1");

    // ValidationError constructed directly → exit 1
    let direct = CliError::ValidationError(HydroTypesError::CrossFieldViolation {
        message: "test".into(),
    });
    assert_eq!(direct.exit_code(), 1, "ValidationError must return exit 1");

    // --- exit 2: NoFeasibleSolution ---

    // From<OptimizationError::AllInfeasible> → NoFeasibleSolution → exit 2
    let cli_err = CliError::from(OptimizationError::AllInfeasible);
    assert_eq!(
        cli_err.exit_code(),
        2,
        "OptimizationError::AllInfeasible must map to exit 2"
    );

    // NoFeasibleSolution constructed directly → exit 2
    assert_eq!(
        CliError::NoFeasibleSolution.exit_code(),
        2,
        "NoFeasibleSolution must return exit 2"
    );

    // --- exit 3: NormComplianceFailed ---

    // From<OptimizationError::NormValidationFailure> → NormComplianceFailed → exit 3
    let cli_err = CliError::from(OptimizationError::NormValidationFailure(
        "too many bends".into(),
    ));
    assert_eq!(
        cli_err.exit_code(),
        3,
        "OptimizationError::NormValidationFailure must map to exit 3"
    );

    // NormComplianceFailed constructed directly → exit 3
    assert_eq!(
        CliError::NormComplianceFailed("msg".into()).exit_code(),
        3,
        "NormComplianceFailed must return exit 3"
    );

    // --- exit 4: InternalError ---

    // From<OptimizationError::InvalidConfig> → InternalError → exit 4
    let cli_err = CliError::from(OptimizationError::InvalidConfig("bad pop".into()));
    assert_eq!(
        cli_err.exit_code(),
        4,
        "OptimizationError::InvalidConfig must map to exit 4"
    );

    // From<OptimizationError::TimeBudgetExceeded> → InternalError → exit 4
    let cli_err = CliError::from(OptimizationError::TimeBudgetExceeded(30.0));
    assert_eq!(
        cli_err.exit_code(),
        4,
        "OptimizationError::TimeBudgetExceeded must map to exit 4"
    );

    // InternalError constructed directly → exit 4
    assert_eq!(
        CliError::InternalError("x".into()).exit_code(),
        4,
        "InternalError must return exit 4"
    );
}
