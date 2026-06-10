# RouteGraph — Architecture Document

## 1. Project Structure

Single crate with a library + binary:

```
route-graph/
├── Cargo.toml
├── src/
│   ├── lib.rs           # Re-exports, prelude
│   ├── main.rs          # CLI binary
│   ├── model.rs         # RouteGraph, Node, Edge, NodeId, Metadata
│   ├── builder.rs       # RouteGraphBuilder (stack-based)
│   ├── error.rs         # ParseError, SourceLocation
│   ├── traits.rs        # Parser, Renderer, FormatDetector
│   ├── parse/
│   │   ├── mod.rs       # Re-exports CaddyParser
│   │   └── caddy.rs     # Caddyfile lexer, AST, parser
│   └── render/
│       ├── mod.rs       # Re-exports DotRenderer, MermaidRenderer
│       ├── dot.rs       # Graphviz DOT output
│       └── mermaid.rs   # Mermaid flowchart output
├── tests/
│   └── fixtures/
│       └── caddy/
│           └── example.Caddyfile
├── .cargo/
│   ├── config.toml      # git-fetch-with-cli
│   ├── deny.toml        # License/source/advisory checks
│   └── audit.toml       # cargo-audit config
├── .github/workflows/
│   ├── ci.yml           # Push to main: lint, test, build, security, docs
│   ├── pr-check.yml     # PR: fmt, clippy, test, audit, deny
│   └── release.yml      # Tag v*.*.*: multi-platform build + crates.io + GitHub Release
├── clippy.toml
├── CHANGELOG.md
├── ARCHITECTURE.md
├── README.md
├── LICENSE-MIT
└── LICENSE-APACHE
```

## 2. Cargo.toml

```toml
[package]
name = "routegraph"
version = "0.1.0"
edition = "2021"
license = "MIT OR Apache-2.0"
repository = "https://github.com/dzany/route-graph"
rust-version = "1.75"

[[bin]]
name = "routegraph"
path = "src/main.rs"

[dependencies]
thiserror = "2"
serde = { version = "1", features = ["derive"], optional = true }
clap = { version = "4", features = ["derive"] }
anyhow = "1"
serde_json = "1"

[features]
default = []
serde = ["dep:serde"]
```

## 3. Data Model (`model.rs`)

### Request flow through the graph

```
Client → Listener → Host → PathMatch → [Middleware]* → Backend
```

### Core types

```rust
pub struct NodeId(u32);  // Copy, usable as Vec index

#[non_exhaustive]
pub enum NodeKind {
    Client, Listener, Host, PathMatch, Middleware, Backend,
}

pub struct Node {
    pub id: NodeId,
    pub kind: NodeKind,
    pub label: String,
    pub metadata: Metadata,
}

pub struct Edge {
    pub source: NodeId,
    pub target: NodeId,
    pub condition: Option<EdgeCondition>,
}
```

### RouteGraph

The central data structure. Stores nodes and edges in flat `Vec`s with precomputed adjacency indices for O(1) traversal:

```rust
pub struct RouteGraph {
    nodes: Vec<Node>,
    edges: Vec<Edge>,
    root_id: Option<NodeId>,
    children_of: Vec<Vec<usize>>,  // adjacency: node → edge indices
    parents_of: Vec<Vec<usize>>,   // reverse adjacency: node → edge indices
}
```

| Operation | Complexity |
|-----------|-----------|
| `root()` | O(1) via cached `root_id` |
| `get_node(id)` | O(1) via Vec index |
| `children_ids(id)` | O(k) where k = out-degree, no allocation |
| `edges_from(id)` | O(k) where k = out-degree |
| `validate()` | O(N + E) |

### Metadata

```rust
#[non_exhaustive]
pub struct Metadata {
    pub port: Option<u16>,
    pub protocol: Option<Protocol>,
    pub tls: Option<TlsConfig>,
    pub custom: Vec<(String, String)>,
}
```

### EdgeCondition

```rust
#[non_exhaustive]
pub enum EdgeCondition {
    Always,
    PathGlob(String),       // /api/*
    PathPrefix(String),     // /api/
    PathExact(String),      // /api/users
    HeaderMatch { name, value },
    Method(String),         // GET, POST
}
```

## 4. Builder (`builder.rs`)

