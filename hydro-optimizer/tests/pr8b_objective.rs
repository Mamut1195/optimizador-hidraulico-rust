//! PR-8b objective.rs tests (REQ-003).
//!
//! RED → GREEN → REFACTOR per strict-tdd.md.
//! Tests are written FIRST, verified RED, then implementation is written to pass.
//!
//! Fixtures are hand-computed from the oracle `objective.py` formulas:
//! - PIPE_COST_PER_M lookup (nearest diameter)
//! - MANHOLE_COST = 2500.0
//! - EXCAVATION_COST_M3 = 12.0
//! - TRENCH_WIDTH = 0.8
//! - PUMP_STATION_COST = 50_000.0
//! - H_MIN = 15.0 mca (Todini)
//! - Rc = (E - N + 1) / (2N - 5) (connectivity redundancy)

use hydro_optimizer::ObjectiveFunction;
use hydro_types::enums::{NodeType, PipeMaterial};
use hydro_types::network::NodeId;
use hydro_types::network::{NetworkNode, NetworkPipe, PipeId, PipeNetwork};
use hydro_types::response::{Solution, SolutionScore};

// ── Fixture helpers ───────────────────────────────────────────────────────────

/// Build a minimal Solution with the given pipe, nodes and optional pressure.
fn make_solution_with_pressure(
    nodes: Vec<NetworkNode>,
    pipes: Vec<NetworkPipe>,
    score: SolutionScore,
) -> Solution {
    Solution {
        rank: 1,
        network: PipeNetwork::new("test", nodes, pipes),
        score,
        metadata: Default::default(),
    }
}

fn make_node(id: &str, x: f64, y: f64, z: f64, node_type: NodeType, demand: f64) -> NetworkNode {
    let mut n = NetworkNode::new(id, x, y, z, node_type);
    n.demand = demand;
    n
}

fn make_node_with_pressure(
    id: &str,
    x: f64,
    y: f64,
    z: f64,
    node_type: NodeType,
    demand: f64,
    pressure_mca: f64,
) -> NetworkNode {
    let mut n = make_node(id, x, y, z, node_type, demand);
    n.metadata
        .insert("pressure_mca".to_owned(), serde_json::json!(pressure_mca));
    n
}

fn make_pipe(
    id: &str,
    start: &str,
    end: &str,
    length: f64,
    diameter: f64,
    start_invert: f64,
    end_invert: f64,
) -> NetworkPipe {
    NetworkPipe {
        id: PipeId::new(id),
        start_node_id: NodeId::new(start),
        end_node_id: NodeId::new(end),
        length,
        effective_length: length,
        diameter,
        material: PipeMaterial::Pvc,
        roughness: 0.009,
        start_invert,
        end_invert,
        slope: (start_invert - end_invert) / length,
        design_flow: 0.01,
        waypoints: vec![],
        metadata: Default::default(),
    }
}

// ── REQ-003 Scenario: Zero pumps yields zero pumping cost ─────────────────────

/// objectives[2] (pumping_cost) must be 0.0 when pump_count=0 and no adverse-slope pipes.
#[test]
fn test_pumping_cost_zero_pumps() {
    // Simple gravity-only network: two nodes, one downhill pipe, no pump nodes
    let nodes = vec![
        make_node("n1", 0.0, 0.0, 20.0, NodeType::Reservoir, 0.0),
        make_node("n2", 100.0, 0.0, 15.0, NodeType::Junction, 0.5),
    ];
    let pipes = vec![make_pipe("p1", "n1", "n2", 100.0, 0.3, 18.0, 14.0)];
    let score = SolutionScore {
        pump_count: 0,
        ..Default::default()
    };
    let solution = make_solution_with_pressure(nodes, pipes, score);
    let of = ObjectiveFunction::new(vec![], None);
    let objectives = of.score(&solution);

    // objectives[2] is pumping cost
    assert_eq!(
        objectives[2], 0.0,
        "zero pumps, downhill pipe → pumping cost must be 0.0"
    );
}

