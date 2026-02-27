# van-parser

[![crates.io](https://img.shields.io/crates/v/van-parser)](https://crates.io/crates/van-parser)

Recursive descent parser for `.van` files (Vue SFC syntax) — parses `<template>`, `<script setup>`, `<style scoped>` blocks into a `VanBlock` struct.

Part of the [Van](https://github.com/vanengine/van) template engine.

## Usage

```rust
use van_parser::{parse_blocks, parse_imports, VanBlock, VanImport, PropDef};

let source = r#"
<template>
  <div>{{ message }}</div>
</template>

<script setup>
import Header from './header.van'
const message = ref('hello')
</script>

<style scoped>
div { color: red; }
</style>
"#;

let block: VanBlock = parse_blocks(source);
assert!(block.template.is_some());
assert!(block.script_setup.is_some());
assert!(block.style_scoped);
```

## Key Types

- **`VanBlock`** — parsed `.van` file: `template`, `script_setup`, `script_server`, `style`, `style_scoped`, `props`
- **`PropDef`** — component prop definition: `name`, `prop_type`, `required`
- **`VanImport`** — `.van` component import: `name` (PascalCase), `tag_name` (kebab-case), `path`
- **`ScriptImport`** — non-component import: `raw`, `is_type_only`, `path`

## Public API

| Function | Description |
|---|---|
| `parse_blocks(source)` | Parse `.van` source into `VanBlock` |
| `parse_imports(script)` | Extract `.van` component imports |
| `parse_script_imports(script)` | Extract `.ts`/`.js` imports |
| `parse_define_props(script)` | Extract `defineProps()` declarations |
| `scope_css(css, id)` | Add scoped class to CSS selectors |
| `add_scope_class(html, id)` | Add scoped class to HTML elements |

## License

MIT
