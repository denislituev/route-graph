# Changelog

All notable changes to RouteGraph will be documented in this file.

## [Unreleased]

## [0.1.0] - 2024-06-09

### Added
- Initial release of RouteGraph
- Core routing graph model with:
  - `RouteGraph`, `Node`, `Edge` data structures
  - Precomputed adjacency indices for O(1) traversal
  - Graph validation
  - `RouteGraphBuilder` for constructing graphs
- Caddyfile parser supporting:
  - Site blocks with addresses (listeners/hosts)
  - `reverse_proxy` backend upstreams
  - `handle` / `handle_path` path-based routing
  - `rewrite` URL rewriting
  - `redir` redirects
  - Global options block
  - Multi-address sites (e.g., `example.com, www.example.com`)
  - Auto-HTTPS detection (port 443)
  - Middleware directive detection
- Renderers:
  - Graphviz DOT output
  - Mermaid flowchart output
- CLI tool:
  - `routegraph parse <FILE>` — parse and show summary
  - `routegraph render <FILE>` — render as DOT or Mermaid
  - Auto-detection of config format
  - Stdin support
  - Custom graph titles
- Format auto-detection via `FormatDetector` trait

### Infrastructure
- GitHub Actions CI/CD:
  - CI on push to main/master
  - PR checks (fmt, clippy, test, security)
  - Multi-platform release builds (linux-amd64/arm64, macos-amd64/arm64, windows)
  - Automated crates.io publishing
  - Cargo-deny license checking
  - Cargo-audit security scanning

### Documentation
- ARCHITECTURE.md with full design documentation
- Inline documentation and examples
- Test fixtures for real-world Caddyfile

[Unreleased]: https://github.com/dzany/route-graph/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/dzany/route-graph/releases/tag/v0.1.0