# RouteGraph — Architecture Document

## 1. Workspace Structure

```
route-graph/
├── Cargo.toml                  # workspace root
├── LICENSE                     # MIT OR Apache-2.0
├── LICENSE-MIT
├── LICENSE-APACHE
├── README.md
├── ARCHITECTURE.md             # этот документ
├── deny.toml                   # cargo-deny config
│
├── crates/
│   ├── route-graph-core/       # модель данных графа, трейты
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── model.rs        # RouteGraph, Node, Edge, ...
│   │       ├── traits.rs       # Parser, Renderer
│   │       └── error.rs        # унифицированные ошибки
│   │
│   ├── route-graph-parser-caddy/    # парсер Caddyfile
│   │   ├── Cargo.toml
│   │   └── src/
│   │       └── lib.rs
│   │
│   ├── route-graph-parser-nginx/    # парсер Nginx
│   │   ├── Cargo.toml
│   │   └── src/
│   │       └── lib.rs
│   │
│   ├── route-graph-parser-tinyproxy/ # парсер Tiny Proxy
│   │   ├── Cargo.toml
│   │   └── src/
│   │       └── lib.rs
│   │
│   ├── route-graph-renderer-dot/    # Graphviz DOT
│   │   ├── Cargo.toml
│   │   └── src/
│   │       └── lib.rs
│   │
│   ├── route-graph-renderer-mermaid/ # Mermaid
│   │   ├── Cargo.toml
│   │   └── src/
│   │       └── lib.rs
│   │
│   ├── route-graph-renderer-json/   # JSON (для интеграции)
│   │   ├── Cargo.toml
│   │   └── src/
│   │       └── lib.rs
│   │
│   └── route-graph-cli/             # CLI-бинарь
│       ├── Cargo.toml
│       └── src/
│           └── main.rs
│
└── tests/                           # интеграционные тесты
    └── fixtures/
        ├── caddy/
        ├── nginx/
        └── tinyproxy/
```

## 2. Workspace `Cargo.toml`

```toml
[workspace]
resolver = "2"
members = [
    "crates/route-graph-core",
    "crates/route-graph-parser-caddy",
    "crates/route-graph-parser-nginx",
    "crates/route-graph-parser-tinyproxy",
    "crates/route-graph-renderer-dot",
    "crates/route-graph-renderer-mermaid",
    "crates/route-graph-renderer-json",
    "crates/route-graph-cli",
]

[workspace.package]
version = "0.1.0"
edition = "2021"
license = "MIT OR Apache-2.0"
repository = "https://github.com/dzany/route-graph"
rust-version = "1.75"

[workspace.dependencies]
route-graph-core = { path = "crates/route-graph-core" }
route-graph-parser-caddy = { path = "crates/route-graph-parser-caddy" }
route-graph-parser-nginx = { path = "crates/route-graph-parser-nginx" }
route-graph-parser-tinyproxy = { path = "crates/route-graph-parser-tinyproxy" }
route-graph-renderer-dot = { path = "crates/route-graph-renderer-dot" }
route-graph-renderer-mermaid = { path = "crates/route-graph-renderer-mermaid" }
route-graph-renderer-json = { path = "crates/route-graph-renderer-json" }

# shared
thiserror = "2"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
clap = { version = "4", features = ["derive"] }
compact_str = "0.9"
```

## 3. Crate: `route-graph-core`

Это сердце проекта. Содержит модель данных и трейты, которые имплементируют парсеры и рендереры.

### 3.1 Модель данных (`model.rs`)

```
┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│ RouteGraph  │────▶│   Node[]    │     │   Edge[]    │
│             │     │             │     │             │
│ nodes       │     │ id: NodeId  │     │ source      │
│ edges       │     │ kind        │     │ target      │
│ root_ids    │     │ label       │     │ condition   │
└─────────────┘     │ metadata    │     └──────┬──────┘
                    └──────┬──────┘            │
                           │                   │
              ┌────────────┼───────────┐       │
              ▼            ▼           ▼       ▼
         NodeKind     Metadata   Protocol  EdgeCondition
         ─────────    ────────   ────────  ─────────────
         Client       port       Http      Always
         Listener     protocol   Https     PathGlob
         Host         tls        Grpc      PathPrefix
         PathMatch    custom     Tcp       PathExact
         Middleware                         HeaderMatch
         Backend      TlsConfig  Udp       Method
                      ─────────
                      cert_path
                      auto
                      sni
```

**Ключевые дизайнерские решения:**

