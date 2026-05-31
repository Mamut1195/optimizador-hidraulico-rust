//! Network topology types: NodeId, PipeId, NetworkNode, NetworkPipe, PipeNetwork.
//!
//! Mirrors Python oracle: `hydro_engine/core/node.py` and the pipe/network
//! structures. IDs are String-backed newtypes (ADR-7): oracle uses ids like
//! "g12" and "Pipe - (3)" which must round-trip exactly (spec S-21).

use std::collections::HashMap;
use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::enums::{NodeType, PipeMaterial};

// ── NodeId ────────────────────────────────────────────────────────────────────

/// Unique identifier for a network node.
///
/// String-backed newtype (ADR-7). Oracle ids like "g12" and "Pipe - (3)"
/// must round-trip exactly through JSON without any numeric coercion.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct NodeId(pub String);

impl NodeId {
    pub fn new(s: impl Into<String>) -> Self {
        NodeId(s.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<&str> for NodeId {
    fn from(s: &str) -> Self {
        NodeId(s.to_owned())
    }
}

impl From<String> for NodeId {
    fn from(s: String) -> Self {
        NodeId(s)
    }
}

// ── PipeId ────────────────────────────────────────────────────────────────────

/// Unique identifier for a network pipe.
///
/// String-backed newtype, same rationale as NodeId.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PipeId(pub String);

impl PipeId {
    pub fn new(s: impl Into<String>) -> Self {
        PipeId(s.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for PipeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<&str> for PipeId {
    fn from(s: &str) -> Self {
        PipeId(s.to_owned())
    }
}

impl From<String> for PipeId {
    fn from(s: String) -> Self {
        PipeId(s)
    }
}

// ── NetworkNode ───────────────────────────────────────────────────────────────

/// A node in the hydraulic network (manhole, junction, outlet, etc.).
///
/// Mirrors `NetworkNode` from `hydro_engine/core/node.py`. Serialization
/// field names match the JSON output schema (spec R-02).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NetworkNode {
    /// Unique node identifier (string, round-trips as-is).
    pub id: NodeId,
    /// X coordinate (easting) in meters.
    pub x: f64,
    /// Y coordinate (northing) in meters.
    pub y: f64,
    /// Ground elevation in meters.
    pub z: f64,
    /// Classification of this node.
    pub node_type: NodeType,
    /// Top-of-structure elevation in meters.
    pub rim_elevation: f64,
    /// Bottom-of-structure elevation in meters.
    pub sump_elevation: f64,
    /// Water demand at this node in L/s (0.0 default).
    #[serde(default)]
    pub demand: f64,
    /// Free-form metadata (Python: dict[str, Any]).
    ///
    /// Known keys used by the validator:
    /// - `"pressure_mca"`: f64 — static pressure at the node (m.c.a.)
    #[serde(default)]
    pub metadata: HashMap<String, Value>,
}

impl NetworkNode {
    /// Construct a node. Defaults mirror the Python oracle:
    /// `rim_elevation = z`, `sump_elevation = z - 1.5`, `demand = 0.0`.
    pub fn new(id: impl Into<NodeId>, x: f64, y: f64, z: f64, node_type: NodeType) -> Self {
        NetworkNode {
            id: id.into(),
            x,
            y,
            z,
            node_type,
            rim_elevation: z,
            sump_elevation: z - 1.5,
            demand: 0.0,
            metadata: HashMap::new(),
        }
    }

    /// 2D Euclidean distance to another node (horizontal only).
    pub fn distance_to(&self, other: &NetworkNode) -> f64 {
        ((self.x - other.x).powi(2) + (self.y - other.y).powi(2)).sqrt()
    }
}

// ── NetworkPipe ───────────────────────────────────────────────────────────────

/// A pipe segment in the hydraulic network.
///
/// Field names match the JSON output schema (spec R-02).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NetworkPipe {
    pub id: PipeId,
    pub start_node_id: NodeId,
    pub end_node_id: NodeId,
    /// Straight-line length in meters.
    pub length: f64,
    /// Polyline length including waypoints in meters.
    pub effective_length: f64,
    /// Pipe diameter in meters (domain unit).
    pub diameter: f64,
    pub material: PipeMaterial,
    /// Manning's roughness coefficient `n`.
    pub roughness: f64,
    /// Start invert elevation in meters.
    pub start_invert: f64,
    /// End invert elevation in meters.
    pub end_invert: f64,
    /// Slope, dimensionless (positive = downhill).
    pub slope: f64,
    /// Design flow in m³/s.
    pub design_flow: f64,
    /// Intermediate waypoint coordinates [[x, y], ...].
    #[serde(default)]
    pub waypoints: Vec<[f64; 2]>,
    /// Free-form metadata (Python: dict[str, Any]).
    ///
    /// Known keys used by the validator:
    /// - `"bypassed_by_pump"`: bool — when true, slope rules are skipped
    /// - `"flow_m3s"`: f64 — design flow for pressure velocity computation
    #[serde(default)]
    pub metadata: HashMap<String, Value>,
}

// ── PipeNetwork ───────────────────────────────────────────────────────────────

/// A complete pipe network: nodes + pipes + a quick lookup map.
///
/// Field names match the `network` object in the Solution JSON (spec R-02).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipeNetwork {
    pub name: String,
    pub nodes: Vec<NetworkNode>,
    pub pipes: Vec<NetworkPipe>,
    /// Total pipe length (sum of all pipe `length` fields) in meters.
    pub total_length: f64,
    /// Adjacency lookup: NodeId → list of (neighbor NodeId, pipe index).
    ///
    /// Not serialized — rebuilt from `nodes` + `pipes` on demand.
    #[serde(skip)]
    pub adjacency: HashMap<NodeId, Vec<(NodeId, usize)>>,
    /// Free-form network-level metadata (Python: network.metadata dict).
    ///
    /// Used by PumpStationSolver to store pump selection and wet-well info
    /// that evaluate() needs to read back.  Skipped from adjacency but included
    /// in serde for JSON round-trips.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub metadata: HashMap<String, serde_json::Value>,
}

impl PipeNetwork {
    pub fn new(name: impl Into<String>, nodes: Vec<NetworkNode>, pipes: Vec<NetworkPipe>) -> Self {
        let total_length = pipes.iter().map(|p| p.length).sum();
        let mut adjacency: HashMap<NodeId, Vec<(NodeId, usize)>> = HashMap::new();
        for (i, pipe) in pipes.iter().enumerate() {
            adjacency
                .entry(pipe.start_node_id.clone())
                .or_default()
                .push((pipe.end_node_id.clone(), i));
            adjacency
                .entry(pipe.end_node_id.clone())
                .or_default()
                .push((pipe.start_node_id.clone(), i));
        }
        PipeNetwork {
            name: name.into(),
            nodes,
            pipes,
            total_length,
            adjacency,
            metadata: HashMap::new(),
        }
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    pub fn pipe_count(&self) -> usize {
        self.pipes.len()
    }

    /// Look up a node by its string id.
    ///
    /// Returns `None` when no node with the given id exists. Performs a linear
    /// scan — suitable for network sizes encountered in hydraulic design
    /// (typically ≤ a few thousand nodes).
    pub fn get_node(&self, id: &NodeId) -> Option<&NetworkNode> {
        self.nodes.iter().find(|n| &n.id == id)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests (T-1.1 NodeId/PipeId + T-1.3 structs)
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // T-1.1: NodeId newtype round-trips through JSON
    #[test]
    fn node_id_roundtrip_oracle_style_g12() {
        let id = NodeId::new("g12");
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, "\"g12\"");
        let back: NodeId = serde_json::from_str(&json).unwrap();
        assert_eq!(back, id);
    }

    #[test]
    fn node_id_roundtrip_pipe_with_spaces_and_parens() {
        let id = NodeId::new("Pipe - (3)");
        let json = serde_json::to_string(&id).unwrap();
        let back: NodeId = serde_json::from_str(&json).unwrap();
        assert_eq!(back, id);
    }

    // T-1.1: PipeId newtype
    #[test]
    fn pipe_id_roundtrip() {
        let id = PipeId::new("p-001");
        let json = serde_json::to_string(&id).unwrap();
        let back: PipeId = serde_json::from_str(&json).unwrap();
        assert_eq!(back, id);
    }

    // T-1.3: NetworkNode default construction
    #[test]
    fn network_node_defaults_match_oracle() {
        let node = NetworkNode::new("g12", 100.0, 200.0, 50.0, NodeType::Manhole);
        assert_eq!(node.rim_elevation, 50.0);
        assert!((node.sump_elevation - (50.0 - 1.5)).abs() < 1e-12);
        assert_eq!(node.demand, 0.0);
    }

    // T-1.3: NetworkNode JSON round-trip
    #[test]
    fn network_node_json_roundtrip() {
        let node = NetworkNode::new("g12", 100.0, 200.0, 50.0, NodeType::Manhole);
        let json = serde_json::to_string(&node).unwrap();
        let back: NetworkNode = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, node.id);
        assert_eq!(back.node_type, node.node_type);
        assert!((back.z - node.z).abs() < 1e-12);
    }

    // T-1.3: PipeNetwork node_count / pipe_count
    #[test]
    fn pipe_network_counts_are_correct() {
        let nodes = vec![
            NetworkNode::new("n1", 0.0, 0.0, 10.0, NodeType::Manhole),
            NetworkNode::new("n2", 10.0, 0.0, 9.0, NodeType::Junction),
        ];
        let pipe = NetworkPipe {
            id: PipeId::new("p1"),
            start_node_id: NodeId::new("n1"),
            end_node_id: NodeId::new("n2"),
            length: 10.0,
            effective_length: 10.0,
            diameter: 0.3,
            material: PipeMaterial::Pvc,
            roughness: 0.009,
            start_invert: 9.5,
            end_invert: 8.5,
            slope: 0.1,
            design_flow: 0.001,
            waypoints: vec![],
            metadata: Default::default(),
        };
        let net = PipeNetwork::new("test", nodes, vec![pipe]);
        assert_eq!(net.node_count(), 2);
        assert_eq!(net.pipe_count(), 1);
        assert!((net.total_length - 10.0).abs() < 1e-12);
    }
}
