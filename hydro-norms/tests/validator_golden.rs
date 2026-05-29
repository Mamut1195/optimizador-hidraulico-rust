//! Integration tests for NormValidator — T-3.3 RED → GREEN.
//!
//! Loads `norms_violations_golden.json` and asserts EXACT parity between the
//! Rust NormValidator output and the Python oracle's expected counts.

use hydro_norms::{NormRegistry, NormValidator};
use hydro_types::{
    enums::{NodeType, PipeMaterial, ProjectType},
    network::{NetworkNode, NetworkPipe, NodeId, PipeId, PipeNetwork},
};
use oracle::norms::load_norms_violations_golden;

/// Convert SCREAMING_SNAKE_CASE node_type string to NodeType enum.
fn parse_node_type(s: &str) -> NodeType {
    match s {
        "MANHOLE" => NodeType::Manhole,
        "JUNCTION" => NodeType::Junction,
        "OUTLET" => NodeType::Outlet,
        "INLET" => NodeType::Inlet,
        "PUMP" => NodeType::Pump,
        "VALVE" => NodeType::Valve,
        "HYDRANT" => NodeType::Hydrant,
        "TANK" => NodeType::Tank,
        "RESERVOIR" => NodeType::Reservoir,
        "SERVICE" => NodeType::Service,
        other => panic!("unknown node_type: '{other}'"),
    }
}

/// Convert SCREAMING_SNAKE_CASE project_type string to ProjectType enum.
fn parse_project_type(s: &str) -> ProjectType {
    match s {
        "SEWER" => ProjectType::Sewer,
        "WATER_SUPPLY" => ProjectType::WaterSupply,
        "CONVEYANCE" => ProjectType::Conveyance,
        "DISTRIBUTION" => ProjectType::Distribution,
        "PUMP_STATION" => ProjectType::PumpStation,
        "INTAKE" => ProjectType::Intake,
        other => panic!("unknown project_type: '{other}'"),
    }
}

/// Build a `PipeNetwork` from the oracle fixture case.
fn build_network(case: &oracle::norms::NormsViolationsCase) -> PipeNetwork {
    let net = &case.network;

    let nodes: Vec<NetworkNode> = net
        .nodes
        .iter()
        .map(|n| {
            let mut node = NetworkNode::new(
                NodeId::new(&n.id),
                n.x,
                n.y,
                n.z,
                parse_node_type(&n.node_type),
            );
            node.metadata = n.metadata.clone();
            node
        })
        .collect();

    let pipes: Vec<NetworkPipe> = net
        .pipes
        .iter()
        .map(|p| {
            // Reconstruct start/end inverts from slope and a base elevation.
            // z=10.0 for all nodes, base cover=1.5m → start_invert=8.5
            let start_invert = 8.5_f64;
            let end_invert = start_invert - p.slope * p.length;
            NetworkPipe {
                id: PipeId::new(&p.id),
                start_node_id: NodeId::new(&p.start_node_id),
                end_node_id: NodeId::new(&p.end_node_id),
                length: p.length,
                effective_length: p.length,
                diameter: p.diameter,
                material: PipeMaterial::Pvc,
                roughness: p.roughness,
                start_invert,
                end_invert,
                slope: p.slope,
                design_flow: p.design_flow,
                waypoints: vec![],
                metadata: p.metadata.clone(),
            }
        })
        .collect();

    PipeNetwork::new(net.name.clone(), nodes, pipes)
}

// T-3.3 acceptance criterion 2+3: hard_violation_count and compliant match oracle
#[test]
fn violation_counts_match_oracle_for_all_fixture_cases() {
    let fixture = load_norms_violations_golden();
    assert!(
        !fixture.cases.is_empty(),
        "violations fixture must have at least one case"
    );

    for case in &fixture.cases {
        let profile = NormRegistry::get(&case.norm_code)
            .unwrap_or_else(|e| panic!("failed to load norm {}: {e}", case.norm_code));
        let validator = NormValidator::new(profile);
        let network = build_network(case);
        let pt = parse_project_type(&case.project_type);
        let result = validator.validate(&network, pt);

        assert_eq!(
            result.hard_violation_count,
            case.expected_hard_count,
            "hard_violation_count mismatch for case '{}' (norm={} pt={}): expected={} actual={}",
            case.label,
            case.norm_code,
            case.project_type,
            case.expected_hard_count,
            result.hard_violation_count
        );
        assert_eq!(
            result.compliant, case.expected_compliant,
            "compliant mismatch for case '{}': expected={} actual={}",
            case.label, case.expected_compliant, result.compliant
        );
    }
}

// T-3.3 acceptance criterion 4: pump-bypassed pipe skips slope rules
#[test]
fn pump_bypassed_pipe_slope_violation_is_skipped() {
    // The slope_violations_with_bypass case: 3 pipes all slope=0.001 below min_slope=0.005
    // but one has bypassed_by_pump=true → only 2 counted, not 3
    let fixture = load_norms_violations_golden();
    let bypass_case = fixture
        .cases
        .iter()
        .find(|c| c.label == "slope_violations_with_bypass")
        .expect("slope_violations_with_bypass case must exist");
    // 3 pipes, 1 bypassed → 2 violations (slope only, no other violations in this case)
    assert_eq!(
        bypass_case.expected_hard_count, 2,
        "bypass case should have 2 hard violations"
    );
    assert!(
        !bypass_case.expected_compliant,
        "bypass case should be non-compliant"
    );
}

// T-3.3 acceptance criterion 5: nodes without pressure_mca are skipped
#[test]
fn pressure_case_skips_reservoir_node() {
    let fixture = load_norms_violations_golden();
    let pressure_case = fixture
        .cases
        .iter()
        .find(|c| c.label == "pressure_violations")
        .expect("pressure_violations case must exist");
    // 2 junction nodes with pressure_mca=8.0 + 1 reservoir (skipped) → 2 violations
    assert_eq!(
        pressure_case.expected_hard_count, 2,
        "pressure case should have exactly 2 hard violations (reservoir skipped)"
    );
}

// T-3.3 acceptance criterion 6: proptest invariant — all within bounds → 0 violations
#[test]
fn all_within_bounds_produces_zero_violations() {
    // Use CONAGUA_MX SEWER with a network where all values are within bounds
    let profile = NormRegistry::get("CONAGUA_MX").unwrap();
    let validator = NormValidator::new(profile);

    let nodes = vec![
        NetworkNode::new(NodeId::new("n1"), 0.0, 0.0, 10.0, NodeType::Manhole),
        NetworkNode::new(NodeId::new("n2"), 100.0, 0.0, 10.0, NodeType::Manhole),
    ];
    let pipe = NetworkPipe {
        id: PipeId::new("p1"),
        start_node_id: NodeId::new("n1"),
        end_node_id: NodeId::new("n2"),
        length: 100.0,
        effective_length: 100.0,
        diameter: 0.3,
        material: PipeMaterial::Pvc,
        roughness: 0.009,
        start_invert: 8.5,
        end_invert: 6.5, // slope = (8.5-6.5)/100 = 0.02
        slope: 0.02,     // within [0.005, 0.05]
        design_flow: 0.005,
        waypoints: vec![],
        metadata: Default::default(),
    };
    let network = PipeNetwork::new("test", nodes, vec![pipe]);
    let result = validator.validate(&network, ProjectType::Sewer);
    assert_eq!(
        result.hard_violation_count, 0,
        "all-within-bounds network must have 0 hard violations"
    );
    assert!(
        result.compliant,
        "all-within-bounds network must be compliant"
    );
}
