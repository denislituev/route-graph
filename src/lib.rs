//! RouteGraph — visualize reverse proxy routing configurations as directed graphs.
//!
//! # Quick start
//!
//! ```rust
//! use routegraph::prelude::*;
//!
//! let mut b = RouteGraph::builder();
//! b.push_node(NodeKind::Client, "Client");
//! b.push_node(NodeKind::Listener, ":443");
//! b.push_node(NodeKind::Host, "example.com");
//! b.push_node(NodeKind::PathMatch, "/api/*");
//! b.push_node(NodeKind::Backend, "http://backend:8080");
//! let graph = b.build();
//!
//! assert_eq!(graph.node_count(), 5);
//! assert!(graph.validate().is_ok());
//! ```

pub mod builder;
pub mod error;
pub mod model;
pub mod parse;
pub mod render;
pub mod traits;

pub use builder::RouteGraphBuilder;
pub use error::{ParseError, SourceLocation};
pub use model::{
    Edge, EdgeCondition, Metadata, Node, NodeId, NodeKind, Protocol, RouteGraph, TlsConfig,
    ValidationError,
};
pub use traits::{DetectionConfidence, FormatDetector, Parser, Renderer};

/// Common imports for working with the routing graph.
pub mod prelude {
    pub use crate::builder::RouteGraphBuilder;
    pub use crate::error::ParseError;
    pub use crate::model::{
        Edge, EdgeCondition, Metadata, Node, NodeId, NodeKind, Protocol, RouteGraph, TlsConfig,
        ValidationError,
    };
    pub use crate::parse::CaddyParser;
    pub use crate::render::{DotRenderer, MermaidRenderer};
    pub use crate::traits::{DetectionConfidence, FormatDetector, Parser, Renderer};
}