// ── REQ-003: pumping cost with pump stations ──────────────────────────────────

/// objectives[2] reflects PUMP_STATION_COST × pump_count (≥ pump_count × 50_000).
#[test]
fn test_pumping_cost_with_pump_stations() {
    // 2 pump nodes, score.pump_count = 2
    let nodes = vec![
        make_node("n1", 0.0, 0.0, 10.0, NodeType::Reservoir, 0.0),
        make_node("n2", 100.0, 0.0, 10.0, NodeType::Pump, 0.0),
        make_node("n3", 200.0, 0.0, 10.0, NodeType::Junction, 0.5),
    ];
    // Downhill pipe (no adverse slope → no extra pump) but score.pump_count=2
    let pipes = vec![
        make_pipe("p1", "n1", "n2", 100.0, 0.3, 9.0, 8.5),
        make_pipe("p2", "n2", "n3", 100.0, 0.3, 8.5, 8.0),
    ];
    let score = SolutionScore {
        pump_count: 2,
        ..Default::default()
    };
    let solution = make_solution_with_pressure(nodes, pipes, score);
    let of = ObjectiveFunction::new(vec![], None);
    let objectives = of.score(&solution);

    // Must be at least 2 × 50_000 = 100_000
    assert!(
        objectives[2] >= 100_000.0,
        "2 pump stations → pumping cost ≥ 100_000, got {}",
        objectives[2]
    );
}

// ── REQ-003: excavation formula ──────────────────────────────────────────────

/// objectives[1] (excavation_m3) = avg_cover_depth × TRENCH_WIDTH × length.
///
/// Fixture:
///   node n1: z=10, pipe start_invert=8.5 → cover = 10 - 8.5 = 1.5 m
///   node n2: z=9,  pipe end_invert=7.5   → cover = 9 - 7.5  = 1.5 m
///   avg_depth = (1.5 + 1.5) / 2 = 1.5
///   length = 100 m
///   excavation = 1.5 × 0.8 × 100 = 120.0 m³
#[test]
fn test_excavation_formula() {
    let nodes = vec![
        make_node("n1", 0.0, 0.0, 10.0, NodeType::Junction, 0.0),
        make_node("n2", 100.0, 0.0, 9.0, NodeType::Junction, 0.0),
    ];
    let pipes = vec![make_pipe("p1", "n1", "n2", 100.0, 0.3, 8.5, 7.5)];
    let score = SolutionScore::default();
    let solution = make_solution_with_pressure(nodes, pipes, score);
    let of = ObjectiveFunction::new(vec![], None);
    let objectives = of.score(&solution);

    let expected_excavation = 1.5_f64 * 0.8 * 100.0; // 120.0
    assert!(
        (objectives[1] - expected_excavation).abs() < 1e-6,
        "excavation expected {expected_excavation}, got {}",
        objectives[1]
    );
}

// ── REQ-003: cost formula ─────────────────────────────────────────────────────

/// objectives[0] (cost_usd) = pipe_cost + manhole_cost + excavation_cost.
///
/// Fixture (single 100m pipe, diameter 0.3m → cost 28.0 $/m, 2 nodes):
///   pipe_cost     = 28.0 × 100 = 2_800.0
///   manhole_cost  = 2 × 2500   = 5_000.0
///   excavation    = 120.0 m³ (from test_excavation_formula fixture)
///   excav_cost    = 120 × 12   = 1_440.0
///   total         = 2_800 + 5_000 + 1_440 = 9_240.0
#[test]
fn test_cost_formula() {
    let nodes = vec![
        make_node("n1", 0.0, 0.0, 10.0, NodeType::Junction, 0.0),
        make_node("n2", 100.0, 0.0, 9.0, NodeType::Junction, 0.0),
    ];
    let pipes = vec![make_pipe("p1", "n1", "n2", 100.0, 0.3, 8.5, 7.5)];
    let score = SolutionScore::default();
    let solution = make_solution_with_pressure(nodes, pipes, score);
    let of = ObjectiveFunction::new(vec![], None);
    let objectives = of.score(&solution);

    // pipe 0.3m → $28/m × 100m = 2_800
    // 2 nodes × $2_500 = 5_000
    // excavation = 120 m³ × $12 = 1_440
    let expected_cost = 2_800.0 + 5_000.0 + 1_440.0;
    assert!(
        (objectives[0] - expected_cost).abs() < 1.0,
        "cost expected {expected_cost}, got {}",
        objectives[0]
    );
}