Stack-based builder. `push_node` creates a child of the current context, `pop_node` returns to the parent:

```rust
let mut b = RouteGraph::builder();
b.push_node(NodeKind::Client, "Client");
b.push_node(NodeKind::Listener, ":443");
b.set_port(443);
b.push_node(NodeKind::Host, "example.com");
b.push_node(NodeKind::PathMatch, "/api/*");
b.set_condition(EdgeCondition::PathGlob("/api/*".into()));
b.push_node(NodeKind::Backend, "http://backend:8080");
b.pop_node();
b.pop_node();
b.pop_node();
let graph = b.build();
```

Key design decisions:
- `&mut` methods are primary (`push_node`, `pop_node`, `set_port`, etc.)
- Consuming wrappers (`push`, `pop`, `with_port`) delegate to `&mut` variants
- `last_edge_idx` tracked for O(1) `set_condition` after `push_node`
- All mutation goes through `RouteGraph::add_node` / `add_edge` (pub(crate))

## 5. Traits (`traits.rs`)

### Parser

```rust
pub trait Parser {
    fn format_name(&self) -> &str;
    fn parse(&self, input: &str) -> Result<RouteGraph, ParseError>;
    fn parse_file(&self, path: &Path) -> Result<RouteGraph, ParseError>;  // default impl
}
```

### FormatDetector

```rust
pub trait FormatDetector {
    fn detect(&self, input: &str) -> DetectionConfidence;
}

#[non_exhaustive]
pub enum DetectionConfidence {
    None = 0, Maybe = 1, Likely = 2, Certain = 3,
}
```

### Renderer

```rust
pub trait Renderer {
    fn format_name(&self) -> &str;
    fn render(&self, graph: &RouteGraph) -> String;
}
```

Returns `String` (not `Result`) — rendering never fails for a valid graph.

## 6. Error Handling (`error.rs`)

```rust
pub enum ParseError {
    Syntax { message: String, line: u32 },
    Semantics { message: String },
    Io { source: io::Error, path: String },
    Unsupported(String),
}
```

- `ParseError::syntax(msg, line)` — convenience constructor
- `ParseError::io(source, path)` — wraps I/O errors with path context
- Implements `std::error::Error` with `source()` for the `Io` variant

## 7. Parsers (`parse/`)

### Caddyfile Parser (`parse/caddy.rs`)

Three-phase pipeline:

```
Input string → Tokenize → AST → RouteGraph
```

**Tokenizer** — `Lexer` iterates over input by byte position, zero allocation per character:
- `Token { kind: TokenKind, line: u32 }` — carries source location
- `TokenKind::Word(String) | OpenBrace | CloseBrace | Newline`
- Handles `#` comments, quoted strings, embedded `{placeholder}` braces

**AST** — intermediate representation:
```rust
struct SiteBlock { addresses: Vec<String>, directives: Vec<Directive> }

enum Directive {
    ReverseProxy { matcher, upstream },
    Handle { matcher, directives },
    HandlePath { path, directives },
    Rewrite { matcher, to },
    Redir { matcher, to, code },
    Other { name, args },
}
```

**Graph construction** — walks AST and drives `RouteGraphBuilder`:
- Single `Client` root node
- Each site address becomes `Listener` (+ optional `Host`)
- `reverse_proxy` → `Backend` (with optional `PathMatch`)
- `handle` / `handle_path` → `PathMatch` with sub-directives
- `rewrite` / `redir` → `Middleware`
- Known middleware directives (`encode`, `log`, `header`, etc.) → `Middleware`
- Global options block `{ ... }` skipped
- Multi-address sites (`example.com, www.example.com`) split into separate listener trees
- Port 443 / `https://` → auto-detect HTTPS + TLS

**FormatDetector** — heuristic based on:
- Known Caddy directives (`reverse_proxy`, `file_server`, `handle_path`, `encode`)
- Block pattern (` {\n`)
- Global block start (`{`)

### Adding a new parser

1. Create `src/parse/<format>.rs`
2. Implement `Parser` and `FormatDetector` traits
3. Add `pub mod <format>;` and `pub use` to `src/parse/mod.rs`
4. Register in CLI (`src/main.rs`) for format detection and parsing

## 8. Renderers (`render/`)

### DOT (`render/dot.rs`)

