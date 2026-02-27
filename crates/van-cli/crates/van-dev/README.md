# van-dev

[![crates.io](https://img.shields.io/crates/v/van-dev)](https://crates.io/crates/van-dev)

Development server with hot reload for the Van template engine.

Part of the [Van](https://github.com/vanengine/van) template engine.

## Overview

Provides the `van dev` command implementation — an Axum-based HTTP server that:

- Serves compiled `.van` pages on the fly
- Watches for file changes via `notify`
- Pushes hot reload updates to the browser via WebSocket

```rust
// Used internally by the van CLI
van_dev::start(3000).await?;
```

## License

MIT
