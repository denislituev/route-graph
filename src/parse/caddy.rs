//! Caddyfile parser for RouteGraph.
//!
//! Parses Caddy's native configuration format into a [`RouteGraph`].
//!
//! # Supported directives
//!
//! - Site blocks with addresses (listeners/hosts)
//! - `reverse_proxy` — backend upstreams
//! - `handle` / `handle_path` — path-based routing
//! - `rewrite` — URL rewriting (middleware)
//! - `redir` — redirects (middleware)
//!
//! # Example
//!
//! ```rust
//! use routegraph::parse::CaddyParser;
//! use routegraph::Parser;
//!
//! let config = r#"
//! example.com {
//!     handle /api/* {
//!         reverse_proxy localhost:8080
//!     }
//! }
//! "#;
//!
//! let parser = CaddyParser::new();
//! let graph = parser.parse(config).unwrap();
//! assert!(graph.root().is_some());
//! ```

use crate::{
    DetectionConfidence, EdgeCondition, FormatDetector, NodeKind, ParseError, Parser, Protocol,
    RouteGraph, RouteGraphBuilder,
};

/// Parser for Caddyfile configuration format.
pub struct CaddyParser;

impl CaddyParser {
    /// Creates a new Caddyfile parser.
    pub fn new() -> Self {
        Self
    }
}

impl Default for CaddyParser {
    fn default() -> Self {
        Self::new()
    }
}

impl Parser for CaddyParser {
    fn format_name(&self) -> &str {
        "caddy"
    }

    fn parse(&self, input: &str) -> Result<RouteGraph, ParseError> {
        let tokens = tokenize(input)?;
        let ast = parse_tokens(&tokens)?;
        let graph = build_graph(&ast)?;
        Ok(graph)
    }
}

impl FormatDetector for CaddyParser {
    fn detect(&self, input: &str) -> DetectionConfidence {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return DetectionConfidence::None;
        }

        let has_caddy_directives = trimmed.contains("reverse_proxy")
            || trimmed.contains("file_server")
            || trimmed.contains("handle_path")
            || trimmed.contains("encode");

        let has_block_pattern = trimmed.contains(" {\n") || trimmed.contains(" {\r\n");
        let starts_with_global = trimmed.starts_with('{');

        if has_caddy_directives && has_block_pattern {
            return DetectionConfidence::Certain;
        }
        if has_caddy_directives {
            return DetectionConfidence::Likely;
        }
        if has_block_pattern || starts_with_global {
            return DetectionConfidence::Maybe;
        }
        DetectionConfidence::None
    }
}

// ---------------------------------------------------------------------------
// Tokenizer — zero-allocation iteration over input
// ---------------------------------------------------------------------------

/// A token from the Caddyfile, with source location.
#[derive(Debug, Clone, PartialEq)]
struct Token {
    kind: TokenKind,
    line: u32,
}

#[derive(Debug, Clone, PartialEq)]
enum TokenKind {
    /// A word (identifier, path, address, etc.).
    Word(String),
    /// Opening brace `{`.
    OpenBrace,
    /// Closing brace `}`.
    CloseBrace,
    /// A newline (significant in Caddyfile — separates directives).
    Newline,
}

/// Zero-allocation lexer over the input string.
struct Lexer<'a> {
    input: &'a str,
    byte_pos: usize,
    line: u32,
}

impl<'a> Lexer<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            input,
            byte_pos: 0,
            line: 1,
        }
    }

    fn is_eof(&self) -> bool {
        self.byte_pos >= self.input.len()
    }

    fn current_char(&self) -> Option<char> {
        if self.byte_pos >= self.input.len() {
            None
        } else {
            self.input[self.byte_pos..].chars().next()
        }
    }

    fn advance(&mut self) -> Option<char> {
        let ch = self.current_char()?;
        self.byte_pos += ch.len_utf8();
        if ch == '\n' {
            self.line += 1;
        }
        Some(ch)
    }

    /// Collect a word, including embedded `{placeholder}` braces.
    fn collect_word(&mut self) -> Option<String> {
        let start = self.byte_pos;
        while let Some(ch) = self.current_char() {
            match ch {
                ' ' | '\t' | '\n' | '\r' | '#' | '"' => break,
                '{' | '}' => {
                    // Standalone brace at word start → not part of this word.
                    if self.byte_pos == start {
                        break;
                    }
                    // Embedded brace → include and continue.
                    self.advance();
                }
                _ => {
                    self.advance();
                }
            }
        }
        if self.byte_pos > start {
            Some(self.input[start..self.byte_pos].to_string())
        } else {
            None
        }
    }

    fn collect_quoted_string(&mut self) -> Result<String, ParseError> {
        let mut result = String::new();
        while let Some(ch) = self.current_char() {
            match ch {
                '"' => {
                    self.advance(); // consume closing quote
                    return Ok(result);
                }
                '\\' => {
                    self.advance();
                    if let Some(escaped) = self.current_char() {
                        result.push(escaped);
                        self.advance();
                    }
                }
                _ => {
                    result.push(ch);
                    self.advance();
                }
            }
        }
        Err(ParseError::syntax("unterminated quoted string", self.line))
    }
}