`DotRenderer` produces Graphviz DOT:

```rust
let renderer = DotRenderer::new().with_title("MyRoutes");
let dot = renderer.render(&graph);
// digraph MyRoutes { rankdir=TB; ... }
```

- Node styling: colors by `NodeKind`, shapes (ellipse/diamond/cylinder/box)
- Edge labels from `EdgeCondition`
- Port shown as tooltip

### Mermaid (`render/mermaid.rs`)

`MermaidRenderer` produces Mermaid flowcharts:

```rust
let renderer = MermaidRenderer::new();
let mermaid = renderer.render(&graph);
// %% RouteGraph\ngraph TD\n...
```

- Node shapes: stadium (Client), diamond (Middleware), cylinder (Backend), rounded (others)
- CSS class definitions for color coding
- Edge labels in `|...|` syntax

### Adding a new renderer

1. Create `src/render/<format>.rs`
2. Implement `Renderer` trait
3. Add `pub mod <format>;` and `pub use` to `src/render/mod.rs`
4. Register in CLI `match renderer.as_str()` block

## 9. CLI (`main.rs`)

```
routegraph parse <FILE> [--format auto|caddy]
routegraph render <FILE> --renderer dot|mermaid [--format auto|caddy] [--title TITLE]
```

- `--format auto` (default) runs `FormatDetector` on input
- Accepts `-` for stdin
- `parse` prints summary: node/edge counts, breakdown by kind, tree view
- `render` outputs formatted graph to stdout
- Uses `anyhow` for error reporting

## 10. Public API Summary

### Root (`routegraph::*`)

| Type | Description |
|------|-------------|
| `RouteGraph` | The routing graph data structure |
| `Node` | A node (id, kind, label, metadata) |
| `Edge` | A directed edge (source, target, condition) |
| `NodeId` | `u32` newtype, `Copy` |
| `NodeKind` | Client, Listener, Host, PathMatch, Middleware, Backend |
| `Metadata` | port, protocol, tls, custom key-values |
| `Protocol` | Http, Https, Grpc, Tcp, Udp |
| `TlsConfig` | auto, cert_path, sni |
| `EdgeCondition` | Always, PathGlob, PathPrefix, PathExact, HeaderMatch, Method |
| `RouteGraphBuilder` | Stack-based builder |
| `ParseError` | Syntax, Semantics, Io, Unsupported |
| `SourceLocation` | line, column, file |
| `ValidationError` | message |
| `Parser` | Trait: parse config string → RouteGraph |
| `Renderer` | Trait: RouteGraph → String |
| `FormatDetector` | Trait: detect config format |
| `DetectionConfidence` | None, Maybe, Likely, Certain |

### `routegraph::parse::*`

| Type | Description |
|------|-------------|
| `CaddyParser` | Caddyfile parser + format detector |

### `routegraph::render::*`

| Type | Description |
|------|-------------|
| `DotRenderer` | Graphviz DOT output |
| `MermaidRenderer` | Mermaid flowchart output |

### `routegraph::prelude::*`

All of the above in one import.

## 11. CI/CD

| Workflow | Trigger | Jobs |
|----------|---------|------|
| `ci.yml` | Push to main/master | lint (fmt + clippy), test, build release, security (deny + audit), docs |
| `pr-check.yml` | PR to main/master | fmt, clippy, test, cargo-audit, cargo-deny licenses |
| `release.yml` | Tag `v*.*.*` | 5 platform builds, `cargo publish` to crates.io, GitHub Release with SHA256 |

Release process:
```bash
git tag v0.1.0
git push origin main v0.1.0
```

## 12. Roadmap

### v0.2 — More Parsers
- Nginx parser
- Tiny Proxy parser
- JSON renderer

### v0.3 — Kubernetes
- Kubernetes Ingress / Gateway API parsers
- Multi-file input (directory of manifests)

### v0.4 — Visualization
- SVG/PNG renderer via Graphviz or resvg
- `RenderOptions` (layout direction, color scheme, metadata visibility)

### v0.5 — Analysis
- Diff between two configurations
- Dead route detection (unreachable backends)
- Dead route detection (unreachable backends)

### v0.6 — Integrations
- Traefik, Envoy, HAProxy parsers
- Streaming renderers (`impl Write`)
- Feature-gated parsers
