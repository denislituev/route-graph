//! Routing graph data model.
//!
//! The model represents the path of an HTTP request through a reverse proxy:
//!
//! ```text
//! Client → Listener → Host → PathMatch → [Middleware]* → Backend
//! ```

use std::fmt;

/// Unique identifier for a node in the routing graph.
///
/// This is a newtype over `u32` — lightweight, `Copy`, and usable as an index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(u32);

impl NodeId {
    /// Returns the raw `u32` value.
    pub fn as_u32(self) -> u32 {
        self.0
    }
}

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "n{}", self.0)
    }
}

/// The kind of a routing node, determining its role in the graph.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum NodeKind {
    /// The originating client (root of the graph).
    Client,
    /// A listening address/port (e.g. `:443`).
    Listener,
    /// A virtual host (e.g. `example.com`).
    Host,
    /// A path matcher (e.g. `/api/*`).
    PathMatch,
    /// A middleware step (e.g. `strip_prefix`, `auth`).
    Middleware,
    /// An upstream backend (e.g. `http://backend:8080`).
    Backend,
}

impl fmt::Display for NodeKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NodeKind::Client => write!(f, "client"),
            NodeKind::Listener => write!(f, "listener"),
            NodeKind::Host => write!(f, "host"),
            NodeKind::PathMatch => write!(f, "path_match"),
            NodeKind::Middleware => write!(f, "middleware"),
            NodeKind::Backend => write!(f, "backend"),
        }
    }
}

/// Network protocol.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Protocol {
    Http,
    Https,
    Grpc,
    Tcp,
    Udp,
}

impl fmt::Display for Protocol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Protocol::Http => write!(f, "HTTP"),
            Protocol::Https => write!(f, "HTTPS"),
            Protocol::Grpc => write!(f, "gRPC"),
            Protocol::Tcp => write!(f, "TCP"),
            Protocol::Udp => write!(f, "UDP"),
        }
    }
}

/// TLS configuration for a listener.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct TlsConfig {
    /// Whether TLS is configured automatically (e.g. Caddy's Auto HTTPS).
    pub auto: bool,
    /// Path to the certificate file.
    pub cert_path: Option<String>,
    /// SNI hostname override.
    pub sni: Option<String>,
}

/// Condition on a routing edge.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum EdgeCondition {
    /// Unconditional (default routing).
    Always,
    /// Glob-style path match (e.g. `/api/*`).
    PathGlob(String),
    /// Path prefix match (e.g. `/api/`).
    PathPrefix(String),
    /// Exact path match.
    PathExact(String),
    /// Header name/value match.
    HeaderMatch { name: String, value: String },
    /// HTTP method match.
    Method(String),
}

impl fmt::Display for EdgeCondition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Always => Ok(()),
            Self::PathGlob(p) => write!(f, "{p}"),
            Self::PathPrefix(p) => write!(f, "{p}*"),
            Self::PathExact(p) => write!(f, "{p}"),
            Self::HeaderMatch { name, value } => write!(f, "{name}: {value}"),
            Self::Method(m) => write!(f, "{m}"),
        }
    }
}

/// Additional metadata attached to a node.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct Metadata {
    /// Listening or backend port.
    pub port: Option<u16>,
    /// Network protocol.
    pub protocol: Option<Protocol>,
    /// TLS configuration (for listeners).
    pub tls: Option<TlsConfig>,
    /// Arbitrary key-value pairs specific to the proxy format.
    pub custom: Vec<(String, String)>,
}

impl Metadata {
    /// Creates empty metadata.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the port.
    pub fn with_port(mut self, port: u16) -> Self {
        self.port = Some(port);
        self
    }

    /// Sets the protocol.
    pub fn with_protocol(mut self, protocol: Protocol) -> Self {
        self.protocol = Some(protocol);
        self
    }

    /// Adds a custom key-value pair.
    pub fn with_custom(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.custom.push((key.into(), value.into()));
        self
    }
}

