# van-context

[![crates.io](https://img.shields.io/crates/v/van-context)](https://crates.io/crates/van-context)

Project context and configuration reader for the Van template engine CLI.

Part of the [Van](https://github.com/vanengine/van) template engine.

## Overview

Provides `VanProject` and `VanConfig` types used by the CLI toolchain (`van dev`, `van generate`) to discover and load Van project settings.

```rust
use van_context::project::VanProject;

let project = VanProject::load(std::path::Path::new("."))?;
// project.root — project root directory
// project.config — parsed van.config.json / package.json settings
```

## Key Types

- **`VanProject`** — loaded project handle: `root` (PathBuf) + `config` (VanConfig)
- **`VanConfig`** — project configuration: source directories, output paths, theme settings

## License

MIT