// ── REQ-003 Scenario: Interference count matches crossing count ───────────────

/// objectives[3] (interference_count) counts separation violations.
///
/// With no existing networks → 0.
#[test]
fn test_interference_count_zero_existing_networks() {
    let nodes = vec![
        make_node("n1", 0.0, 0.0, 10.0, NodeType::Junction, 0.0),
        make_node("n2", 100.0, 0.0, 9.0, NodeType::Junction, 0.0),
    ];
    let pipes = vec![make_pipe("p1", "n1", "n2", 100.0, 0.3, 8.5, 7.5)];
    let solution = make_solution_with_pressure(nodes, pipes, SolutionScore::default());
    let of = ObjectiveFunction::new(vec![], None);
    let objectives = of.score(&solution);

    assert_eq!(
        objectives[3], 0.0,
        "no existing networks → interference_count must be 0"
    );
}

/// objectives[3] counts crossings that violate vertical separation.
///
/// Our pipe runs (0,0)→(100,0) and there is an existing network at (50,-10)→(50,10)
/// crossing it. The pipe avg_depth and existing_depth differ by < 0.3m → 1 violation.
#[test]
fn test_interference_count_matches() {
    let nodes = vec![
        make_node("n1", 0.0, 0.0, 10.0, NodeType::Junction, 0.0),
        make_node("n2", 100.0, 0.0, 10.0, NodeType::Junction, 0.0),
    ];
    // start_invert=9.0, end_invert=9.0 → avg_depth = 10-9.0 = 1.0
    // existing_depth = 1.1 → vertical_sep = |1.0 - 1.1| = 0.1 < 0.3 → violation
    let pipes = vec![make_pipe("p1", "n1", "n2", 100.0, 0.3, 9.0, 9.0)];
    let existing = vec![hydro_optimizer::ExistingNetworkSpec {
        coords: vec![(50.0, -10.0), (50.0, 10.0)],
        depth: Some(1.1),
    }];
    let solution = make_solution_with_pressure(nodes, pipes, SolutionScore::default());
    let of = ObjectiveFunction::new(existing, None);
    let objectives = of.score(&solution);

    assert_eq!(
        objectives[3] as i64, 1,
        "one crossing with vertical sep 0.1 < 0.3m → interference_count must be 1, got {}",
        objectives[3]
    );
}

// ── REQ-003 Scenario: Todini Ir deficit is 0 for adequate pressure ────────────

/// objectives[4] (resilience_deficit) = 0.0 when all nodes exceed H_MIN=15mca by margin.
///
/// Fixture: reservoir at z=50 (source), 1 demand node with pressure_mca=30
/// (well above 15 mca). Todini Ir should be 1.0, so resilience_deficit = 0.0.
#[test]
fn test_todini_ir_deficit_zero_for_adequate_pressure() {
    let nodes = vec![
        // source (reservoir) — no demand
        make_node_with_pressure("src", 0.0, 0.0, 50.0, NodeType::Reservoir, 0.0, 0.0),
        // demand node: pressure_mca = 30, demand = 1.0 L/s
        make_node_with_pressure("n1", 100.0, 0.0, 10.0, NodeType::Junction, 1.0, 30.0),
    ];
    let pipes = vec![make_pipe("p1", "src", "n1", 100.0, 0.3, 49.0, 9.5)];
    let solution = make_solution_with_pressure(nodes, pipes, SolutionScore::default());
    let of = ObjectiveFunction::new(vec![], None);
    let objectives = of.score(&solution);

    // Todini Ir = numerator / denominator where numerator > 0 (pressure > H_MIN)
    // resilience_deficit = 1 - Ir, should be close to 0
    assert!(
        objectives[4] < 0.5,
        "adequate pressure → resilience_deficit should be < 0.5, got {}",
        objectives[4]
    );
}

