mod i18n;
mod resolve;
pub mod render;

use std::collections::HashMap;

pub use render::PageAssets;
pub use resolve::ResolvedComponent;
pub use resolve::resolve_single;
pub use resolve::resolve_with_files;
pub use resolve::resolve_with_files_debug;

// ── Compile (no data) ───────────────────────────────────────────
// Produces HTML with v-for/v-if/:class/{{ }} preserved for Java runtime.

/// Compile a multi-file `.van` project into HTML template (no data binding).
pub fn compile(
    entry_path: &str,
    files: &HashMap<String, String>,
) -> Result<String, String> {
    build_page(entry_path, files, None, false, &HashMap::new(), "Van")
}

/// Like `compile`, but with all options.
pub fn compile_full(
    entry_path: &str,
    files: &HashMap<String, String>,
    debug: bool,
    file_origins: &HashMap<String, String>,
    global_name: &str,
) -> Result<String, String> {
    build_page(entry_path, files, None, debug, file_origins, global_name)
}

/// Compile with separated assets (no data binding).
pub fn compile_assets(
    entry_path: &str,
    files: &HashMap<String, String>,
    asset_prefix: &str,
) -> Result<PageAssets, String> {
    build_page_assets(entry_path, files, None, asset_prefix, false, &HashMap::new(), "Van")
}

/// Like `compile_assets`, but with all options.
pub fn compile_assets_full(
    entry_path: &str,
    files: &HashMap<String, String>,
    asset_prefix: &str,
    debug: bool,
    file_origins: &HashMap<String, String>,
    global_name: &str,
) -> Result<PageAssets, String> {
    build_page_assets(entry_path, files, None, asset_prefix, debug, file_origins, global_name)
}

/// Compile a single `.van` file source (no data binding).
pub fn compile_single(source: &str) -> Result<String, String> {
    let mut files = HashMap::new();
    files.insert("main.van".to_string(), source.to_string());
    compile("main.van", &files)
}

// ── Render (with data) ─────────────────────────────────────────
// Compiles template AND binds data, producing final HTML.

/// Compile and render a multi-file `.van` project with data into final HTML.
pub fn render_to_string(
    entry_path: &str,
    files: &HashMap<String, String>,
    data_json: &str,
) -> Result<String, String> {
    build_page(entry_path, files, Some(data_json), false, &HashMap::new(), "Van")
}

/// Like `render_to_string`, but with debug HTML comments at component/slot boundaries.
pub fn render_to_string_debug(
    entry_path: &str,
    files: &HashMap<String, String>,
    data_json: &str,
    file_origins: &HashMap<String, String>,
) -> Result<String, String> {
    build_page(entry_path, files, Some(data_json), true, file_origins, "Van")
}

/// Like `render_to_string`, but with all options.
pub fn render_to_string_full(
    entry_path: &str,
    files: &HashMap<String, String>,
    data_json: &str,
    debug: bool,
    file_origins: &HashMap<String, String>,
    global_name: &str,
) -> Result<String, String> {
    build_page(entry_path, files, Some(data_json), debug, file_origins, global_name)
}

/// Render with separated assets.
pub fn render_to_assets(
    entry_path: &str,
    files: &HashMap<String, String>,
    data_json: &str,
    asset_prefix: &str,
) -> Result<PageAssets, String> {
    build_page_assets(entry_path, files, Some(data_json), asset_prefix, false, &HashMap::new(), "Van")
}

/// Like `render_to_assets`, but with all options.
pub fn render_to_assets_full(
    entry_path: &str,
    files: &HashMap<String, String>,
    data_json: &str,
    asset_prefix: &str,
    debug: bool,
    file_origins: &HashMap<String, String>,
    global_name: &str,
) -> Result<PageAssets, String> {
    build_page_assets(entry_path, files, Some(data_json), asset_prefix, debug, file_origins, global_name)
}

/// Render a single `.van` file source with data.
pub fn render_single(source: &str, data_json: &str) -> Result<String, String> {
    let mut files = HashMap::new();
    files.insert("main.van".to_string(), source.to_string());
    render_to_string("main.van", &files, data_json)
}

// ── Internal shared implementation ──────────────────────────────

fn build_page(
    entry_path: &str,
    files: &HashMap<String, String>,
    data_json: Option<&str>,
    debug: bool,
    file_origins: &HashMap<String, String>,
    global_name: &str,
) -> Result<String, String> {
    let compile = data_json.is_none();
    let json_str = data_json.unwrap_or("{}");
    let data: serde_json::Value = serde_json::from_str(json_str)
        .map_err(|e| format!("Invalid JSON: {e}"))?;
    let resolved = if debug {
        resolve::resolve_with_files_debug(entry_path, files, &data, file_origins)?
    } else {
        resolve::resolve_with_files(entry_path, files, &data)?
    };
    if compile {
        render::compile(&resolved, global_name)
    } else {
        render::render_to_string(&resolved, &data, global_name)
    }
}

