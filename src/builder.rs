//! Builder for constructing a [`RouteGraph`].
//!
//! The builder uses a stack-based approach: [`push_node`](Self::push_node)
//! creates a child of the current context, [`pop_node`](Self::pop_node)
//! returns to the previous level.
//!
//! # Example
//!
//! ```rust
//! use routegraph::prelude::*;
//!
//! let mut b = RouteGraph::builder();
//! b.push_node(NodeKind::Client, "Client");
//! b.push_node(NodeKind::Listener, ":443");
//! b.set_port(443);
//! b.push_node(NodeKind::Host, "example.com");
//! b.push_node(NodeKind::PathMatch, "/api/*");
//! b.push_node(NodeKind::Backend, "http://backend:8080");
//! b.pop_node();
//! b.pop_node();
//! b.push_node(NodeKind::PathMatch, "/*");
//! b.push_node(NodeKind::Backend, "http://frontend:3000");
//! let graph = b.build();
//!
//! assert_eq!(graph.node_count(), 7);
//! assert_eq!(graph.edge_count(), 6);
//! assert!(graph.validate().is_ok());
//! ```

use crate::model::{EdgeCondition, Metadata, NodeId, NodeKind, Protocol, RouteGraph, TlsConfig};

/// Stack-based builder for [`RouteGraph`].
///
/// Each call to [`push_node`](Self::push_node) creates a new node as a child
/// of the current context node and makes it the new context.
/// Call [`pop_node`](Self::pop_node) to return to the parent context.
pub struct RouteGraphBuilder {
    graph: RouteGraph,
    stack: Vec<NodeId>,
    /// Index of the last edge created by `push_node`. Used for O(1) condition setting.
    last_edge_idx: Option<usize>,
}

impl RouteGraphBuilder {
    /// Creates a new empty builder.
    #[must_use]
    pub fn new() -> Self {
        Self {
            graph: RouteGraph::new(),
            stack: Vec::new(),
            last_edge_idx: None,
        }
    }

    /// Returns the [`NodeId`] of the current context (top of the stack).
    pub fn current(&self) -> Option<NodeId> {
        self.stack.last().copied()
    }

    // -----------------------------------------------------------------------
    // &mut API — primary interface (parsers and imperative code)
    // -----------------------------------------------------------------------

    /// Pushes a new node as a child of the current context.
    ///
    /// Returns the [`NodeId`] of the newly created node.
    pub fn push_node(&mut self, kind: NodeKind, label: impl Into<String>) -> NodeId {
        let id = self.graph.add_node(kind, label.into(), Metadata::default());
        if let Some(parent) = self.stack.last().copied() {
            let edge_idx = self.graph.add_edge(parent, id, None);
            self.last_edge_idx = Some(edge_idx);
        } else {
            self.last_edge_idx = None;
        }
        self.stack.push(id);
        id
    }

    /// Pops the current context, returning to the parent level.
    ///
    /// # Panics
    ///
    /// Panics if the stack is empty.
    pub fn pop_node(&mut self) {
        self.stack
            .pop()
            .expect("pop called with empty builder stack");
        self.last_edge_idx = None;
    }

    /// Sets the port on the current context node.
    pub fn set_port(&mut self, port: u16) {
        if let Some(id) = self.current() {
            self.graph.get_node_mut(id).metadata.port = Some(port);
        }
    }

    /// Sets the protocol on the current context node.
    pub fn set_protocol(&mut self, protocol: Protocol) {
        if let Some(id) = self.current() {
            self.graph.get_node_mut(id).metadata.protocol = Some(protocol);
        }
    }

    /// Sets TLS auto mode on the current context node.
    pub fn set_tls_auto(&mut self) {
        if let Some(id) = self.current() {
            self.graph.get_node_mut(id).metadata.tls = Some(TlsConfig {
                auto: true,
                cert_path: None,
                sni: None,
            });
        }
    }

    /// Adds a custom metadata key-value pair to the current context node.
    pub fn add_custom(&mut self, key: impl Into<String>, value: impl Into<String>) {
        if let Some(id) = self.current() {
            self.graph
                .get_node_mut(id)
                .metadata
                .custom
                .push((key.into(), value.into()));
        }
    }

    /// Sets the edge condition on the edge created by the last `push_node`. O(1).
    pub fn set_condition(&mut self, condition: EdgeCondition) {
        if let Some(idx) = self.last_edge_idx {
            self.graph.set_edge_condition_by_idx(idx, condition);
        }
    }

    /// Connects two existing nodes with a new edge.
    pub fn add_connection(
        &mut self,
        source: NodeId,
        target: NodeId,
        condition: Option<EdgeCondition>,
    ) {
        self.graph.add_edge(source, target, condition);
    }

    /// Consumes the builder and returns the completed [`RouteGraph`].
    pub fn build(self) -> RouteGraph {
        self.graph
    }

    // -----------------------------------------------------------------------
    // Consuming API — thin wrappers for chaining convenience
    // -----------------------------------------------------------------------

    /// Consuming variant of [`push_node`](Self::push_node).
    #[must_use]
    pub fn push(mut self, kind: NodeKind, label: impl Into<String>) -> Self {
        self.push_node(kind, label);
        self
    }

    /// Consuming variant of [`pop_node`](Self::pop_node).
    #[must_use]
    pub fn pop(mut self) -> Self {
        self.pop_node();
        self
    }

    /// Consuming variant of [`set_port`](Self::set_port).
    #[must_use]
    pub fn with_port(mut self, port: u16) -> Self {
        self.set_port(port);
        self
    }

    /// Consuming variant of [`set_protocol`](Self::set_protocol).
    #[must_use]
    pub fn with_protocol(mut self, protocol: Protocol) -> Self {
        self.set_protocol(protocol);
        self
    }

    /// Consuming variant of [`set_tls_auto`](Self::set_tls_auto).
    #[must_use]
    pub fn with_tls_auto(mut self) -> Self {
        self.set_tls_auto();
        self
    }

    /// Consuming variant of [`add_custom`](Self::add_custom).
    #[must_use]
    pub fn with_custom(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.add_custom(key, value);
        self
    }

    /// Consuming variant of [`set_condition`](Self::set_condition).
    #[must_use]
    pub fn with_condition(mut self, condition: EdgeCondition) -> Self {
        self.set_condition(condition);
        self
    }

    /// Consuming variant of [`add_connection`](Self::add_connection).
    #[must_use]
    pub fn connect(
        mut self,
        source: NodeId,
        target: NodeId,
        condition: Option<EdgeCondition>,
    ) -> Self {
        self.add_connection(source, target, condition);
        self
    }

    /// Alias for [`build`](Self::build).
    pub fn into_graph(self) -> RouteGraph {
        self.build()
    }
}

impl Default for RouteGraphBuilder {
    fn default() -> Self {
        Self::new()
    }
}
