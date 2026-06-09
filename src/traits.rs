//! Traits for extending RouteGraph with parsers and renderers.

use std::path::Path;

use crate::error::ParseError;
use crate::model::RouteGraph;

/// Confidence level for auto-detection of config format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[non_exhaustive]
pub enum DetectionConfidence {
    /// Definitely not this format.
    None = 0,
    /// Might be this format but not sure.
    Maybe = 1,
    /// Likely this format.
    Likely = 2,
    /// Certain this is the format.
    Certain = 3,
}

/// A parser that reads a proxy configuration format and produces a [`RouteGraph`].
///
/// # Implementing
///
/// Implementors only need to provide [`format_name`](Self::format_name)
/// and [`parse`](Self::parse). The [`parse_file`](Self::parse_file) method
/// has a default implementation that reads the file and delegates to `parse`.
pub trait Parser {
    /// Returns the name of the format this parser handles (e.g. `"caddy"`).
    fn format_name(&self) -> &str;

    /// Parses a configuration string into a routing graph.
    fn parse(&self, input: &str) -> Result<RouteGraph, ParseError>;

    /// Convenience: reads a file and parses it.
    fn parse_file(&self, path: &Path) -> Result<RouteGraph, ParseError> {
        let content = std::fs::read_to_string(path).map_err(|e| ParseError::io(e, path))?;
        self.parse(&content)
    }
}

/// Auto-detection of configuration format.
///
/// Implementors provide a heuristic for recognizing whether a given
/// input looks like their format.
pub trait FormatDetector {
    /// Returns a confidence level for whether `input` is this format.
    fn detect(&self, input: &str) -> DetectionConfidence;
}

/// A renderer that produces a visual representation of a [`RouteGraph`].
///
/// Renderers receive a validated graph and always succeed (no error type).
/// If the graph is invalid, call [`RouteGraph::validate`] before rendering.
pub trait Renderer {
    /// Returns the name of the output format (e.g. `"dot"`, `"mermaid"`).
    fn format_name(&self) -> &str;

    /// Renders the graph to a string.
    fn render(&self, graph: &RouteGraph) -> String;
}