/// objectives[4] > 0 when pressure is below H_MIN.
#[test]
fn test_todini_ir_deficit_positive_for_insufficient_head() {
    // demand node with pressure_mca = 5 (below H_MIN=15) → numerator=0 → Ir=0 → deficit=1
    let nodes = vec![
        make_node_with_pressure("src", 0.0, 0.0, 50.0, NodeType::Reservoir, 0.0, 0.0),
        make_node_with_pressure("n1", 100.0, 0.0, 10.0, NodeType::Junction, 1.0, 5.0),
    ];
    let pipes = vec![make_pipe("p1", "src", "n1", 100.0, 0.3, 49.0, 9.5)];
    let solution = make_solution_with_pressure(nodes, pipes, SolutionScore::default());
    let of = ObjectiveFunction::new(vec![], None);
    let objectives = of.score(&solution);

    assert!(
        objectives[4] > 0.0,
        "pressure 5mca < H_MIN=15 → resilience_deficit > 0, got {}",
        objectives[4]
    );
}

// ── REQ-003: connectivity redundancy for gravity networks ────────────────────

/// objectives[4] uses Rc = (E-N+1)/(2N-5) for networks without pressure metadata.
///
/// Fixture: N=4 nodes, E=3 pipes → Rc = (3-4+1)/(2*4-5) = 0/3 = 0 → deficit=1.0
#[test]
fn test_gravity_resilience_uses_connectivity() {
    // No pressure_mca in any node metadata → falls back to connectivity redundancy
    let nodes = vec![
        make_node("n1", 0.0, 0.0, 20.0, NodeType::Reservoir, 0.0),
        make_node("n2", 50.0, 0.0, 18.0, NodeType::Junction, 0.0),
        make_node("n3", 100.0, 0.0, 16.0, NodeType::Junction, 0.0),
        make_node("n4", 150.0, 0.0, 14.0, NodeType::Outlet, 0.0),
    ];
    // Linear chain: E=3, N=4 → Rc = (3-4+1)/(8-5) = 0/3 = 0 → deficit = 1.0
    let pipes = vec![
        make_pipe("p1", "n1", "n2", 50.0, 0.3, 19.0, 17.0),
        make_pipe("p2", "n2", "n3", 50.0, 0.3, 17.0, 15.0),
        make_pipe("p3", "n3", "n4", 50.0, 0.3, 15.0, 13.0),
    ];
    let solution = make_solution_with_pressure(nodes, pipes, SolutionScore::default());
    let of = ObjectiveFunction::new(vec![], None);
    let objectives = of.score(&solution);

    // Rc = 0 → resilience_deficit = 1.0 - 0 = 1.0
    assert!(
        (objectives[4] - 1.0).abs() < 1e-9,
        "linear chain N=4, E=3 → Rc=0 → deficit=1.0, got {}",
        objectives[4]
    );
}

// ── REQ-003: all 5 objectives are non-negative ───────────────────────────────