| Решение | Обоснование |
|---------|------------|
| `NodeId = u32` (newtype) | Индексы вместо `Rc` / `Arc` — нет аллокаций, быстрый lookup, `Copy` |
| `CompactString` вместо `String` | 24 байта inline (вмещает большинство лейблов), меньше аллокаций |
| `Vec<Node>` + `Vec<Edge>` | SlotMap-подобная структура, данные локальны в памяти, cache-friendly |
| `metadata.custom: Vec<(K,V)>` | Маленький vec для произвольных полей вместо `HashMap` — парсеров мало ключей |
| `EdgeCondition` — enum | Паттерн matching — конечное множество, расширяемо добавлением variant |
| `Protocol` / `TlsConfig` — отдельные типы | Строгая типизация, невозможно перепутать строку с портом |

### 3.2 Builder API

Граф строится через builder-паттерн, чтобы парсеры не работали с внутренними полями напрямую:

```rust
RouteGraph::builder()
    .add_listener(":443")
        .with_protocol(Protocol::Https)
        .with_tls_auto()
        .add_host("example.com")
            .add_path_match("/api/*")
                .add_middleware("strip_prefix")
                    .with_custom("path", "/api")
                    .done()
                .add_middleware("header_rewrite")
                    .done()
                .add_backend("http://backend:8080")
                .done()
            .done()
        .done()
    .build()
```

Builder возвращает `&mut ChildBuilder<...>` — типизированный курсор по уровню вложенности. Парсер не может добавить backend на уровень listener.

### 3.3 Трейты (`traits.rs`)

#### trait `Parser`

```rust
pub trait Parser {
    /// Имя формата: "caddy", "nginx", "tinyproxy"
    fn format_name(&self) -> &str;

    /// Парсинг из строки конфигурации.
    /// Принимает &str — нулевая аллокация на входе.
    fn parse(&self, input: &str) -> Result<RouteGraph, ParseError>;

    /// Парсинг из файла (convenience, делегирует в parse).
    fn parse_file(&self, path: &Path) -> Result<RouteGraph, ParseError>;
}
```

#### trait `FormatDetector`

```rust
pub trait FormatDetector {
    /// Быстрая эвристика — можно ли парсить этот файл данным парсером.
    /// Используется CLI для auto-detect.
    fn detect(&self, input: &str) -> DetectionConfidence;
}

pub enum DetectionConfidence {
    None,
    Maybe,
    Likely,
    Certain,
}
```

#### trait `Renderer`

```rust
pub trait Renderer {
    /// Имя выходного формата: "dot", "mermaid", "json"
    fn format_name(&self) -> &str;

    /// Рендеринг графа в строку.
    fn render(&self, graph: &RouteGraph) -> Result<String, RenderError>;

    /// Рендеринг с опциями (позволяет расширять без breaking change).
    fn render_with_options(
        &self,
        graph: &RouteGraph,
        options: &RenderOptions,
    ) -> Result<String, RenderError>;
}

pub struct RenderOptions {
    pub direction: LayoutDirection,
    pub include_metadata: bool,
    pub collapse_middleware: bool,
    pub color_scheme: ColorScheme,
    pub title: Option<String>,
}

pub enum LayoutDirection {
    TopToBottom,
    LeftToRight,
    BottomToTop,
    RightToLeft,
}

pub enum ColorScheme {
    Auto,
    Dark,
    Light,
    Plain,
}
```

### 3.4 Error handling (`error.rs`)

```rust
pub struct ParseError {
    pub kind: ParseErrorKind,
    pub message: String,
    pub location: Option<SourceLocation>,
}

pub enum ParseErrorKind {
    Syntax,
    Semantics,
    Io,
    UnsupportedFeature,
}

pub struct SourceLocation {
    pub line: u32,
    pub column: Option<u32>,
    pub file: Option<String>,
}

pub struct RenderError { ... }
```

## 4. Crate: Парсеры (на примере `route-graph-parser-caddy`)

Каждый crate парсера:

- Зависит **только** от `route-graph-core`
- Экспортирует одну публичную структуру, реализующую `Parser` + `FormatDetector`
- Содержит приватные модули для lexer/parser внутренних шагов
- Имеет `#[cfg(test)]` модуль с тестами на fixtures

```
route-graph-parser-caddy/
├── Cargo.toml
└── src/
    ├── lib.rs          # pub struct CaddyParser; impl Parser, impl FormatDetector
    ├── lexer.rs        # токенизация Caddyfile
    └── grammar.rs      # AST → RouteGraph
```

**Зависимости:** `route-graph-core`, `thiserror`, опционально `log`.

### Добавление нового парсера (checklist):

1. Создать `crates/route-graph-parser-<name>/`
2. Добавить в `workspace.members`
3. Реализовать `trait Parser` + `trait FormatDetector`
4. Добавить fixture-тесты
5. Подключить в CLI (feature flag)

## 5. Crate: Рендереры

Аналогичная структура. Каждый рендерер зависит от `route-graph-core`.

### Рендереры MVP:

| Crate | Выходной формат | Use case |
|-------|----------------|----------|
| `route-graph-renderer-dot` | Graphviz DOT | `dot -Tpng`, документация |
| `route-graph-renderer-mermaid` | Mermaid | Markdown, GitHub README |
| `route-graph-renderer-json` | JSON | Интеграция, веб-интерфейсы |

## 6. Crate: `route-graph-cli`

### CLI interface

```
route-graph [OPTIONS] <INPUT>

Arguments:
  <INPUT>  Path to config file, or "-" for stdin

Options:
  -f, --format <FORMAT>      Input format [caddy|nginx|tinyproxy|auto]
                             [default: auto]
  -r, --renderer <RENDERER>  Output format [dot|mermaid|json]
                             [default: mermaid]
  -o, --output <OUTPUT>      Output file [default: stdout]
      --direction <DIR>      Layout direction [tb|lr|bt|rl] [default: tb]
      --no-metadata          Hide metadata (ports, TLS, etc.)
      --collapse-middleware  Show middleware as single node
      --title <TITLE>        Graph title
      --list-formats         List available input formats and exit
      --list-renderers       List available renderers and exit
  -h, --help                 Print help
  -V, --version              Print version
```

### Архитектура CLI

```
CLI args (clap)
  │
  ▼
--format auto? ──Yes──▶ Try FormatDetector for each parser
  │                          │
  No                         │
  │                          │
  ▼                          ▼
Select parser by name ◀──────┘
  │
  ▼
parser.parse(input)
  │
  ▼
RouteGraph
  │
  ▼
Selected Renderer
  │
  ▼
renderer.render(graph)
  │
  ▼
Output (stdout / file)
```

CLI собирает парсеры через feature flags:

```toml
[features]
default = ["caddy", "nginx", "tinyproxy", "dot", "mermaid", "json"]
caddy = ["dep:route-graph-parser-caddy"]
nginx = ["dep:route-graph-parser-nginx"]
tinyproxy = ["dep:route-graph-parser-tinyproxy"]
dot = ["dep:route-graph-renderer-dot"]
mermaid = ["dep:route-graph-renderer-mermaid"]
json = ["dep:route-graph-renderer-json"]
```

`main.rs` содержит registry парсеров/рендереров:

```rust
fn build_parser_registry() -> Vec<Box<dyn Parser>> {
    let mut parsers: Vec<Box<dyn Parser>> = Vec::new();
    #[cfg(feature = "caddy")]
    parsers.push(Box::new(CaddyParser::new()));
    #[cfg(feature = "nginx")]
    parsers.push(Box::new(NginxParser::new()));
    #[cfg(feature = "tinyproxy")]
    parsers.push(Box::new(TinyProxyParser::new()));
    parsers
}
```

## 7. Модель данных — детальный разбор

### Поток запроса через модель

```
Client
  │
  ▼
Listener :443
  │
  ▼
Host example.com
  │
  ├──▶ PathMatch /api/*
  │      │
  │      ▼
  │    Middleware strip_prefix
  │      │
  │      ▼
  │    Middleware header_rewrite
  │      │
  │      ▼
  │    Backend http://backend:8080
  │
  └──▶ PathMatch /*
         │
         ▼
       Backend http://frontend:3000
```

Это прямо ложится в структуру: `Client → Listener → Host → PathMatch → [Middleware]* → Backend`.

### Node — минимальный набор полей

| Поле | Тип | Зачем |
|------|-----|-------|
| `id` | `NodeId` | Уникальный идентификатор (индекс в слайсе) |
| `kind` | `NodeKind` | Тип ноды — определяет рендеринг и валидацию |
| `label` | `CompactString` | Человекочитаемый текст (":443", "example.com") |
| `metadata` | `Metadata` | Опциональные данные (порт, TLS, протокол) |

### Edge — направленная связь

`Edge` содержит `source` и `target` (оба `NodeId`). Рендерер может добавить лейбл к edge через `condition`.

## 8. Публичные API (summary)

### `route-graph-core`

```rust
// Реэкспорты
pub use model::*;
pub use traits::*;
pub use error::*;
pub use builder::*;

// model
pub struct RouteGraph { ... }         // основная структура
pub struct Node { ... }
pub struct Edge { ... }
pub struct NodeId(u32);
pub enum NodeKind { Client, Listener, Host, PathMatch, Middleware, Backend }
pub struct Metadata { ... }
pub enum Protocol { Http, Https, Grpc, Tcp, Udp }
pub struct TlsConfig { ... }
pub enum EdgeCondition { ... }

// builder
pub struct RouteGraphBuilder { ... }
impl RouteGraph {
    pub fn builder() -> RouteGraphBuilder;
}

// traits
pub trait Parser { ... }
pub trait Renderer { ... }
pub trait FormatDetector { ... }

// config
pub struct RenderOptions { ... }
pub enum LayoutDirection { ... }
pub enum ColorScheme { ... }
pub enum DetectionConfidence { ... }

// errors
pub struct ParseError { ... }
pub struct RenderError { ... }
```