/// A node in the routing graph.
#[derive(Debug, Clone)]
pub struct Node {
    /// Unique identifier within this graph.
    pub id: NodeId,
    /// What role this node plays.
    pub kind: NodeKind,
    /// Human-readable label (address, hostname, path, etc.).
    pub label: String,
    /// Additional metadata.
    pub metadata: Metadata,
}

/// A directed edge between two nodes.
#[derive(Debug, Clone)]
pub struct Edge {
    /// Source node.
    pub source: NodeId,
    /// Target node.
    pub target: NodeId,
    /// Optional condition for this routing step.
    pub condition: Option<EdgeCondition>,
}

/// The routing graph — the central data structure.
///
/// Holds a flat list of nodes and edges with precomputed adjacency indices
/// for O(1) traversal. Parsers produce this via [`RouteGraphBuilder`],
/// renderers consume it.
#[derive(Debug, Clone)]
pub struct RouteGraph {
    nodes: Vec<Node>,
    edges: Vec<Edge>,
    root_id: Option<NodeId>,
    /// Adjacency: `children_of[node_id]` → indices into `edges`.
    children_of: Vec<Vec<usize>>,
    /// Reverse adjacency: `parents_of[node_id]` → indices into `edges`.
    parents_of: Vec<Vec<usize>>,
}