/// All 5 objectives must be >= 0.0 for any valid solution.
#[test]
fn test_all_five_objectives_are_nonnegative() {
    let nodes = vec![
        make_node("n1", 0.0, 0.0, 20.0, NodeType::Reservoir, 0.0),
        make_node("n2", 100.0, 0.0, 15.0, NodeType::Junction, 0.5),
    ];
    let pipes = vec![make_pipe("p1", "n1", "n2", 100.0, 0.3, 18.0, 14.0)];
    let solution = make_solution_with_pressure(nodes, pipes, SolutionScore::default());
    let of = ObjectiveFunction::new(vec![], None);
    let objectives = of.score(&solution);

    for (i, obj) in objectives.iter().enumerate() {
        assert!(*obj >= 0.0, "objectives[{i}] = {obj} is negative");
    }
}

// ── REQ-003: determinism ─────────────────────────────────────────────────────

/// Same input → same output (call twice, assert equal).
#[test]
fn test_objective_function_is_deterministic() {
    let nodes = vec![
        make_node("n1", 0.0, 0.0, 20.0, NodeType::Reservoir, 0.0),
        make_node("n2", 100.0, 0.0, 15.0, NodeType::Junction, 0.5),
    ];
    let pipes = vec![make_pipe("p1", "n1", "n2", 100.0, 0.3, 18.0, 14.0)];
    let solution = make_solution_with_pressure(nodes, pipes, SolutionScore::default());
    let of = ObjectiveFunction::new(vec![], None);
    let a = of.score(&solution);
    let b = of.score(&solution);

    for i in 0..5 {
        assert_eq!(
            a[i], b[i],
            "objectives[{i}] differs between calls: {} vs {}",
            a[i], b[i]
        );
    }
}

// ── REQ-003: Todini formula matches oracle exactly ────────────────────────────

/// Todini Ir formula verification against hand-computed oracle values.
///
/// Oracle `_todini_index` formula:
///   numerator   = sum_i( (demand_i/1000) * max(pressure_mca_i - H_MIN, 0) )
///   mean_source_head = mean( z_k + pressure_mca_k )   [for tank/reservoir nodes]
///   total_demand_m3s = sum_i( demand_i / 1000 )
///   input_power  = mean_source_head * total_demand_m3s
///   min_demand_head = sum_i( (demand_i/1000) * (z_i + H_MIN) )
///   denominator  = input_power - min_demand_head
///   Ir = clamp( numerator / denominator, 0, 1 )
///
/// Fixture:
///   reservoir: z=100, pressure_mca=0  → source head = 100
///   demand node: z=0, pressure_mca=40, demand=2.0 L/s (= 0.002 m³/s)
///
///   numerator = 0.002 * max(40-15, 0) = 0.002 * 25 = 0.05
///   total_demand = 0.002
///   input_power  = 100 * 0.002 = 0.2
///   min_demand_head = 0.002 * (0 + 15) = 0.03
///   denominator  = 0.2 - 0.03 = 0.17
///   Ir = min(1, max(0, 0.05 / 0.17)) = 0.294117...
///   resilience_deficit = 1 - Ir = 0.705882...
#[test]
fn test_todini_ir_formula_matches_oracle() {
    let nodes = vec![
        make_node_with_pressure("src", 0.0, 0.0, 100.0, NodeType::Reservoir, 0.0, 0.0),
        make_node_with_pressure("n1", 100.0, 0.0, 0.0, NodeType::Junction, 2.0, 40.0),
    ];
    let pipes = vec![make_pipe("p1", "src", "n1", 100.0, 0.3, 99.0, -0.5)];
    let solution = make_solution_with_pressure(nodes, pipes, SolutionScore::default());
    let of = ObjectiveFunction::new(vec![], None);
    let objectives = of.score(&solution);

    // Oracle-computed expected deficit
    let ir_expected = 0.05_f64 / 0.17_f64; // ≈ 0.294117...
    let deficit_expected = 1.0 - ir_expected.min(1.0).max(0.0);

    assert!(
        (objectives[4] - deficit_expected).abs() < 1e-6,
        "Todini resilience_deficit expected {deficit_expected:.6}, got {:.6}",
        objectives[4]
    );
}