fn build_page_assets(
    entry_path: &str,
    files: &HashMap<String, String>,
    data_json: Option<&str>,
    asset_prefix: &str,
    debug: bool,
    file_origins: &HashMap<String, String>,
    global_name: &str,
) -> Result<PageAssets, String> {
    let compile = data_json.is_none();
    let json_str = data_json.unwrap_or("{}");
    let data: serde_json::Value = serde_json::from_str(json_str)
        .map_err(|e| format!("Invalid JSON: {e}"))?;
    let resolved = if debug {
        resolve::resolve_with_files_debug(entry_path, files, &data, file_origins)?
    } else {
        resolve::resolve_with_files(entry_path, files, &data)?
    };

    let page_name = entry_path.trim_end_matches(".van");

    if compile {
        render::compile_assets(&resolved, page_name, asset_prefix, global_name)
    } else {
        render::render_to_assets(&resolved, &data, page_name, asset_prefix, global_name)
    }
}

#[cfg(feature = "wasm")]
use wasm_bindgen::prelude::*;

#[cfg(feature = "wasm")]
#[wasm_bindgen]
pub fn compile_van(
    entry_path: &str,
    files_json: &str,
    data_json: &str,
) -> Result<String, JsValue> {
    let files_value: serde_json::Value = serde_json::from_str(files_json)
        .map_err(|e| JsValue::from_str(&format!("Invalid files JSON: {e}")))?;

    let files_obj = files_value
        .as_object()
        .ok_or_else(|| JsValue::from_str("files_json must be a JSON object"))?;

    let mut files = HashMap::new();
    for (key, val) in files_obj {
        let content = val
            .as_str()
            .ok_or_else(|| JsValue::from_str(&format!("File '{}' content must be a string", key)))?;
        files.insert(key.clone(), content.to_string());
    }

    // WASM: treat empty string as "{}" for backward compat
    let data = if data_json.is_empty() { "{}" } else { data_json };
    render_to_string(entry_path, &files, data).map_err(|e| JsValue::from_str(&e))
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Render tests (with data) ──

    #[test]
    fn test_render_single_basic() {
        let source = r#"
<template>
  <h1>{{ title }}</h1>
</template>
"#;
        let data = r#"{"title": "Hello World"}"#;
        let html = render_single(source, data).unwrap();
        assert!(html.contains("<h1>Hello World</h1>"));
        assert!(html.contains("<!DOCTYPE html>"));
    }

    #[test]
    fn test_render_single_with_signals() {
        let source = r#"
<template>
  <div>
    <p>Count: {{ count }}</p>
    <button @click="increment">+1</button>
  </div>
</template>

<script setup>
const count = ref(0)
function increment() { count.value++ }
</script>
"#;
        let html = render_single(source, "{}").unwrap();
        assert!(html.contains("Van"));
        assert!(!html.contains("@click"));
        assert!(html.contains("effect"));
    }

    #[test]
    fn test_render_to_string_invalid_json() {
        let mut files = HashMap::new();
        files.insert("main.van".to_string(), "<template><p>Hi</p></template>".to_string());
        let result = render_to_string("main.van", &files, "not json");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid JSON"));
    }

    #[test]
    fn test_render_to_string_with_style() {
        let source = r#"
<template>
  <h1>Hello</h1>
</template>

<style>
h1 { color: blue; }
</style>
"#;
        let html = render_single(source, "{}").unwrap();
        assert!(html.contains("color: blue"));
    }

    #[test]
    fn test_render_to_string_multi_file() {
        let mut files = HashMap::new();
        files.insert(
            "index.van".to_string(),
            r#"
<template>
  <hello :name="title" />
</template>

<script setup>
import Hello from './hello.van'
</script>
"#
            .to_string(),
        );
        files.insert(
            "hello.van".to_string(),
            r#"
<template>
  <h1>Hello, {{ name }}!</h1>
</template>

<style>
h1 { color: green; }
</style>
"#
            .to_string(),
        );

        let data = r#"{"title": "Van"}"#;
        let html = render_to_string("index.van", &files, data).unwrap();
        assert!(html.contains("<h1>Hello, Van!</h1>"));
        assert!(html.contains("color: green"));
    }

    #[test]
    fn test_render_to_string_with_ts_import() {
        let mut files = HashMap::new();
        files.insert(
            "index.van".to_string(),
            r#"
<template>
  <div>
    <p>Count: {{ count }}</p>
    <button @click="increment">+1</button>
  </div>
</template>

<script setup lang="ts">
import { formatDate } from './utils/format.ts'
import type { User } from './types.ts'
const count = ref(0)
function increment() { count.value++ }
</script>
"#
            .to_string(),
        );
        files.insert(
            "utils/format.ts".to_string(),
            r#"function formatDate(d) { return d.toISOString(); }
return { formatDate: formatDate };"#
                .to_string(),
        );

        let html = render_to_string("index.van", &files, "{}").unwrap();
        assert!(html.contains("__mod_0"));
        assert!(html.contains("formatDate"));
        assert!(html.contains("Van"));
        assert!(html.contains("effect"));
        assert!(!html.contains("__mod_1"));
    }

    #[test]
    fn test_render_single_i18n_basic() {
        let source = r#"
<template>
  <h1>{{ $t('title') }}</h1>
  <p>{{ $t('greeting', { name: userName }) }}</p>
</template>
"#;
        let data = r#"{"userName": "Alice", "$i18n": {"title": "欢迎", "greeting": "你好，{name}！"}}"#;
        let html = render_single(source, data).unwrap();
        assert!(html.contains("<h1>欢迎</h1>"));
        assert!(html.contains("<p>你好，Alice！</p>"));
    }

    #[test]
    fn test_render_i18n_child_component_inherits() {
        let mut files = HashMap::new();
        files.insert(
            "index.van".to_string(),
            r#"
<template>
  <greeting :name="userName" />
</template>

<script setup>
import Greeting from './greeting.van'
</script>
"#
            .to_string(),
        );
        files.insert(
            "greeting.van".to_string(),
            r#"
<template>
  <p>{{ $t('hello') }}, {{ name }}!</p>
</template>
"#
            .to_string(),
        );

        let data = r#"{"userName": "Bob", "$i18n": {"hello": "你好"}}"#;
        let html = render_to_string("index.van", &files, data).unwrap();
        assert!(html.contains("你好, Bob!"));
    }

    #[test]
    fn test_render_i18n_prop_binding() {
        let mut files = HashMap::new();
        files.insert(
            "index.van".to_string(),
            r#"
<template>
  <img-comp :alt="$t('logo.alt')" />
</template>

<script setup>
import ImgComp from './img-comp.van'
</script>
"#
            .to_string(),
        );
        files.insert(
            "img-comp.van".to_string(),
            r#"
<template>
  <img alt="{{ alt }}" />
</template>
"#
            .to_string(),
        );

        let data = r#"{"$i18n": {"logo": {"alt": "Logo图片"}}}"#;
        let html = render_to_string("index.van", &files, data).unwrap();
        assert!(html.contains("Logo图片"));
    }

    #[test]
    fn test_render_to_string_type_only_import_erased() {
        let mut files = HashMap::new();
        files.insert(
            "index.van".to_string(),
            r#"
<template>
  <div><p>{{ count }}</p></div>
</template>

<script setup lang="ts">
import type { Config } from './config.ts'
const count = ref(0)
</script>
"#
            .to_string(),
        );

        let html = render_to_string("index.van", &files, "{}").unwrap();
        assert!(!html.contains("__mod_"));
        assert!(html.contains("V.signal(0)"));
    }

    // ── Compile tests (no data) ──

    #[test]
    fn test_compile_preserves_v_for() {
        let source = r#"
<template>
  <ul><li v-for="item in items">{{ item.name }}</li></ul>
</template>
"#;
        let html = compile_single(source).unwrap();
        assert!(html.contains("v-for=\"item in items\""), "v-for should be preserved in compile mode");
        assert!(html.contains("{{item.name}}"), "{{ }} should be preserved in compile mode");
    }

    #[test]
    fn test_compile_preserves_v_if() {
        let source = r#"
<template>
  <div v-if="visible">content</div>
</template>
"#;
        let html = compile_single(source).unwrap();
        assert!(html.contains("v-if=\"visible\""), "v-if should be preserved in compile mode");
    }

    #[test]
    fn test_compile_preserves_class_binding() {
        let source = r#"
<template>
  <span :class="{ 'active': isActive }">text</span>
</template>
"#;
        let html = compile_single(source).unwrap();
        assert!(html.contains(":class="), ":class should be preserved in compile mode");
    }

    #[test]
    fn test_compile_strips_click() {
        let source = r#"
<template>
  <button @click="handler">text</button>
</template>
"#;
        let html = compile_single(source).unwrap();
        assert!(!html.contains("@click"), "@click should be stripped in compile mode");
    }
}

#[cfg(test)]
mod layout_html_test {
    use super::*;

    #[test]
    fn test_layout_html_propagates() {
        let mut files = HashMap::new();
        files.insert("pages/index.van".to_string(), r#"
<script setup>
import Layout from '../components/Layout.van'
</script>
<template>
  <Layout title="Dashboard">
    <h1>Hello</h1>
  </Layout>
</template>
"#.to_string());

        files.insert("components/Layout.van".to_string(), r#"
<script setup>
defineProps({ title: String })
</script>
<template>
  <html lang="en">
  <head>
    <title>{{ title }}</title>
    <link rel="stylesheet" href="/style.css" />
  </head>
  <body>
    <slot />
  </body>
  </html>
</template>
"#.to_string());

        let result = render_to_string("pages/index.van", &files, "{}").unwrap();
        assert!(result.contains("<html"), "Output should contain <html tag from Layout. Got:\n{}", result);
        assert!(result.contains("/style.css"), "Output should contain CSS link from Layout. Got:\n{}", result);
        assert!(!result.contains("Van Playground"), "Output should NOT use default shell. Got:\n{}", result);
    }
}
