# van-compiler

[![crates.io](https://img.shields.io/crates/v/van-compiler)](https://crates.io/crates/van-compiler)

Server-side HTML compiler for the Van template engine — orchestrates parsing, import resolution, rendering, and client JS generation from `.van` files.

Part of the [Van](https://github.com/vanengine/van) template engine.

## Usage

```rust
use std::collections::HashMap;
use van_compiler::{compile, compile_single, render_to_string, render_single};

// Compile mode (no data) — preserves {{ }}, v-for, v-if for Java runtime
let html = compile_single(r#"<template><h1>{{ title }}</h1></template>"#)?;

// Render mode (with data) — binds data, produces final HTML
let html = render_single(
    r#"<template><h1>{{ title }}</h1></template>"#,
    r#"{"title": "Hello"}"#,
)?;

// Multi-file project
let mut files = HashMap::new();
files.insert("pages/index.van".into(), template_source.into());
files.insert("components/header.van".into(), header_source.into());

let html = render_to_string("pages/index.van", &files, &data_json)?;
```

## Compilation Pipeline

```
.van files → van-parser (parse)
               → resolve (recursive import resolution, max depth 10)
                 → compile or render (server HTML)
                   → van-signal-gen (client JS)
                     → inject CSS/JS → final HTML
```

## API

### Compile (no data — template for Java runtime)

| Function | Description |
|---|---|
| `compile(entry, files)` | Compile multi-file project → HTML template |
| `compile_full(entry, files, debug, origins, global)` | Same with all options |
| `compile_assets(entry, files, prefix)` | Compile with separated CSS/JS assets |
| `compile_assets_full(...)` | Same with all options |
| `compile_single(source)` | Compile a single `.van` string |

### Render (with data — final HTML)

| Function | Description |
|---|---|
| `render_to_string(entry, files, data_json)` | Render multi-file project → HTML string |
| `render_to_string_debug(entry, files, data_json, origins)` | Same with debug comments at boundaries |
| `render_to_string_full(...)` | Same with all options |
| `render_to_assets(entry, files, data_json, prefix)` | Render with separated CSS/JS assets |
| `render_to_assets_full(...)` | Same with all options |
| `render_single(source, data_json)` | Render a single `.van` string |

All functions return `Result<T, String>` for WASM compatibility.

## License

MIT