### `route-graph-parser-caddy` (и аналоги)

```rust
pub struct CaddyParser;
impl CaddyParser {
    pub fn new() -> Self;
}
impl Parser for CaddyParser { ... }
impl FormatDetector for CaddyParser { ... }
```

### `route-graph-renderer-dot` (и аналоги)

```rust
pub struct DotRenderer;
impl DotRenderer {
    pub fn new() -> Self;
}
impl Renderer for DotRenderer { ... }
```

## 9. Зависимости между crates

```
                    route-graph-core
                    (model + traits)
                    ┌───────┬───────┐
                    │       │       │
              ┌─────┘       │       └─────┐
              ▼             ▼             ▼
      parser-caddy    parser-nginx   parser-tinyproxy
              │             │             │
              └─────┬───────┘─────────────┘
                    │ (optional dep)
                    ▼
              route-graph-cli ──────────────────────
              │         │         │                 │
              ▼         ▼         ▼                 ▼
       renderer-dot  renderer-mermaid  renderer-json
```

Пунктир = optional dependency через feature flag. Парсеры и рендереры не знают друг о друге.

## 10. План реализации MVP

### Фаза 1: Foundation (неделя 1)

| # | Задача | Crate |
|---|--------|-------|
| 1 | Инициализация workspace, `Cargo.toml`, license файлы | root |
| 2 | Реализация модели данных: `RouteGraph`, `Node`, `Edge`, `Metadata` | core |
| 3 | Реализация `RouteGraphBuilder` | core |
| 4 | Определение трейтов `Parser`, `Renderer`, `FormatDetector` | core |
| 5 | Error types: `ParseError`, `RenderError` | core |
| 6 | Базовые unit-тесты модели (создание графа, обход) | core |

### Фаза 2: Первый парсер + рендерер (неделя 2)

| # | Задача | Crate |
|---|--------|-------|
| 7 | `CaddyParser` — лексер + парсер Caddyfile | parser-caddy |
| 8 | Fixture-тесты на реальных Caddyfile | parser-caddy |
| 9 | `DotRenderer` — генерация Graphviz DOT | renderer-dot |
| 10 | `MermaidRenderer` — генерация Mermaid | renderer-mermaid |
| 11 | Визуальная валидация на примерах | — |

### Фаза 3: CLI + ещё парсеры (неделя 3)

| # | Задача | Crate |
|---|--------|-------|
| 12 | CLI через `clap`, auto-detect формата | cli |
| 13 | `NginxParser` | parser-nginx |
| 14 | `TinyProxyParser` | parser-tinyproxy |
| 15 | `JsonRenderer` | renderer-json |
| 16 | Интеграционные тесты `tests/` | root |

### Фаза 4: Polish (неделя 4)

| # | Задача |
|---|--------|
| 17 | CI (GitHub Actions): test, clippy, fmt, deny |
| 18 | Документация: rustdoc + README |
| 19 | Публикация на crates.io |
| 20 | Примеры в `examples/` |

## 11. Roadmap развития

### v0.2 — Расширение парсеров

- Traefik (TOML/YAML динамическая конфигурация)
- Envoy (xDS / YAML)
- HAProxy

### v0.3 — Kubernetes

- Kubernetes Ingress (YAML)
- Kubernetes Gateway API (YAML)
- Поддержка multi-file конфигураций (директории с манифестами)

### v0.4 — Визуализация

- SVG/PNG рендерер (через Graphviz или resvg)
- Интерактивный HTML рендерер (D3.js / cytoscape.js)
- WebAssembly сборка для браузера

### v0.5 — Анализ

- Diff между конфигурациями (два файла → diff графа)
- Валидация: обнаружение конфликтующих маршрутов
- Dead route detection (unreachable backends)
- Мерж нескольких конфигураций в один граф

### v0.6 — Интеграции

- Library API стабилизация (1.0-ready)
- Python bindings (PyO3)
- GitHub Action для CI-визуализации
- VS Code extension

## 12. Ключевые архитектурные принципы

1. **Парсеры не зависят друг от друга** — добавление нового парсера не требует изменений в существующих
2. **Рендереры не зависят от парсеров** — через промежуточную модель `RouteGraph`
3. **Core не знает о конкретных парсерах/рендерерах** — только трейты
4. **CLI собирает всё через feature flags** — можно собрать минимальный бинарник
5. **`&str` на входе, `String` на выходе** — минимальные аллокации в парсерах
6. **`CompactString` для лейблов** — inline хранение до 24 байт, избегаем heap для коротких строк
7. **Builder для построения графа** — типобезопасная вложенность, парсер не может создать невалидный граф
8. **`NodeId = u32`** — вместо указателей, нет borrow checker проблем, cache-friendly
