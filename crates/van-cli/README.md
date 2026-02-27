# Van

[![crates.io](https://img.shields.io/crates/v/van)](https://crates.io/crates/van)

CLI toolchain for the [Van](https://github.com/vanengine/van) template engine — project scaffolding, dev server with hot reload, and static HTML generation.

## Install

```bash
cargo install van
```

Or download pre-built binaries from [GitHub Releases](https://github.com/vanengine/van/releases).

## Commands

```bash
van init [name]    # Create a new Van project
van dev            # Start dev server with hot reload
van generate       # Generate static HTML pages
```

## .van File Example

```html
<template>
  <div class="counter">
    <h1>{{ title }}</h1>
    <p>Count: {{ count }}</p>
    <button @click="increment">+1</button>
  </div>
</template>

<script setup>
const count = ref(0)
function increment() {
  count.value++
}
</script>

<style scoped>
.counter { padding: 2rem; }
button { cursor: pointer; }
</style>
```

Van uses Vue SFC syntax (`.van` files) for server-side HTML rendering with optional signal-based client-side interactivity. No Node.js runtime needed.

## License

MIT
