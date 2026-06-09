//! RouteGraph CLI — visualize reverse proxy routing configurations.
//!
//! # Commands
//!
//! - `routegraph parse <FILE>` — parse a config and output a summary
//! - `routegraph render <FILE>` — render as DOT or Mermaid

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use routegraph::parse::CaddyParser;
use routegraph::render::{DotRenderer, MermaidRenderer};
use routegraph::{
    DetectionConfidence, FormatDetector as _, NodeId, NodeKind, Parser as _, Renderer as _,
    RouteGraph,
};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "routegraph",
    version,
    about = "Visualize reverse proxy routing configurations as graphs",
    long_about = "RouteGraph reads reverse proxy configurations and produces\n\
                  visual representations of request routing as graphs.\n\n\
                  Supports: Caddyfile (MVP)"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Parse a configuration file and output a summary.
    Parse {
        /// Path to the configuration file, or "-" for stdin.
        file: PathBuf,

        /// Input format [caddy|auto].
        #[arg(short, long, default_value = "auto")]
        format: String,
    },
    /// Render the routing graph in a specific output format.
    Render {
        /// Path to the configuration file, or "-" for stdin.
        file: PathBuf,

        /// Output format [dot|mermaid].
        #[arg(short, long, default_value = "mermaid")]
        renderer: String,

        /// Input format [caddy|auto].
        #[arg(short = 'f', long, default_value = "auto")]
        format: String,

        /// Graph title.
        #[arg(long)]
        title: Option<String>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Parse { file, format } => {
            let input = read_input(&file)?;
            let graph = parse_config(&input, &format)?;
            print_summary(&graph);
        }
        Command::Render {
            file,
            renderer,
            format,
            title,
        } => {
            let input = read_input(&file)?;
            let graph = parse_config(&input, &format)?;

            let output = match renderer.as_str() {
                "dot" => {
                    let mut r = DotRenderer::new();
                    if let Some(t) = title {
                        r = r.with_title(t);
                    }
                    r.render(&graph)
                }
                "mermaid" => {
                    let mut r = MermaidRenderer::new();
                    if let Some(t) = title {
                        r = r.with_title(t);
                    }
                    r.render(&graph)
                }
                other => anyhow::bail!("Unknown renderer: {other}. Supported: dot, mermaid"),
            };
            println!("{output}");
        }
    }

    Ok(())
}

fn read_input(path: &PathBuf) -> Result<String> {
    if path.to_str() == Some("-") {
        use std::io::Read;
        let mut buf = String::new();
        std::io::stdin()
            .read_to_string(&mut buf)
            .context("Failed to read from stdin")?;
        Ok(buf)
    } else {
        std::fs::read_to_string(path).with_context(|| format!("Failed to read {}", path.display()))
    }
}

fn parse_config(input: &str, format: &str) -> Result<RouteGraph> {
    let resolved_format = if format == "auto" {
        detect_format(input)?
    } else {
        format.to_string()
    };

    match resolved_format.as_str() {
        "caddy" => {
            let parser = CaddyParser::new();
            parser
                .parse(input)
                .map_err(|e| anyhow::anyhow!("{e}"))
                .context("Caddyfile parse error")
        }
        _ => anyhow::bail!("Unsupported format: {resolved_format}. Supported: caddy"),
    }
}

fn detect_format(input: &str) -> Result<String> {
    let parser = CaddyParser::new();
    let confidence = parser.detect(input);
    if confidence >= DetectionConfidence::Likely {
        return Ok("caddy".to_string());
    }
    anyhow::bail!(
        "Could not auto-detect config format. \
         Use --format to specify: caddy"
    )
}

fn print_summary(graph: &RouteGraph) {
    println!("RouteGraph Summary");
    println!("==================");
    println!("Nodes: {}", graph.node_count());
    println!("Edges: {}", graph.edge_count());

    if let Err(errors) = graph.validate() {
        println!();
        println!("Validation errors:");
        for err in &errors {
            println!("  - {err}");
        }
    }

    println!();

    let mut counts = std::collections::HashMap::new();
    for node in graph.nodes() {
        *counts.entry(&node.kind).or_insert(0usize) += 1;
    }

    let order: &[NodeKind] = &[
        NodeKind::Client,
        NodeKind::Listener,
        NodeKind::Host,
        NodeKind::PathMatch,
        NodeKind::Middleware,
        NodeKind::Backend,
    ];

    for kind in order {
        if let Some(count) = counts.get(kind) {
            println!("  {kind}: {count}");
        }
    }

    println!();

    if let Some(root) = graph.root() {
        print_tree(graph, root.id, 0);
    }
}

fn print_tree(graph: &RouteGraph, node_id: NodeId, depth: usize) {
    let Some(node) = graph.get_node(node_id) else {
        return;
    };
    let indent = "  ".repeat(depth);
    let arrow = if depth == 0 { "" } else { "└─ " };
    println!("{indent}{arrow}{} [{}]", node.label, node.kind);

    let child_ids: Vec<_> = graph.children_ids(node_id).collect();
    for id in child_ids {
        print_tree(graph, id, depth + 1);
    }
}
