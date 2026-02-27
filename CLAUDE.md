# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What is Van?

A language-agnostic template rendering engine that uses Vue SFC syntax (`.van` files) for server-side HTML rendering with optional signal-based client-side interactivity. The core is written in Rust, compiles to WASM for backend integration. No Node.js runtime is needed in production.

## Build & Run Commands

```bash
# Build all Rust crates
cargo build --release

# Build CLI binary
cargo build --release -p van-cli

# Build WASM binary (for framework integration)
cargo build --target wasm32-wasip1 -p van-compiler-wasi --release
```

## Testing

All tests are inline `#[cfg(test)]` modules (no separate `tests/` directories).

```bash
cargo test                              # run all workspace tests
cargo test -p van-parser                # parser tests only (39 tests)
cargo test -p van-signal-gen            # signal generation tests (33 tests)
cargo test -p van-compiler              # compiler tests (43 tests across lib.rs, resolve.rs, render.rs)
cargo test test_parse_blocks_basic      # run a single test by name
```

No custom rustfmt, clippy, or toolchain configuration — use defaults.

## Project Structure

Cargo workspace with 8 crates (version managed at workspace level in root `Cargo.toml`):

**Core Engine** (`crates/`):

| Crate | Purpose |
|---|---|
| `van-parser` | Hand-written recursive descent parser for `.van` files |
| `van-compiler` | Orchestrates server HTML + client JS compilation |
| `van-compiler-wasi` | WASM entry point (JSON stdin/stdout protocol) |
| `van-signal-gen` | Compiles `<script setup>` → signal-based direct DOM JS (~4KB runtime) |

**CLI Toolchain** (`crates/van-cli/`):

| Crate | Purpose |
|---|---|
| `van-cli` | CLI binary (`van init`, `van dev`, `van generate`) |
| `van-context` | Project context and configuration |
| `van-dev` | Dev server with hot reload |
| `van-init` | Project scaffolding |

## Compilation Pipeline

```
.van file → [van-parser] → VanBlock
                              ├── [van-compiler] → Server HTML with {{ expr }}
                              └── [van-signal-gen] → Signal-based JS (direct DOM ops, no virtual DOM)
```

Two compilation modes in `van-compiler`: `compile_page()` (inline assets) and `compile_page_assets()` (separate JS/CSS with asset hashing). Both have `_debug()` variants that add HTML comments at component/slot boundaries.

**Internal call chain:** `compile_page()` → `resolve::resolve_with_files()` (recursive import resolution, max depth 10) → `render::render_page()` → `van_signal_gen::generate_signals()` → inject CSS/JS into HTML.

Additional entry points: `compile_single()` for single-file compilation without a files map, `compile_van()` as a wasm-bindgen export (`#[cfg(feature = "wasm")]`).

## Error Handling Patterns

- **Library/WASM crates** (`van-parser`, `van-compiler`, `van-signal-gen`): use `Result<T, String>` or `Option<T>` — zero external error dependencies to keep WASM-compatible
- **CLI crates** (`van-cli`, `van-context`, `van-dev`, `van-init`): use `anyhow::Result` for ergonomic error handling

## Key Types (van-parser)

- `VanBlock` — parsed `.van` file: `template: Option<String>`, `script_setup: Option<String>`, `script_server: Option<String>`, `style: Option<String>`, `style_scoped: bool`, `props: Vec<PropDef>`
- `PropDef` — component prop: `name`, `prop_type: Option<String>`, `required: bool`
- `VanImport` — component import: `name` (PascalCase), `tag_name` (kebab-case), `path`
- `ScriptImport` — non-component import: `raw`, `is_type_only: bool`, `path`

## WASM Integration

The WASM compiler (`van-compiler-wasi`) receives JSON via stdin: `{ entry_path, files, data_json, asset_prefix, debug, file_origins }` and returns `{ ok, html?, assets?, error? }`. When `asset_prefix` is provided, CSS/JS are emitted as separate assets. Host frameworks perform a second pass to interpolate `{{ expr }}` with server-side model data.

Two execution modes:
- **Single-shot** (default): reads all stdin, compiles once, writes response
- **Daemon** (`--daemon` flag): reads one JSON object per line (JSON Lines), compiles each, writes response per line — stays alive until stdin EOF

## Key Conventions

- `.van` files follow Vue 3 SFC syntax: `<template>`, `<script setup>`, `<style scoped>`
- PascalCase imports → kebab-case in templates (`UserCard` → `<user-card />`)
- Page data lives in `data/index.json`, keyed by page path (e.g., `"pages/index"`)
- Theme inheritance via `theme.json` in `van.themes/` directory
- Signal primitives in `<script setup>`: `ref()`, `computed()`, `watch()`
- Supported directives: `@click`, `v-show`, `v-if`, `v-html`, `v-text`, `:class`
- Slots: `<slot>` and named `<slot name="...">` in layout components
- `defineProps({ name: String })` for prop declarations
- Dev server runs on port 3000 by default; watches `src/` and `data/` for `.van`, `.json`, `.css` changes
- Static generation: `index.van` → `dist/index.html`, `other.van` → `dist/other/index.html`

## CI/CD

Two workflows, both triggered by pushes to `Cargo.toml` or `crates/**`:

- `.github/workflows/release.yml` — pushes to `main`: version check → create git tag → build WASM + native binaries + CLI binaries (linux x64/arm64, macOS x64/arm64, Windows x64) → GitHub Release → publish to crates.io → dispatch to `van-spring-boot-starter`
- `.github/workflows/dev-release.yml` — pushes to `dev`: build WASM + native binaries + CLI binaries → GitHub pre-release (version: `{base}-dev.{run_number}`) → dispatch to `van-spring-boot-starter`
