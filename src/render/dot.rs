//! Graphviz DOT renderer for RouteGraph.
//!
//! Produces a DOT-format graph that can be rendered with `dot`:
//!
//! ```bash
//! routegraph render -r dot config/Caddyfile | dot -Tpng -o routes.png
//! ```
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
//! let renderer = DotRenderer::new();
//! let dot = renderer.render(&graph);
//! assert!(dot.contains("digraph"));
//! ```

use crate::{NodeKind, Renderer, RouteGraph};

/// Renders a [`RouteGraph`] as a Graphviz DOT graph.
pub struct DotRenderer {
    title: String,
}

impl DotRenderer {
    /// Creates a new DOT renderer.
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

impl Default for DotRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl Renderer for DotRenderer {
    fn format_name(&self) -> &str {
        "dot"
    }

    fn render(&self, graph: &RouteGraph) -> String {
        let mut out = String::with_capacity(1024);

        out.push_str("digraph ");
        out.push_str(&self.title);
        out.push_str(" {\n");
        out.push_str("    rankdir=TB;\n");
        out.push_str("    node [shape=box, style=\"rounded,filled\"];\n");
        out.push_str("    edge [fontsize=10];\n\n");

        // Nodes
        for node in graph.nodes() {
            out.push_str("    ");
            out.push_str(&node.id.to_string());
            out.push('[');

            out.push_str("label=\"");
            out.push_str(&escape_dot(&node.label));
            out.push('"');

            let (fillcolor, fontcolor) = node_colors(&node.kind);
            out.push_str(", fillcolor=\"");
            out.push_str(fillcolor);
            out.push('"');
            if fontcolor != "black" {
                out.push_str(", fontcolor=\"");
                out.push_str(fontcolor);
                out.push('"');
            }

            let shape = node_shape(&node.kind);
            if shape != "box" {
                out.push_str(", shape=\"");
                out.push_str(shape);
                out.push('"');
            }

            if let Some(port) = node.metadata.port {
                out.push_str(", tooltip=\"port: ");
                out.push_str(&port.to_string());
                out.push('"');
            }

            out.push_str("];\n");
        }

        out.push('\n');

        // Edges
        for edge in graph.edges() {
            out.push_str("    ");
            out.push_str(&edge.source.to_string());
            out.push_str(" -> ");
            out.push_str(&edge.target.to_string());

            if let Some(ref cond) = edge.condition {
                let lbl = cond.to_string();
                if !lbl.is_empty() {
                    out.push_str(" [label=\"");
                    out.push_str(&escape_dot(&lbl));
                    out.push_str("\"]");
                }
            }

            out.push_str(";\n");
        }

        out.push_str("}\n");
        out
    }
}

fn node_colors(kind: &NodeKind) -> (&'static str, &'static str) {
    match kind {
        NodeKind::Client => ("#4A90D9", "white"),
        NodeKind::Listener => ("#7B68EE", "white"),
        NodeKind::Host => ("#5B9BD5", "white"),
        NodeKind::PathMatch => ("#FFA500", "black"),
        NodeKind::Middleware => ("#FFD700", "black"),
        NodeKind::Backend => ("#2ECC71", "white"),
    }
}

fn node_shape(kind: &NodeKind) -> &'static str {
    match kind {
        NodeKind::Client => "ellipse",
        NodeKind::Middleware => "diamond",
        NodeKind::Backend => "cylinder",
        NodeKind::Listener | NodeKind::Host | NodeKind::PathMatch => "box",
    }
}

fn escape_dot(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
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
    fn test_empty_graph() {
        let graph = RouteGraph::new();
        let renderer = DotRenderer::new();
        let result = renderer.render(&graph);
        assert!(result.contains("digraph"));
    }

    #[test]
    fn test_basic_graph() {
        let graph = build_simple_graph();
        let renderer = DotRenderer::new();
        let dot = renderer.render(&graph);

        assert!(dot.starts_with("digraph RouteGraph"));
        assert!(dot.contains("n0"));
        assert!(dot.contains("n0 -> n1"));
        assert!(dot.contains("example.com"));
        assert!(dot.contains("http://backend:8080"));
    }

    #[test]
    fn test_custom_title() {
        let mut b = RouteGraph::builder();
        b.push_node(NodeKind::Client, "Client");
        let graph = b.build();

        let renderer = DotRenderer::new().with_title("MyRoutes");
        let dot = renderer.render(&graph);
        assert!(dot.starts_with("digraph MyRoutes"));
    }
}
