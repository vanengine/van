<h1 align="center">Van</h1>

<p align="center">
  <strong>Language-agnostic template engine with Vue syntax</strong><br>
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
  <a href="#installation">Installation</a> ·
  <a href="#usage">Usage</a> ·
  <a href="#example">Example</a>
</p>

<p align="center">
  🌐 <a href="README.md">English</a> · <a href="docs/i18n/zh-CN/README.md">简体中文</a>
</p>

---

## Features

- **Vue Syntax** — Write templates with familiar `<template>`, `<script setup>`, `<style scoped>` blocks
- **Signal-based Reactivity** — Lightweight client-side interactivity with direct DOM updates (~4KB runtime)

## Installation

**One-line install** (Linux / macOS):

```bash
curl -fsSL https://raw.githubusercontent.com/vanengine/van/main/install.sh | sh
```

**Manual download**: grab the latest `van-*` binary from [GitHub Releases](https://github.com/vanengine/van/releases) and place it in your `PATH`.

## Usage

```bash
van init my-project        # Scaffold a new Van project
van dev                    # Start dev server with hot reload
van generate               # Static site generation
```

### Framework Integration

Van compiles `.van` files to HTML via a WASM binary — integrate with multiple backends:

- **Spring Boot** — [van-spring-boot-starter](https://github.com/van-java/van-spring-boot-starter)

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

## Related

- [**van-spring-boot-starter**](https://github.com/van-java/van-spring-boot-starter) — Spring Boot integration

## License

[MIT](LICENSE)
