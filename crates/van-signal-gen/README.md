# van-signal-gen

Compiles `<script setup>` reactive code into signal-based JavaScript that performs direct DOM operations — no virtual DOM, no framework runtime (~4KB).

Part of the [Van](https://github.com/vanengine/van) template engine.

## Usage

```rust
use van_signal_gen::generate_signals;

let script = r#"
const count = ref(0)
function increment() { count.value++ }
"#;
let template = r#"<button @click="increment">{{ count }}</button>"#;

if let Some(js) = generate_signals(script, template, &[]) {
    // js contains signal runtime + direct DOM update code
    println!("{js}");
}
```

## Key Functions

| Function | Description |
|---|---|
| `generate_signals(script, template, modules)` | Generate client-side JS from script setup + template |
| `extract_initial_values(script)` | Extract `ref()` initial values for SSR interpolation |
| `analyze_script(script)` | Analyze signals, computed, watchers in script |
| `walk_template(html, reactive_names)` | Find reactive bindings in template HTML |

## How It Works

1. **Analyze** `<script setup>` — identify `ref()` signals, `computed()`, `watch()`, functions
2. **Walk** template HTML — find `{{ expr }}`, `:attr`, `@event`, `v-if`/`v-for` bindings
3. **Generate** JS that wires signals to DOM nodes via direct `element.textContent` / `setAttribute` calls

## License

MIT
