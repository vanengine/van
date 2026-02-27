# van-compiler

[![crates.io](https://img.shields.io/crates/v/van-compiler)](https://crates.io/crates/van-compiler)

Server-side HTML compiler for the Van template engine — orchestrates parsing, import resolution, rendering, and client JS generation from `.van` files.

Part of the [Van](https://github.com/vanengine/van) template engine.

## Usage

```rust
use std::collections::HashMap;
use van_compiler::{compile_page, compile_single};

// Single file compilation
let html = compile_single(
    r#"<template><h1>{{ title }}</h1></template>"#,
    r#"{"title": "Hello"}"#,
)?;

// Multi-file project compilation
let mut files = HashMap::new();
files.insert("pages/index.van".into(), template_source.into());
files.insert("components/header.van".into(), header_source.into());

let html = compile_page("pages/index.van", &files, &data_json)?;
```

## Compilation Pipeline

```
.van files → van-parser (parse)
               → resolve (recursive import resolution, max depth 10)
                 → render (server HTML with {{ expr }} placeholders)
                   → van-signal-gen (client JS)
                     → inject CSS/JS → final HTML
```

## API

| Function | Description |
|---|---|
| `compile_page(entry, files, data_json)` | Compile multi-file project → HTML string |
| `compile_page_debug(entry, files, data_json, origins)` | Same with debug comments at boundaries |
| `compile_page_assets(entry, files, data_json, prefix)` | Compile with separated CSS/JS assets |
| `compile_page_assets_debug(...)` | Same with debug comments |
| `compile_single(source, data_json)` | Compile a single `.van` string |

All functions return `Result<T, String>` for WASM compatibility.

## License

MIT