impl RouteGraph {
    /// Creates a new empty graph.
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            edges: Vec::new(),
            root_id: None,
            children_of: Vec::new(),
            parents_of: Vec::new(),
        }
    }

    /// Returns a new graph builder.
    pub fn builder() -> crate::RouteGraphBuilder {
        crate::RouteGraphBuilder::new()
    }

    // -----------------------------------------------------------------------
    // Accessors
    // -----------------------------------------------------------------------

    /// Returns all nodes.
    pub fn nodes(&self) -> &[Node] {
        &self.nodes
    }

    /// Returns all edges.
    pub fn edges(&self) -> &[Edge] {
        &self.edges
    }

    /// Looks up a node by its ID. O(1).
    pub fn get_node(&self, id: NodeId) -> Option<&Node> {
        self.nodes.get(id.as_u32() as usize)
    }

    /// Looks up a node by its ID for mutation.
    pub(crate) fn get_node_mut(&mut self, id: NodeId) -> &mut Node {
        &mut self.nodes[id.as_u32() as usize]
    }

    /// Returns the root `Client` node, if any. O(1).
    pub fn root(&self) -> Option<&Node> {
        self.root_id.and_then(|id| self.get_node(id))
    }

    /// Returns the root `NodeId`, if any.
    pub fn root_id(&self) -> Option<NodeId> {
        self.root_id
    }

    /// Returns edges outgoing from the given node. O(k) where k = out-degree.
    pub fn edges_from(&self, id: NodeId) -> impl Iterator<Item = &Edge> {
        self.children_of(id).map(|edge_idx| &self.edges[edge_idx])
    }

    /// Returns edges incoming to the given node. O(k) where k = in-degree.
    pub fn edges_to(&self, id: NodeId) -> impl Iterator<Item = &Edge> {
        self.parents_of(id).map(|edge_idx| &self.edges[edge_idx])
    }

    /// Returns child node IDs of the given node. No allocation.
    pub fn children_ids(&self, id: NodeId) -> impl Iterator<Item = NodeId> + '_ {
        self.children_of(id)
            .map(|edge_idx| self.edges[edge_idx].target)
    }

    /// Returns child nodes of the given node.
    pub fn children(&self, id: NodeId) -> impl Iterator<Item = &Node> + '_ {
        self.children_ids(id)
            .filter_map(move |cid| self.get_node(cid))
    }

    /// Returns parent node IDs of the given node. No allocation.
    pub fn parent_ids(&self, id: NodeId) -> impl Iterator<Item = NodeId> + '_ {
        self.parents_of(id)
            .map(|edge_idx| self.edges[edge_idx].source)
    }

    /// Returns the number of nodes.
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Returns the number of edges.
    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }

    /// Returns true if the graph has no nodes.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    // -----------------------------------------------------------------------
    // Mutation (pub(crate) — only builder uses these)
    // -----------------------------------------------------------------------

    /// Inserts a node, returning its ID. O(1).
    /// Maintains adjacency indices.
    pub(crate) fn add_node(&mut self, kind: NodeKind, label: String, metadata: Metadata) -> NodeId {
        let is_client = matches!(kind, NodeKind::Client);
        let id = NodeId(self.nodes.len() as u32);
        self.nodes.push(Node {
            id,
            kind,
            label,
            metadata,
        });
        self.children_of.push(Vec::new());
        self.parents_of.push(Vec::new());
        if self.root_id.is_none() && is_client {
            self.root_id = Some(id);
        }
        id
    }

    /// Inserts a directed edge. O(1).
    /// Maintains adjacency indices.
    pub(crate) fn add_edge(
        &mut self,
        source: NodeId,
        target: NodeId,
        condition: Option<EdgeCondition>,
    ) -> usize {
        let idx = self.edges.len();
        self.edges.push(Edge {
            source,
            target,
            condition,
        });
        self.children_of[source.as_u32() as usize].push(idx);
        self.parents_of[target.as_u32() as usize].push(idx);
        idx
    }

    /// Directly sets the condition on an edge by index. O(1).
    pub(crate) fn set_edge_condition_by_idx(&mut self, edge_idx: usize, condition: EdgeCondition) {
        if let Some(edge) = self.edges.get_mut(edge_idx) {
            edge.condition = Some(condition);
        }
    }

    // -----------------------------------------------------------------------
    // Validation
    // -----------------------------------------------------------------------

    /// Validates graph integrity. Returns `Ok(())` if valid,
    /// or a list of validation errors.
    pub fn validate(&self) -> Result<(), Vec<ValidationError>> {
        let mut errors = Vec::new();

        // Check edge bounds and self-loops
        for edge in &self.edges {
            let src = edge.source.as_u32() as usize;
            let tgt = edge.target.as_u32() as usize;
            if src >= self.nodes.len() {
                errors.push(ValidationError {
                    message: format!("edge references invalid source {}", edge.source),
                });
            }
            if tgt >= self.nodes.len() {
                errors.push(ValidationError {
                    message: format!("edge references invalid target {}", edge.target),
                });
            }
            if edge.source == edge.target {
                errors.push(ValidationError {
                    message: format!("self-loop on node {}", edge.source),
                });
            }
        }

        // At most one Client
        let clients: Vec<_> = self
            .nodes
            .iter()
            .filter(|n| matches!(n.kind, NodeKind::Client))
            .collect();
        if clients.len() > 1 {
            errors.push(ValidationError {
                message: format!("{} Client nodes found, expected at most 1", clients.len()),
            });
        }

        // Client has no incoming edges
        if let Some(client) = clients.first() {
            let parent_count = self.parents_of(client.id).count();
            if parent_count > 0 {
                errors.push(ValidationError {
                    message: "Client node must have no incoming edges".into(),
                });
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    // -----------------------------------------------------------------------
    // Private helpers
    // -----------------------------------------------------------------------

    /// Returns edge indices for children of a node.
    fn children_of(&self, id: NodeId) -> impl Iterator<Item = usize> + '_ {
        self.children_of
            .get(id.as_u32() as usize)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
            .iter()
            .copied()
    }

    /// Returns edge indices for parents of a node.
    fn parents_of(&self, id: NodeId) -> impl Iterator<Item = usize> + '_ {
        self.parents_of
            .get(id.as_u32() as usize)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
            .iter()
            .copied()
    }
}

impl Default for RouteGraph {
    fn default() -> Self {
        Self::new()
    }
}

/// A validation error found in a [`RouteGraph`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationError {
    /// Description of the problem.
    pub message: String,
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ValidationError {}