fn tokenize(input: &str) -> Result<Vec<Token>, ParseError> {
    let mut lexer = Lexer::new(input);
    let mut tokens = Vec::new();

    while !lexer.is_eof() {
        let line = lexer.line;
        let ch = lexer.current_char().unwrap();

        match ch {
            '\n' => {
                lexer.advance();
                if tokens.last().map(|t: &Token| &t.kind) != Some(&TokenKind::Newline) {
                    tokens.push(Token {
                        kind: TokenKind::Newline,
                        line,
                    });
                }
            }
            '\r' => {
                lexer.advance();
            }
            ' ' | '\t' => {
                lexer.advance();
            }
            '#' => {
                // Comment: skip to end of line
                while let Some(c) = lexer.current_char() {
                    if c == '\n' {
                        break;
                    }
                    lexer.advance();
                }
            }
            '{' => {
                lexer.advance();
                tokens.push(Token {
                    kind: TokenKind::OpenBrace,
                    line,
                });
            }
            '}' => {
                lexer.advance();
                tokens.push(Token {
                    kind: TokenKind::CloseBrace,
                    line,
                });
            }
            '"' => {
                lexer.advance(); // consume opening quote
                let word = lexer.collect_quoted_string()?;
                tokens.push(Token {
                    kind: TokenKind::Word(word),
                    line,
                });
            }
            _ => {
                if let Some(word) = lexer.collect_word() {
                    tokens.push(Token {
                        kind: TokenKind::Word(word),
                        line,
                    });
                }
            }
        }
    }

    Ok(tokens)
}

// ---------------------------------------------------------------------------
// AST
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct SiteBlock {
    addresses: Vec<String>,
    directives: Vec<Directive>,
}

#[derive(Debug, Clone)]
enum Directive {
    ReverseProxy {
        matcher: Option<String>,
        upstream: String,
    },
    Handle {
        matcher: Option<String>,
        directives: Vec<Directive>,
    },
    HandlePath {
        path: String,
        directives: Vec<Directive>,
    },
    Rewrite {
        matcher: Option<String>,
        to: String,
    },
    Redir {
        matcher: Option<String>,
        to: String,
        code: Option<String>,
    },
    Other {
        name: String,
        args: Vec<String>,
    },
}

// ---------------------------------------------------------------------------
// Token parser → AST
// ---------------------------------------------------------------------------

struct TokenStream<'a> {
    tokens: &'a [Token],
    pos: usize,
}

