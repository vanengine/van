# van-init

Project scaffolding for the Van template engine.

Part of the [Van](https://github.com/vanengine/van) template engine.

## Overview

Provides the `van init` command implementation — interactive project creation that generates:

- Project directory structure (`pages/`, `components/`, `layouts/`)
- `package.json` with Van configuration
- Starter `.van` template files
- Mock data (`mock/index.json`)

```rust
// Used internally by the van CLI
van_init::run(Some("my-project".into()))?;
```

## License

MIT
