<h1 align="center">Van</h1>

<p align="center">
  <strong>Language-agnostic template engine with Vue SFC syntax</strong><br>
  Server-side HTML rendering · Signal-based client interactivity · WASM-powered
</p>

<p align="center">
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="License" /></a>
  <a href="https://github.com/vanengine/van/releases"><img src="https://img.shields.io/github/v/release/vanengine/van?include_prereleases" alt="Release" /></a>
  <a href="https://crates.io/crates/van-compiler"><img src="https://img.shields.io/crates/v/van-compiler" alt="Crates.io" /></a>
  <img src="https://img.shields.io/badge/platforms-linux%20%7C%20macOS%20%7C%20windows-lightgrey" alt="Platforms" />
</p>

<p align="center">
  <a href="#features">Features</a> ·
  <a href="#example">Example</a> ·
  <a href="#architecture">Architecture</a> ·
  <a href="#build-from-source">Build</a> ·
  <a href="#wasm-integration">WASM Integration</a>
</p>

<p align="center">
  🌐 <a href="README.md">English</a> · <a href="docs/i18n/zh-CN/README.md">简体中文</a>
</p>

---

## Features

- **Vue SFC Syntax** — Write templates with familiar `<template>`, `<script setup>`, `<style scoped>` blocks
- **Zero Node.js Dependency** — Core written in Rust, compiles to WASM for backend integration
- **Signal-based Reactivity** — Lightweight client-side interactivity with direct DOM updates (~4KB runtime, no virtual DOM)
- **Framework-agnostic** — WASM compiler integrates with any backend via JSON stdin/stdout protocol
- **Cross-platform** — Pre-built WASM + native binaries for Linux x64/ARM64, macOS x64/ARM64, and Windows x64

## Example

```vue
<template>
  <h1>{{ title }}</h1>
  <button @click="count++">Clicked {{ count }} times</button>
</template>

<script setup>
let count = 0
</script>

<style scoped>
h1 { color: steelblue; }
</style>
```

Server-side `{{ title }}` is interpolated by the host framework; `count` becomes a reactive signal with automatic DOM updates on the client.

## Architecture

```
.van file → [van-parser] → VanBlock
                              ├── [van-compiler] → Server HTML with {{ expr }}
                              └── [van-signal-gen] → Signal-based JS (direct DOM ops)
```

| Crate | Purpose |
|---|---|
| `van-parser` | Hand-written recursive descent parser for `.van` files |
| `van-compiler` | Orchestrates server HTML + client JS compilation |
| `van-compiler-wasi` | WASM entry point (JSON stdin/stdout protocol) |
| `van-signal-gen` | `<script setup>` → signal-based direct DOM JS |

> **Note:** The CLI toolchain (`van init`, `van dev`, `van build`, `van generate`) lives in the [van-cli](https://github.com/vanengine/van-cli) repository.

## Build from Source

Prerequisites: [Rust toolchain](https://rustup.rs/) (1.70+)

```bash
# Build all crates
cargo build --release

# Build WASM binary (for framework integration)
cargo build --target wasm32-wasip1 -p van-compiler-wasi --release

# Run tests
cargo test
```

## WASM Integration

The WASM compiler receives JSON via stdin and returns compiled HTML:

```jsonc
// Input
{ "entry_path": "pages/index.van", "files": { ... }, "mock_data_json": "..." }

// Output
{ "ok": true, "html": "<h1>{{ title }}</h1>..." }
```

Two execution modes:

- **Single-shot** (default) — reads stdin, compiles once, writes response
- **Daemon** (`--daemon`) — JSON Lines protocol, stays alive until stdin EOF

Host frameworks perform a second pass to interpolate `{{ expr }}` with server-side model data.

## Related

- [**Van CLI**](https://github.com/vanengine/van-cli) — Command-line toolchain (scaffolding, dev server, build, static generation)
- [**van-spring-boot-starter**](https://github.com/vanengine/van-spring-boot-starter) — Spring Boot integration

## License

[MIT](LICENSE)