impl<'a> TokenStream<'a> {
    fn new(tokens: &'a [Token]) -> Self {
        Self { tokens, pos: 0 }
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn skip_newlines(&mut self) {
        while let Some(t) = self.peek() {
            if t.kind == TokenKind::Newline {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    /// Collect words until `{`, `}`, or newline.
    fn collect_words(&mut self) -> Vec<String> {
        let mut words = Vec::new();
        while let Some(t) = self.peek() {
            if let TokenKind::Word(w) = &t.kind {
                words.push(w.clone());
                self.pos += 1;
            } else {
                break;
            }
        }
        words
    }

    fn parse_block_contents(&mut self) -> Vec<Directive> {
        let mut directives = Vec::new();
        self.skip_newlines();

        loop {
            self.skip_newlines();
            match self.peek() {
                None
                | Some(Token {
                    kind: TokenKind::CloseBrace,
                    ..
                }) => break,
                Some(Token {
                    kind: TokenKind::Word(name),
                    ..
                }) => {
                    let name = name.clone();
                    self.pos += 1;
                    directives.push(self.parse_directive(name));
                }
                Some(Token {
                    kind: TokenKind::OpenBrace,
                    ..
                }) => {
                    self.pos += 1;
                }
                Some(Token {
                    kind: TokenKind::Newline,
                    ..
                }) => {
                    self.pos += 1;
                }
            }
        }
        directives
    }

    fn parse_directive(&mut self, name: String) -> Directive {
        let args = self.collect_words();

        match name.as_str() {
            "reverse_proxy" => {
                let (matcher, upstream) = if args.len() >= 2 {
                    (Some(args[0].clone()), args[1].clone())
                } else if args.len() == 1 {
                    (None, args[0].clone())
                } else {
                    (None, String::new())
                };
                self.skip_newlines();
                Directive::ReverseProxy { matcher, upstream }
            }
            "handle" => {
                let matcher = args.first().cloned();
                self.skip_newlines();
                let sub = if self.peek().map(|t| &t.kind) == Some(&TokenKind::OpenBrace) {
                    self.pos += 1;
                    let dirs = self.parse_block_contents();
                    if self.peek().map(|t| &t.kind) == Some(&TokenKind::CloseBrace) {
                        self.pos += 1;
                    }
                    self.skip_newlines();
                    dirs
                } else {
                    Vec::new()
                };
                Directive::Handle {
                    matcher,
                    directives: sub,
                }
            }
            "handle_path" => {
                let path = args.first().cloned().unwrap_or_default();
                self.skip_newlines();
                let sub = if self.peek().map(|t| &t.kind) == Some(&TokenKind::OpenBrace) {
                    self.pos += 1;
                    let dirs = self.parse_block_contents();
                    if self.peek().map(|t| &t.kind) == Some(&TokenKind::CloseBrace) {
                        self.pos += 1;
                    }
                    self.skip_newlines();
                    dirs
                } else {
                    Vec::new()
                };
                Directive::HandlePath {
                    path,
                    directives: sub,
                }
            }
            "rewrite" => {
                let (matcher, to) = if args.len() >= 2 {
                    (Some(args[0].clone()), args[1].clone())
                } else if args.len() == 1 {
                    (None, args[0].clone())
                } else {
                    (None, String::new())
                };
                Directive::Rewrite { matcher, to }
            }
            "redir" => {
                let (matcher, to, code) = if args.is_empty() {
                    (None, String::new(), None)
                } else if args.len() == 1 {
                    (None, args[0].clone(), None)
                } else if args.len() == 2 {
                    if looks_like_path(&args[0]) {
                        (Some(args[0].clone()), args[1].clone(), None)
                    } else {
                        (None, args[0].clone(), Some(args[1].clone()))
                    }
                } else {
                    (
                        Some(args[0].clone()),
                        args[1].clone(),
                        Some(args[2].clone()),
                    )
                };
                Directive::Redir { matcher, to, code }
            }
            _ => Directive::Other { name, args },
        }
    }
}

fn parse_tokens(tokens: &[Token]) -> Result<Vec<SiteBlock>, ParseError> {
    let mut stream = TokenStream::new(tokens);
    let mut site_blocks = Vec::new();

    // Skip global options block if present
    stream.skip_newlines();
    if stream.peek().map(|t| &t.kind) == Some(&TokenKind::OpenBrace) {
        stream.pos += 1;
        let mut depth = 1u32;
        while depth > 0 {
            match stream.peek() {
                Some(Token {
                    kind: TokenKind::OpenBrace,
                    ..
                }) => {
                    depth += 1;
                    stream.pos += 1;
                }
                Some(Token {
                    kind: TokenKind::CloseBrace,
                    ..
                }) => {
                    depth -= 1;
                    stream.pos += 1;
                }
                None => {
                    return Err(ParseError::syntax("unterminated global options block", 1));
                }
                _ => {
                    stream.pos += 1;
                }
            }
        }
        stream.skip_newlines();
    }

    loop {
        stream.skip_newlines();
        if stream.peek().is_none() {
            break;
        }

        let addresses = stream.collect_words();
        if addresses.is_empty() {
            break;
        }

        stream.skip_newlines();

        let directives = if stream.peek().map(|t| &t.kind) == Some(&TokenKind::OpenBrace) {
            stream.pos += 1;
            let dirs = stream.parse_block_contents();
            if stream.peek().map(|t| &t.kind) == Some(&TokenKind::CloseBrace) {
                stream.pos += 1;
            }
            dirs
        } else {
            Vec::new()
        };

        site_blocks.push(SiteBlock {
            addresses,
            directives,
        });
    }

    Ok(site_blocks)
}

// ---------------------------------------------------------------------------
// AST → RouteGraph
// ---------------------------------------------------------------------------

fn build_graph(site_blocks: &[SiteBlock]) -> Result<RouteGraph, ParseError> {
    let mut b = RouteGraphBuilder::new();

    // Single Client root
    b.push_node(NodeKind::Client, "Client");

    for block in site_blocks {
        for addr in &block.addresses {
            for single_addr in addr.split(',') {
                let single_addr = single_addr.trim();
                if single_addr.is_empty() {
                    continue;
                }
                build_site(&mut b, single_addr, &block.directives)?;
            }
        }
    }

    Ok(b.build())
}

fn build_site(
    b: &mut RouteGraphBuilder,
    addr: &str,
    directives: &[Directive],
) -> Result<(), ParseError> {
    let (listener_label, host_label, port) = parse_address(addr);

    b.push_node(NodeKind::Listener, &listener_label);
    if let Some(p) = port {
        b.set_port(p);
    }
    if port == Some(443) || addr.starts_with("https://") {
        b.set_protocol(Protocol::Https);
        b.set_tls_auto();
    } else {
        b.set_protocol(Protocol::Http);
    }

    if let Some(ref host) = host_label {
        b.push_node(NodeKind::Host, host);
    }

    process_directives(b, directives)?;

    if host_label.is_some() {
        b.pop_node();
    }
    b.pop_node();

    Ok(())
}

fn process_directives(
    b: &mut RouteGraphBuilder,
    directives: &[Directive],
) -> Result<(), ParseError> {
    for directive in directives {
        match directive {
            Directive::ReverseProxy { matcher, upstream } => {
                if let Some(ref m) = matcher {
                    if looks_like_path(m) {
                        b.push_node(NodeKind::PathMatch, m);
                        b.set_condition(EdgeCondition::PathGlob(m.clone()));
                        b.push_node(NodeKind::Backend, upstream);
                        b.pop_node();
                        b.pop_node();
                        continue;
                    }
                }
                b.push_node(NodeKind::Backend, upstream);
                b.pop_node();
            }
            Directive::Handle {
                matcher,
                directives: sub,
            } => {
                let path = matcher.as_deref().unwrap_or("*");
                b.push_node(NodeKind::PathMatch, path);
                if let Some(ref m) = matcher {
                    if looks_like_path(m) {
                        b.set_condition(EdgeCondition::PathGlob(m.clone()));
                    }
                }
                process_directives(b, sub)?;
                b.pop_node();
            }
            Directive::HandlePath {
                path,
                directives: sub,
            } => {
                b.push_node(NodeKind::PathMatch, path);
                b.set_condition(EdgeCondition::PathPrefix(path.clone()));
                b.push_node(NodeKind::Middleware, "strip_prefix");
                b.add_custom("prefix", path);
                process_directives(b, sub)?;
                b.pop_node();
                b.pop_node();
            }
            Directive::Rewrite { matcher, to } => {
                let label = format!("rewrite → {to}");
                if let Some(ref m) = matcher {
                    if looks_like_path(m) {
                        b.push_node(NodeKind::PathMatch, m);
                        b.push_node(NodeKind::Middleware, &label);
                        b.pop_node();
                        b.pop_node();
                        continue;
                    }
                }
                b.push_node(NodeKind::Middleware, &label);
                b.pop_node();
            }
            Directive::Redir { matcher, to, code } => {
                let code_str = code.as_deref().unwrap_or("302");
                let label = format!("redir → {to} [{code_str}]");
                if let Some(ref m) = matcher {
                    if looks_like_path(m) {
                        b.push_node(NodeKind::PathMatch, m);
                        b.push_node(NodeKind::Middleware, &label);
                        b.pop_node();
                        b.pop_node();
                        continue;
                    }
                }
                b.push_node(NodeKind::Middleware, &label);
                b.pop_node();
            }
            Directive::Other { name, args } => {
                if is_middleware_directive(name) {
                    let label = if args.is_empty() {
                        name.clone()
                    } else {
                        format!("{name} {}", args.join(" "))
                    };
                    b.push_node(NodeKind::Middleware, &label);
                    b.pop_node();
                }
            }
        }
    }
    Ok(())
}

fn parse_address(addr: &str) -> (String, Option<String>, Option<u16>) {
    let addr = addr.trim();

    if let Some(stripped) = addr.strip_prefix(':') {
        let port = stripped.parse::<u16>().ok();
        return (addr.to_string(), None, port);
    }

    if let Some(colon) = addr.rfind(':') {
        let host = &addr[..colon];
        let port_str = &addr[colon + 1..];
        if let Ok(port) = port_str.parse::<u16>() {
            let listener = format!(":{port}");
            return (listener, Some(host.to_string()), Some(port));
        }
    }

    (":80".to_string(), Some(addr.to_string()), Some(80))
}

fn looks_like_path(s: &str) -> bool {
    s.starts_with('/') || s.starts_with('*')
}

fn is_middleware_directive(name: &str) -> bool {
    matches!(
        name,
        "encode"
            | "decode"
            | "log"
            | "header"
            | "request_body"
            | "templates"
            | "cache"
            | "rate_limit"
            | "authentication"
            | "authorize"
            | "cors"
            | "compress"
            | "deflate"
            | "gzip"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Parser;

    #[test]
    fn test_empty_input() {
        let parser = CaddyParser::new();
        let graph = parser.parse("").unwrap();
        assert_eq!(graph.node_count(), 1);
    }

    #[test]
    fn test_simple_reverse_proxy() {
        let config = r#"
app.example.com {
    reverse_proxy localhost:3000
}
"#;
        let parser = CaddyParser::new();
        let graph = parser.parse(config).unwrap();

        let kinds: Vec<_> = graph.nodes().iter().map(|n| n.kind.clone()).collect();
        assert!(kinds.contains(&NodeKind::Client));
        assert!(kinds.contains(&NodeKind::Listener));
        assert!(kinds.contains(&NodeKind::Host));
        assert!(kinds.contains(&NodeKind::Backend));
    }

    #[test]
    fn test_handle_path() {
        let config = r#"
example.com {
    handle_path /api/* {
        reverse_proxy localhost:8080
    }
}
"#;
        let parser = CaddyParser::new();
        let graph = parser.parse(config).unwrap();

        let labels: Vec<_> = graph.nodes().iter().map(|n| n.label.as_str()).collect();
        assert!(labels.contains(&"/api/*"));
        assert!(labels.contains(&"strip_prefix"));
        assert!(labels.contains(&"localhost:8080"));
    }

    #[test]
    fn test_multiple_handles() {
        let config = r#"
example.com {
    handle /api/* {
        reverse_proxy localhost:8080
    }
    handle /static/* {
        reverse_proxy localhost:9000
    }
}
"#;
        let parser = CaddyParser::new();
        let graph = parser.parse(config).unwrap();

        let backends: Vec<_> = graph
            .nodes()
            .iter()
            .filter(|n| n.kind == NodeKind::Backend)
            .map(|n| n.label.as_str())
            .collect();
        assert!(backends.contains(&"localhost:8080"));
        assert!(backends.contains(&"localhost:9000"));
    }

    #[test]
    fn test_detect_caddy() {
        let parser = CaddyParser::new();
        assert_eq!(
            parser.detect("example.com {\n    reverse_proxy localhost:8080\n}"),
            DetectionConfidence::Certain
        );
        assert_eq!(parser.detect("hello world"), DetectionConfidence::None);
    }

    #[test]
    fn test_tls_detection() {
        let config = r#"
:443 {
    reverse_proxy localhost:8080
}
"#;
        let parser = CaddyParser::new();
        let graph = parser.parse(config).unwrap();

        let listener = graph
            .nodes()
            .iter()
            .find(|n| n.kind == NodeKind::Listener)
            .unwrap();
        assert_eq!(listener.metadata.protocol, Some(Protocol::Https));
        assert!(listener.metadata.tls.is_some());
    }

    #[test]
    fn test_global_options_skipped() {
        let config = r#"
{
    admin off
    email test@example.com
}

example.com {
    reverse_proxy localhost:3000
}
"#;
        let parser = CaddyParser::new();
        let graph = parser.parse(config).unwrap();
        assert!(graph.nodes().iter().any(|n| n.kind == NodeKind::Backend));
    }

    #[test]
    fn test_validation_passes() {
        let config = r#"
example.com {
    reverse_proxy localhost:3000
}
"#;
        let parser = CaddyParser::new();
        let graph = parser.parse(config).unwrap();
        assert!(graph.validate().is_ok());
    }
}
