# Van

A language-agnostic template rendering engine that uses Vue SFC syntax (`.van` files) for server-side HTML rendering with optional signal-based client-side interactivity.

- **Vue SFC syntax** — Write templates using familiar `<template>`, `<script setup>`, and `<style scoped>` blocks
- **No Node.js required** — Core is written in Rust, compiles to WASM for backend integration
- **Signal-based reactivity** — Lightweight client-side interactivity (~4KB runtime, direct DOM ops, no virtual DOM)
- **Framework-agnostic** — WASM compiler integrates with any backend via JSON stdin/stdout protocol

## Example `.van` File

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

## Build from Source

```bash
# Build all crates
cargo build --release

# Build WASM binary (for framework integration)
cargo build --target wasm32-wasip1 -p van-compiler-wasi --release

# Run tests
cargo test
```

## Project Structure

| Crate | Purpose |
|---|---|
| `van-parser` | Recursive descent parser for `.van` files |
| `van-compiler` | Orchestrates server HTML + client JS compilation |
| `van-compiler-wasi` | WASM entry point (JSON stdin/stdout) |
| `van-signal-gen` | `<script setup>` → signal-based DOM JS |

> **Note:** The CLI toolchain (`van-cli`, `van-core`, `van-dev-server`, `van-registry`) has been moved to the [van-cli](https://github.com/vanengine/van-cli) repository.

## Framework Adapters

- [van-spring-boot-starter](https://github.com/vanengine/van-spring-boot-starter) — Spring Boot integration

## Specification

See [`spec/v0.1.md`](spec/v0.1.md) for the full language specification.

## License

[MIT](LICENSE)
