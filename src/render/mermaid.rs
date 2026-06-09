//! Mermaid diagram renderer for RouteGraph.
//!
//! Produces a [Mermaid](https://mermaid.js.org/) flowchart that can be
//! embedded in Markdown, GitHub README, or any Mermaid-compatible viewer.
//!
//! # Example
//!
//! ```rust
//! use routegraph::prelude::*;
//!
//! let mut b = RouteGraph::builder();
//! b.push_node(NodeKind::Client, "Client");
//! b.push_node(NodeKind::Listener, ":443");
//! b.push_node(NodeKind::Host, "example.com");
//! b.push_node(NodeKind::Backend, "http://backend:8080");
//! let graph = b.build();
//!
//! let renderer = MermaidRenderer::new();
//! let mermaid = renderer.render(&graph);
//! assert!(mermaid.contains("graph TD"));
//! ```

use crate::{NodeKind, Renderer, RouteGraph};

/// Renders a [`RouteGraph`] as a Mermaid flowchart.
pub struct MermaidRenderer {
    title: String,
}

impl MermaidRenderer {
    /// Creates a new Mermaid renderer.
    pub fn new() -> Self {
        Self {
            title: "RouteGraph".to_string(),
        }
    }

    /// Sets the graph title.
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }
}

impl Default for MermaidRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl Renderer for MermaidRenderer {
    fn format_name(&self) -> &str {
        "mermaid"
    }

    fn render(&self, graph: &RouteGraph) -> String {
        let mut out = String::with_capacity(1024);

        out.push_str("%% ");
        out.push_str(&self.title);
        out.push('\n');
        out.push_str("graph TD\n");

        // Nodes
        for node in graph.nodes() {
            out.push_str("    ");
            out.push_str(&node.id.to_string());
            let shape = node_shape(&node.kind);
            let label = escape_mermaid(&node.label);
            out.push_str(&format_node(&shape, &label));
            out.push('\n');
        }

        out.push('\n');

        // Edges
        for edge in graph.edges() {
            out.push_str("    ");
            out.push_str(&edge.source.to_string());
            out.push_str(" -->");

            if let Some(ref cond) = edge.condition {
                let lbl = cond.to_string();
                if !lbl.is_empty() {
                    out.push('|');
                    out.push_str(&escape_mermaid(&lbl));
                    out.push('|');
                }
            }

            out.push(' ');
            out.push_str(&edge.target.to_string());
            out.push('\n');
        }

        // Style classes
        out.push_str("\nclassDef client fill:#4A90D9,color:#fff\n");
        out.push_str("classDef listener fill:#7B68EE,color:#fff\n");
        out.push_str("classDef host fill:#5B9BD5,color:#fff\n");
        out.push_str("classDef path fill:#FFA500,color:#000\n");
        out.push_str("classDef middleware fill:#FFD700,color:#000\n");
        out.push_str("classDef backend fill:#2ECC71,color:#fff\n");

        for node in graph.nodes() {
            let class = match &node.kind {
                NodeKind::Client => "client",
                NodeKind::Listener => "listener",
                NodeKind::Host => "host",
                NodeKind::PathMatch => "path",
                NodeKind::Middleware => "middleware",
                NodeKind::Backend => "backend",
            };
            out.push_str("class ");
            out.push_str(&node.id.to_string());
            out.push(' ');
            out.push_str(class);
            out.push('\n');
        }

        out
    }
}

enum MermaidShape {
    Rounded,
    Stadium,
    Diamond,
    Cylinder,
}

fn node_shape(kind: &NodeKind) -> MermaidShape {
    match kind {
        NodeKind::Client => MermaidShape::Stadium,
        NodeKind::Middleware => MermaidShape::Diamond,
        NodeKind::Backend => MermaidShape::Cylinder,
        _ => MermaidShape::Rounded,
    }
}

fn format_node(shape: &MermaidShape, label: &str) -> String {
    match shape {
        MermaidShape::Rounded => format!("[\"{label}\"]"),
        MermaidShape::Stadium => format!("(\"{label}\")"),
        MermaidShape::Diamond => format!("{{\"{label}\"}}"),
        MermaidShape::Cylinder => format!("[(\"{label}\")]"),
    }
}

fn escape_mermaid(s: &str) -> String {
    s.replace('"', "&quot;")
        .replace('{', "&#123;")
        .replace('}', "&#125;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NodeKind;

    fn build_simple_graph() -> RouteGraph {
        let mut b = RouteGraph::builder();
        b.push_node(NodeKind::Client, "Client");
        b.push_node(NodeKind::Listener, ":443");
        b.push_node(NodeKind::Host, "example.com");
        b.push_node(NodeKind::Backend, "http://backend:8080");
        b.build()
    }

    #[test]
    fn test_basic_graph() {
        let graph = build_simple_graph();
        let renderer = MermaidRenderer::new();
        let mermaid = renderer.render(&graph);

        assert!(mermaid.contains("graph TD"));
        assert!(mermaid.contains("n0 --> n1"));
        assert!(mermaid.contains("example.com"));
        assert!(mermaid.contains("http://backend:8080"));
        assert!(mermaid.contains("classDef backend"));
    }

    #[test]
    fn test_custom_title() {
        let mut b = RouteGraph::builder();
        b.push_node(NodeKind::Client, "Client");
        let graph = b.build();

        let renderer = MermaidRenderer::new().with_title("MyRoutes");
        let mermaid = renderer.render(&graph);
        assert!(mermaid.starts_with("%% MyRoutes"));
    }

    #[test]
    fn test_middleware_diamond_shape() {
        let mut b = RouteGraph::builder();
        b.push_node(NodeKind::Client, "Client");
        b.push_node(NodeKind::Listener, ":80");
        b.push_node(NodeKind::Middleware, "auth");
        b.push_node(NodeKind::Backend, "be:8080");
        let graph = b.build();

        let renderer = MermaidRenderer::new();
        let mermaid = renderer.render(&graph);
        assert!(mermaid.contains("{\"auth\"}"));
    }
}
